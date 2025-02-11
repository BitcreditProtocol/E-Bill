pub mod backup_service;
pub mod bill_service;
pub mod company_service;
pub mod contact_service;
pub mod file_upload_service;
pub mod identity_service;
pub mod notification_service;
pub mod search_service;

use super::{dht::Client, Config};
use crate::external::bitcoin::BitcoinClient;
use crate::persistence::db::SurrealDbConfig;
use crate::persistence::DbContext;
use crate::web::ErrorResponse;
use crate::{blockchain, dht, external};
use crate::{persistence, util};
use backup_service::{BackupService, BackupServiceApi};
use bill_service::{BillService, BillServiceApi};
use company_service::{CompanyService, CompanyServiceApi};
use contact_service::{ContactService, ContactServiceApi};
use file_upload_service::{FileUploadService, FileUploadServiceApi};
use identity_service::{IdentityService, IdentityServiceApi};
use log::error;
use notification_service::push_notification::{PushApi, PushService};
use notification_service::{
    create_nostr_client, create_nostr_consumer, create_notification_service, NostrConsumer,
    NotificationServiceApi,
};
use rocket::http::ContentType;
use rocket::Response;
use rocket::{http::Status, response::Responder};
use search_service::{SearchService, SearchServiceApi};
use std::io::Cursor;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, watch, RwLock};

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// all errors originating from the persistence layer
    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),

    /// errors that currently return early http status code Status::NotAcceptable
    #[error("not acceptable")]
    PreconditionFailed,

    /// errors that currently return early http status code Status::NotFound
    #[error("not found")]
    NotFound,

    /// errors stemming from sending or receiving notifications
    #[error("Notification service error: {0}")]
    NotificationService(#[from] notification_service::Error),

    /// errors stemming from handling bills
    #[error("Bill service error: {0}")]
    BillService(#[from] bill_service::Error),

    /// errors stemming from crypto utils
    #[error("Crypto util error: {0}")]
    CryptoUtil(#[from] util::crypto::Error),

    /// errors that stem from interacting with the Dht
    #[error("Dht error: {0}")]
    Dht(#[from] dht::Error),

    /// errors that stem from validation
    #[error("Validation Error: {0}")]
    Validation(String),

    #[error("External API error: {0}")]
    ExternalApi(#[from] external::Error),

    /// errors that stem from interacting with a blockchain
    #[error("Blockchain error: {0}")]
    Blockchain(#[from] blockchain::Error),

    /// std io
    #[error("Io error: {0}")]
    Io(#[from] std::io::Error),

    /// error returned if the given file upload id is not a temp file we have
    #[error("No file found for file upload id")]
    NoFileForFileUploadId,
}

/// Map from service errors directly to rocket status codes. This allows us to
/// write handlers that return `Result<T, service::Error>` and still return the correct
/// status code.
impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            Error::NoFileForFileUploadId => {
                let body =
                    ErrorResponse::new("bad_request", self.to_string(), 400).to_json_string();
                Response::build()
                    .status(Status::BadRequest)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            // for now, DHT errors are InternalServerError
            Error::Dht(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::CryptoUtil(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            // for now handle all persistence errors as InternalServerError, there
            // will be cases where we want to handle them differently (eg. 409 Conflict)
            Error::Persistence(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Blockchain(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::PreconditionFailed => Status::NotAcceptable.respond_to(req),
            Error::NotFound => {
                let body =
                    ErrorResponse::new("not_found", "not found".to_string(), 404).to_json_string();
                Response::build()
                    .status(Status::NotFound)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            Error::NotificationService(_) => Status::InternalServerError.respond_to(req),
            Error::BillService(e) => {
                error!("{e}");
                e.respond_to(req)
            }
            Error::Validation(msg) => build_validation_response(msg),
            // If an external API errors, we can only tell the caller that something went wrong on
            // our end
            Error::ExternalApi(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Io(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
        }
    }
}

fn build_validation_response<'o>(msg: String) -> rocket::response::Result<'o> {
    let err_resp = ErrorResponse::new("validation_error", msg, 400);
    let body = err_resp.to_json_string();
    Response::build()
        .status(Status::BadRequest)
        .header(ContentType::JSON)
        .sized_body(body.len(), Cursor::new(body))
        .ok()
}

/// A dependency container for all services that are used by the application
#[derive(Clone)]
pub struct ServiceContext {
    pub config: Config,
    dht_client: Client,
    pub contact_service: Arc<dyn ContactServiceApi>,
    pub search_service: Arc<dyn SearchServiceApi>,
    pub bill_service: Arc<dyn BillServiceApi>,
    pub identity_service: Arc<dyn IdentityServiceApi>,
    pub company_service: Arc<dyn CompanyServiceApi>,
    pub file_upload_service: Arc<dyn FileUploadServiceApi>,
    pub nostr_consumer: NostrConsumer,
    pub shutdown_sender: broadcast::Sender<bool>,
    pub notification_service: Arc<dyn NotificationServiceApi>,
    pub push_service: Arc<dyn PushApi>,
    pub current_identity: Arc<RwLock<SwitchIdentityState>>,
    pub backup_service: Arc<dyn BackupServiceApi>,
}

/// A structure describing the currently selected identity between the personal and multiple
/// possible company identities
#[derive(Clone, Debug)]
pub struct SwitchIdentityState {
    pub personal: String,
    pub company: Option<String>,
}

impl ServiceContext {
    /// returns an owned instance of the dht client
    pub fn dht_client(&self) -> Client {
        self.dht_client.clone()
    }

    /// sends a shutdown event to all parts of the application
    pub fn shutdown(&self) {
        if let Err(e) = self.shutdown_sender.send(true) {
            error!("Error sending shutdown event: {e}");
        }
    }

    pub async fn get_current_identity(&self) -> SwitchIdentityState {
        self.current_identity.read().await.clone()
    }

    pub async fn set_current_personal_identity(&self, node_id: String) {
        let mut current_identity = self.current_identity.write().await;
        current_identity.personal = node_id;
        current_identity.company = None;
    }

    pub async fn set_current_company_identity(&self, node_id: String) {
        let mut current_identity = self.current_identity.write().await;
        current_identity.company = Some(node_id);
    }
}

/// building up the service context dependencies here for now. Later we can modularize this
/// and make it more flexible.
pub async fn create_service_context(
    local_node_id: &str,
    config: Config,
    client: Client,
    shutdown_sender: broadcast::Sender<bool>,
    db: DbContext,
    reboot_sender: watch::Sender<bool>,
) -> Result<ServiceContext> {
    let contact_service = Arc::new(ContactService::new(
        db.contact_store.clone(),
        db.file_upload_store.clone(),
        db.identity_store.clone(),
    ));
    let bitcoin_client = Arc::new(BitcoinClient::new());

    let nostr_client = create_nostr_client(&config, db.identity_store.clone()).await?;
    let notification_service =
        create_notification_service(nostr_client.clone(), db.notification_store.clone()).await?;

    let bill_service = Arc::new(BillService::new(
        client.clone(),
        db.bill_store,
        db.bill_blockchain_store.clone(),
        db.identity_store.clone(),
        db.file_upload_store.clone(),
        bitcoin_client,
        notification_service.clone(),
        db.identity_chain_store.clone(),
        db.company_chain_store.clone(),
        db.contact_store.clone(),
        db.company_store.clone(),
    ));
    let identity_service = IdentityService::new(
        db.identity_store.clone(),
        db.file_upload_store.clone(),
        db.identity_chain_store.clone(),
    );

    let company_service = CompanyService::new(
        db.company_store,
        db.file_upload_store.clone(),
        db.identity_store.clone(),
        db.contact_store,
        db.identity_chain_store,
        db.company_chain_store,
    );
    let file_upload_service = FileUploadService::new(db.file_upload_store);

    let push_service = Arc::new(PushService::new());

    let nostr_consumer = create_nostr_consumer(
        nostr_client,
        contact_service.clone(),
        db.nostr_event_offset_store.clone(),
        db.notification_store.clone(),
        push_service.clone(),
    )
    .await?;

    let search_service = SearchService::new(
        bill_service.clone(),
        contact_service.clone(),
        Arc::new(company_service.clone()),
    );

    let backup_service = BackupService::new(
        db.backup_store.clone(),
        db.identity_store.clone(),
        SurrealDbConfig::new(&config.surreal_db_connection),
        reboot_sender.clone(),
    );

    Ok(ServiceContext {
        config,
        dht_client: client,
        contact_service,
        search_service: Arc::new(search_service),
        bill_service,
        identity_service: Arc::new(identity_service),
        company_service: Arc::new(company_service),
        file_upload_service: Arc::new(file_upload_service),
        nostr_consumer,
        shutdown_sender,
        notification_service,
        push_service,
        current_identity: Arc::new(RwLock::new(SwitchIdentityState {
            personal: local_node_id.to_owned(),
            company: None,
        })),
        backup_service: Arc::new(backup_service),
    })
}
