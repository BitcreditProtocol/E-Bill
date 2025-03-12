use std::sync::Arc;

use crate::Config;
use crate::persistence::identity::IdentityStoreApi;
use crate::persistence::nostr::NostrEventOffsetStoreApi;
use crate::persistence::notification::NotificationStoreApi;
use bcr_ebill_transport::handler::{
    BillChainEventHandler, LoggingEventHandler, NotificationHandlerApi,
};
use bcr_ebill_transport::{Error, EventType, Result};
use bcr_ebill_transport::{NotificationServiceApi, PushApi};
use default_service::DefaultNotificationService;
#[cfg(test)]
pub mod test_utils;

pub mod default_service;
mod nostr;

pub use bcr_ebill_transport::NotificationJsonTransportApi;
use log::error;
pub use nostr::{NostrClient, NostrConfig, NostrConsumer};

use super::contact_service::ContactServiceApi;

/// Creates a new nostr client configured with the current identity user.
pub async fn create_nostr_client(
    config: &Config,
    identity_store: Arc<dyn IdentityStoreApi>,
) -> Result<NostrClient> {
    let keys = identity_store.get_or_create_key_pair().await.map_err(|e| {
        error!(
            "Failed to get or create nostr key pair for nostr client: {}",
            e
        );
        Error::Crypto("Failed to get or create nostr key pair".to_string())
    })?;

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
    contact_service: Arc<dyn ContactServiceApi>,
) -> Result<Arc<dyn NotificationServiceApi>> {
    #[allow(clippy::arc_with_non_send_sync)]
    Ok(Arc::new(DefaultNotificationService::new(
        Box::new(client),
        notification_store,
        contact_service,
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
        Box::new(BillChainEventHandler::new(notification_store, push_service)),
    ];
    let consumer = NostrConsumer::new(client, contact_service, handlers, nostr_event_offset_store);
    Ok(consumer)
}
