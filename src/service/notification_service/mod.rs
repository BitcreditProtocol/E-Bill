use std::sync::Arc;

use crate::config::Config;
use crate::data::{
    bill::BitcreditBill,
    contact::IdentityPublicData,
    notification::{Notification, NotificationType},
};
use crate::persistence::notification::{NotificationFilter, NotificationStoreApi};
use crate::persistence::NostrEventOffsetStoreApi;
use crate::persistence::{self, identity::IdentityStoreApi};
use crate::util::{self};
use async_trait::async_trait;
use bill_action_event_handler::BillActionEventHandler;
use default_service::DefaultNotificationService;
use handler::{LoggingEventHandler, NotificationHandlerApi};
#[cfg(test)]
use mockall::automock;
use push_notification::PushApi;
use thiserror::Error;

#[cfg(test)]
pub mod test_utils;

pub mod bill_action_event_handler;
pub mod default_service;
mod email;
mod email_lettre;
mod email_sendgrid;
mod event;
mod handler;
mod nostr;
pub mod push_notification;
mod transport;

pub use email::NotificationEmailTransportApi;
pub use event::{ActionType, EventEnvelope, EventType};
pub use nostr::{NostrClient, NostrConfig, NostrConsumer};
pub use transport::NotificationJsonTransportApi;

