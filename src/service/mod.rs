pub mod contact_service;

use std::sync::Arc;

use rocket::{http::Status, response::Responder};
use thiserror::Error;

use super::{Client, Config};
use crate::persistence;
use crate::persistence::FileBasedContactStore;
use contact_service::{ContactService, ContactServiceApi};

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
}

/// Map from service errors directly to rocket status codes. This allows us to
/// write handlers that return `Result<T, service::Error>` and still return the correct
/// status code.
impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            // for now handle all persistence errors as InternalServerError, there
            // will be cases where we want to handle them differently (eg. 409 Conflict)
            Error::Persistence(_) => Status::InternalServerError.respond_to(req),
            Error::PreconditionFailed => Status::NotAcceptable.respond_to(req),
        }
    }
}

/// A depencency container for all services that are used by the application
#[derive(Clone)]
pub struct ServiceContext {
    pub config: Config,
    dht_client: Client,
    pub contact_service: Arc<dyn ContactServiceApi>,
}

impl ServiceContext {
    pub fn new(config: Config, client: Client, contact_service: ContactService) -> Self {
        Self {
            config,
            dht_client: client,
            contact_service: Arc::new(contact_service),
        }
    }

    /// returns an owned instance of the dht client
    pub fn dht_client(&self) -> Client {
        self.dht_client.clone()
    }
}

/// building up the service context dependencies here for now. Later we can modularize this
/// and make it more flexible.
pub async fn create_service_context(config: Config, client: Client) -> Result<ServiceContext> {
    let contact_store = FileBasedContactStore::new(&config.data_dir, "contacts", "contacts")?;
    let contact_service = ContactService::new(client.clone(), Arc::new(contact_store));

    Ok(ServiceContext::new(config, client, contact_service))
}