use super::contact_service::ContactServiceApi;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// json errors when serializing/deserializing notification events
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// errors stemming from lettre smtp transport
    #[error("lettre smtp transport error: {0}")]
    SmtpTransport(#[from] lettre::transport::smtp::Error),

    /// errors stemming from lettre stub transport (this will only be used for testing)
    #[error("lettre stub transport error: {0}")]
    StubTransport(#[from] lettre::transport::stub::Error),

    /// errors stemming from lettre email contents creation
    #[error("lettre email error: {0}")]
    LettreEmail(#[from] lettre::error::Error),

    /// errors stemming from lettre address parsing
    #[error("lettre address error: {0}")]
    LettreAddress(#[from] lettre::address::AddressError),

    /// some transports require a http client where we use reqwest
    #[error("http client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    #[error("nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),

    #[error("nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),

    #[error("crypto util error: {0}")]
    CryptoUtil(#[from] util::crypto::Error),

    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),
}

/// Creates a new nostr client configured with the current identity user.
pub async fn create_nostr_client(
    config: &Config,
    identity_store: Arc<dyn IdentityStoreApi>,
) -> Result<NostrClient> {
    let keys = identity_store.get_or_create_key_pair().await?;

    let nostr_name = match identity_store.get().await {
        Ok(identity) => identity.get_nostr_name(),
        _ => "New user".to_owned(),
    };
    let config = NostrConfig::new(keys, vec![config.nostr_relay.clone()], nostr_name);
    NostrClient::new(&config).await
}

/// Creates a new notification service that will send events via the given Nostr json transport.
pub async fn create_notification_service(
    client: NostrClient,
    notification_store: Arc<dyn NotificationStoreApi>,
) -> Result<Arc<dyn NotificationServiceApi>> {
    Ok(Arc::new(DefaultNotificationService::new(
        Box::new(client),
        notification_store,
    )))
}

/// Creates a new nostr consumer that will listen for incoming events and handle them
/// with the given handlers. The consumer is just set up here and needs to be started
/// via the run method later.
pub async fn create_nostr_consumer(
    client: NostrClient,
    contact_service: Arc<dyn ContactServiceApi>,
    nostr_event_offset_store: Arc<dyn NostrEventOffsetStoreApi>,
    notification_store: Arc<dyn NotificationStoreApi>,
    push_service: Arc<dyn PushApi>,
) -> Result<NostrConsumer> {
    // register the logging event handler for all events for now. Later we will probably
    // setup the handlers outside and pass them to the consumer via this functions arguments.
    let handlers: Vec<Box<dyn NotificationHandlerApi>> = vec![
        Box::new(LoggingEventHandler {
            event_types: EventType::all(),
        }),
        Box::new(BillActionEventHandler::new(
            notification_store,
            push_service,
        )),
    ];
    let consumer = NostrConsumer::new(client, contact_service, handlers, nostr_event_offset_store);
    Ok(consumer)
}

/// Send events via all channels required for the event type.
#[allow(dead_code)]
#[cfg_attr(test, automock)]
#[async_trait]
pub trait NotificationServiceApi: Send + Sync {
    /// Sent when: A bill is signed by: Drawer
    /// Receiver: Payer, Action: AcceptBill
    /// Receiver: Payee, Action: CheckBill
    async fn send_bill_is_signed_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is accepted by: Payer
    /// Receiver: Holder, Action: CheckBill
    async fn send_bill_is_accepted_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is requested to be accepted, Sent by: Holder
    /// Receiver: Payer, Action: AcceptBill
    async fn send_request_to_accept_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is requested to be paid, Sent by: Holder
    /// Receiver: Payer, Action: PayBill
    async fn send_request_to_pay_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is paid by: Payer (Bitcoin API)
    /// Receiver: Payee, Action: CheckBill
    async fn send_bill_is_paid_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is endorsed by: Previous Holder
    /// Receiver: NewHolder, Action: CheckBill
    async fn send_bill_is_endorsed_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A bill is offered to be sold, Sent by: Holder
    /// Receiver: Buyer, Action: CheckBill (with buy page)
    async fn send_offer_to_sell_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        buyer: &IdentityPublicData,
    ) -> Result<()>;

    /// Sent when: A bill is sold by: Seller (old holder)
    /// Receiver: Buyer (new holder), Action: CheckBill
    async fn send_bill_is_sold_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        buyer: &IdentityPublicData,
    ) -> Result<()>;

    /// Sent when: A bill recourse was paid, by: Recourser (old holder)
    /// Receiver: Recoursee (new holder), Action: CheckBill
    async fn send_bill_recourse_paid_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        recoursee: &IdentityPublicData,
    ) -> Result<()>;

    /// In case a participant rejects one of the 'request to' actions (e.g. request to accept,
    /// request to pay) we send this event to all bill participants. Will only send the event
    /// if the given action can be a rejected action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * rejected_action: The action that was rejected
    /// * recipients: The list of recipients that should receive the notification
    async fn send_request_to_action_rejected_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        rejected_action: event::ActionType,
        recipients: Vec<IdentityPublicData>,
    ) -> Result<()>;

    /// In case a participant did not perform an action (e.g. request to accept, request
    /// to pay) in time we notify all bill participants about the timed out action. Will
    /// only send the event if the given action can be a timed out action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * timed_out_action: The action that has timed out
    /// * recipients: The list of recipients that should receive the notification
    async fn send_request_to_action_timed_out_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        timed_out_action: event::ActionType,
        recipients: Vec<IdentityPublicData>,
    ) -> Result<()>;

    /// In case an action was rejected or timed out a holder can request a recourse action
    /// from another participant in the chain. Will only send the event if the given action
    /// can be a recourse action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * action: The action that should be performed via recourse. This will also be the action
    /// sent in the event given it can be a recourse action.
    /// * recipient: The recourse recipient that should perform the action
    async fn send_recourse_action_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        action: event::ActionType,
        recipient: &IdentityPublicData,
    ) -> Result<()>;

    /// Sent when: A bill is requested to be minted, Sent by: Holder
    /// Receiver: Mint, Action: CheckBill (with generate quote page)
    async fn send_request_to_mint_event(&self, bill: &BitcreditBill) -> Result<()>;

    /// Sent when: A new quote is created, Sent by: Mint
    /// Receiver: Holder, Action: Check quote page
    async fn send_new_quote_event(&self, quote: &BitcreditBill) -> Result<()>;

    /// Sent when: A quote is approved by: Previous Holder
    /// Receiver: Mint (new holder), Action: CheckBill
    async fn send_quote_is_approved_event(&self, quote: &BitcreditBill) -> Result<()>;

    /// Returns filtered client notifications
    async fn get_client_notifications(
        &self,
        filter: NotificationFilter,
    ) -> Result<Vec<Notification>>;

    /// Marks the notification with given id as done
    async fn mark_notification_as_done(&self, notification_id: &str) -> Result<()>;

    /// Returns the active bill notification for the given bill id
    async fn get_active_bill_notification(&self, bill_id: &str) -> Option<Notification>;

    /// Returns whether a notification was already sent for the given bill id and action
    async fn check_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<bool>;

    /// Stores that a notification was sent for the given bill id and action
    async fn mark_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<()>;
}
