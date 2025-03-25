use async_trait::async_trait;
use bcr_ebill_core::contact::IdentityPublicData;
use bcr_ebill_transport::event::EventEnvelope;
use bcr_ebill_transport::handler::NotificationHandlerApi;
use log::{error, trace, warn};
use nostr_sdk::nips::nip59::UnwrappedGift;
use nostr_sdk::{
    Client, EventId, Filter, Kind, Metadata, Options, PublicKey, RelayPoolNotification, Timestamp,
    ToBech32, UnsignedEvent,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::service::contact_service::ContactServiceApi;
use crate::util::{BcrKeys, crypto};
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_persistence::{NostrEventOffset, NostrEventOffsetStoreApi};
use bcr_ebill_transport::{Error, NotificationJsonTransportApi, Result};

use tokio::task::spawn;
#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
use tokio_with_wasm as tokio;

#[cfg(not(target_arch = "wasm32"))]
use tokio;

#[derive(Clone, Debug)]
pub struct NostrConfig {
    keys: BcrKeys,
    relays: Vec<String>,
    name: String,
}

impl NostrConfig {
    pub fn new(keys: BcrKeys, relays: Vec<String>, name: String) -> Self {
        assert!(!relays.is_empty());
        Self { keys, relays, name }
    }

    #[allow(dead_code)]
    pub fn get_npub(&self) -> String {
        self.keys.get_nostr_npub()
    }

    pub fn get_relay(&self) -> String {
        self.relays[0].clone()
    }
}

/// A wrapper around nostr_sdk that implements the NotificationJsonTransportApi.
///
/// # Example:
/// ```ignore
/// let config = NostrConfig {
///     keys: BcrKeys::new(),
///     relays: vec!["wss://relay.example.com".to_string()],
///     name: "My Company".to_string(),
/// };
/// let transport = NostrClient::new(&config).await.unwrap();
/// transport.send(&recipient, event).await.unwrap();
/// ```
/// We use the latest GiftWrap and PrivateDirectMessage already with this if I
/// understand the nostr-sdk docs and sources correctly.
/// @see https://nips.nostr.com/59 and https://nips.nostr.com/17
#[derive(Clone, Debug)]
pub struct NostrClient {
    pub keys: BcrKeys,
    pub client: Client,
}

impl NostrClient {
    #[allow(dead_code)]
    pub async fn new(config: &NostrConfig) -> Result<Self> {
        let keys = config.keys.clone();
        let options = Options::new();
        let client = Client::builder()
            .signer(keys.get_nostr_keys().clone())
            .opts(options)
            .build();
        for relay in &config.relays {
            client.add_relay(relay).await.map_err(|e| {
                error!("Failed to add relay to Nostr client: {e}");
                Error::Network("Failed to add relay to Nostr client".to_string())
            })?;
        }
        client.connect().await;
        let metadata = Metadata::new()
            .name(&config.name)
            .display_name(&config.name);
        client.set_metadata(&metadata).await.map_err(|e| {
            error!("Failed to set and send user metadata with Nostr client: {e}");
            Error::Network("Failed to send user metadata with Nostr client".to_string())
        })?;
        Ok(Self { keys, client })
    }

    pub fn get_node_id(&self) -> String {
        self.keys.get_public_key()
    }

    /// Subscribe to some nostr events with a filter
    pub async fn subscribe(&self, subscription: Filter) -> Result<()> {
        self.client
            .subscribe(subscription, None)
            .await
            .map_err(|e| {
                error!("Failed to subscribe to Nostr events: {e}");
                Error::Network("Failed to subscribe to Nostr events".to_string())
            })?;
        Ok(())
    }

    /// Unwrap envelope from private direct message
    pub async fn unwrap_envelope(
        &self,
        note: RelayPoolNotification,
    ) -> Option<(EventEnvelope, PublicKey, EventId, Timestamp)> {
        let mut result: Option<(EventEnvelope, PublicKey, EventId, Timestamp)> = None;
        if let RelayPoolNotification::Event { event, .. } = note {
            if event.kind == Kind::GiftWrap {
                result = match self.client.unwrap_gift_wrap(&event).await {
                    Ok(UnwrappedGift { rumor, sender }) => extract_event_envelope(rumor)
                        .map(|e| (e, sender, event.id, event.created_at)),
                    Err(e) => {
                        error!("Unwrapping gift wrap failed: {e}");
                        None
                    }
                }
            }
        }
        result
    }
}

impl ServiceTraitBounds for NostrClient {}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl NotificationJsonTransportApi for NostrClient {
    fn get_sender_key(&self) -> String {
        self.get_node_id()
    }
    async fn send(
        &self,
        recipient: &IdentityPublicData,
        event: EventEnvelope,
    ) -> bcr_ebill_transport::Result<()> {
        if let Ok(npub) = crypto::get_nostr_npub_as_hex_from_node_id(&recipient.node_id) {
            let public_key = PublicKey::from_str(&npub).map_err(|e| {
                error!("Failed to parse Nostr npub when sending a notification: {e}");
                Error::Crypto("Failed to parse Nostr npub".to_string())
            })?;
            let message = serde_json::to_string(&event)?;
            if let Some(relay) = &recipient.nostr_relay {
                if let Err(e) = self
                    .client
                    .send_private_msg_to(vec![relay], public_key, message, None)
                    .await
                {
                    error!("Error sending Nostr message: {e}")
                };
            } else if let Err(e) = self
                .client
                .send_private_msg(public_key, message, None)
                .await
            {
                error!("Error sending Nostr message: {e}")
            }
        } else {
            error!(
                "Try to send Nostr message but Nostr npub not found in contact {}",
                recipient.name
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct NostrConsumer {
    clients: HashMap<String, Arc<NostrClient>>,
    event_handlers: Arc<Vec<Box<dyn NotificationHandlerApi>>>,
    contact_service: Arc<dyn ContactServiceApi>,
    offset_store: Arc<dyn NostrEventOffsetStoreApi>,
}

impl NostrConsumer {
    #[allow(dead_code)]
    pub fn new(
        clients: Vec<Arc<NostrClient>>,
        contact_service: Arc<dyn ContactServiceApi>,
        event_handlers: Vec<Box<dyn NotificationHandlerApi>>,
        offset_store: Arc<dyn NostrEventOffsetStoreApi>,
    ) -> Self {
        let clients = clients
            .into_iter()
            .map(|c| (c.get_node_id(), c))
            .collect::<HashMap<String, Arc<NostrClient>>>();
        Self {
            clients,
            #[allow(clippy::arc_with_non_send_sync)]
            event_handlers: Arc::new(event_handlers),
            contact_service,
            offset_store,
        }
    }

    #[allow(dead_code)]
    pub async fn start(&self) -> Result<()> {
        // move dependencies into thread scope
        let clients = self.clients.clone();
        let event_handlers = self.event_handlers.clone();
        let _contact_service = self.contact_service.clone();
        let offset_store = self.offset_store.clone();

        let mut tasks = Vec::new();

        for (node_id, node_client) in clients.into_iter() {
            let current_client = node_client.clone();
            let event_handlers = event_handlers.clone();
            let offset_store = offset_store.clone();
            let client_id = node_id.clone();
            
            // Spawn a task for each client
            let task = spawn(async move {
                // continue where we left off
                let offset_ts = get_offset(&offset_store, &node_id).await;
                let public_key = current_client.keys.get_nostr_keys().public_key();

                // subscribe only to private messages sent to our pubkey
                current_client
                    .subscribe(
                        Filter::new()
                            .pubkey(public_key)
                            .kind(Kind::GiftWrap)
                            .since(offset_ts),
                    )
                    .await
                    .expect("Failed to subscribe to Nostr events");

                let inner = current_client.clone();
                current_client
                    .client
                    .handle_notifications(move |note| {
                        let client = inner.clone();
                        let event_handlers = event_handlers.clone();
                        let offset_store = offset_store.clone();
                        let node_id = node_id.clone();
                        let client_id = client_id.clone();
                        
                        async move {
                            if let Some((envelope, sender, event_id, time)) =
                                client.unwrap_envelope(note).await
                            {
                                if !offset_store.is_processed(&event_id.to_hex()).await? {
                                    let sender_npub = sender.to_bech32();
                                    let sender_node_id = sender.to_hex();
                                    trace!("Received event: {envelope:?} from {sender_npub:?} (hex: {sender_node_id}) on client {client_id}");
                                    // We use hex here, so we can compare it with our node_ids
                                    // TODO: re-enable after presentation: if contact_service.is_known_npub(&sender_node_id).await? {
                                        trace!("Processing event: {envelope:?}");
                                        handle_event(envelope, &node_id, &event_handlers).await?;
                                    // }

                                    // store the new event offset
                                    add_offset(&offset_store, event_id, time, true, &node_id).await;
                                }
                            };
                            Ok(false)
                        }
                    })
                    .await
                    .expect("Nostr notification handler failed");
            });
            
            tasks.push(task);
        }

        // Wait for all tasks to complete (they would run indefinitely unless interrupted)
        for task in tasks {
            if let Err(e) = task.await {
                error!("Nostr client task failed: {e}");
            }
        }
        
        Ok(())
    }
}

async fn get_offset(db: &Arc<dyn NostrEventOffsetStoreApi>, node_id: &str) -> Timestamp {
    Timestamp::from_secs(
        db.current_offset(node_id)
            .await
            .map_err(|e| error!("Could not get event offset: {e}"))
            .ok()
            .unwrap_or(0),
    )
}

async fn add_offset(
    db: &Arc<dyn NostrEventOffsetStoreApi>,
    event_id: EventId,
    time: Timestamp,
    success: bool,
    node_id: &str,
) {
    db.add_event(NostrEventOffset {
        event_id: event_id.to_hex(),
        time: time.as_u64(),
        success,
        node_id: node_id.to_string(),
    })
    .await
    .map_err(|e| error!("Could not store event offset: {e}"))
    .ok();
}

fn extract_event_envelope(rumor: UnsignedEvent) -> Option<EventEnvelope> {
    if rumor.kind == Kind::PrivateDirectMessage {
        match serde_json::from_str::<EventEnvelope>(rumor.content.as_str()) {
            Ok(envelope) => Some(envelope),
            Err(e) => {
                error!("Json deserializing event envelope failed: {e}");
                None
            }
        }
    } else {
        None
    }
}

/// Handle extracted event with given handlers.
async fn handle_event(
    event: EventEnvelope,
    node_id: &str,
    handlers: &Arc<Vec<Box<dyn NotificationHandlerApi>>>,
) -> Result<()> {
    let event_type = &event.event_type;
    let mut times = 0;
    for handler in handlers.iter() {
        if handler.handles_event(event_type) {
            match handler.handle_event(event.to_owned(), node_id).await {
                Ok(_) => times += 1,
                Err(e) => error!("Nostr event handler failed: {e}"),
            }
        }
    }
    if times < 1 {
        warn!("No handler subscribed for event: {event:?}");
    } else {
        trace!("{event_type:?} event handled successfully {times} times");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bcr_ebill_core::{ServiceTraitBounds, notification::BillEventType};
    use bcr_ebill_transport::event::{Event, EventType};
    use bcr_ebill_transport::handler::NotificationHandlerApi;
    use mockall::predicate;
    use tokio::time;

    use super::super::test_utils::get_mock_relay;
    use super::{NostrClient, NostrConfig, NostrConsumer};
    use crate::persistence::nostr::NostrEventOffset;
    use crate::service::{
        contact_service::MockContactServiceApi,
        notification_service::{NotificationJsonTransportApi, test_utils::*},
    };
    use crate::tests::tests::MockNostrEventOffsetStoreApiMock;
    use crate::util::BcrKeys;
    use mockall::mock;

    impl ServiceTraitBounds for MockNotificationHandler {}
    mock! {
        pub NotificationHandler {}
        #[async_trait::async_trait]
        impl NotificationHandlerApi for NotificationHandler {
            async fn handle_event(&self, event: bcr_ebill_transport::EventEnvelope, identity: &str) -> bcr_ebill_transport::Result<()>;
            fn handles_event(&self, event_type: &EventType) -> bool;
        }
    }

    /// When testing with the mock relay we need to be careful. It is always
    /// listening on the same port and will not start multiple times. If we
    /// share the instance tests will fail with events from other tests.
    #[tokio::test]
    async fn test_send_and_receive_event() {
        let relay = get_mock_relay().await;
        let url = relay.url();

        let keys1 = BcrKeys::new();
        let keys2 = BcrKeys::new();

        // given two clients
        let config1 = NostrConfig {
            keys: keys1.clone(),
            relays: vec![url.to_string()],
            name: "BcrDamus1".to_string(),
        };
        let client1 = NostrClient::new(&config1)
            .await
            .expect("failed to create nostr client 1");

        let config2 = NostrConfig {
            keys: keys2.clone(),
            relays: vec![url.to_string()],
            name: "BcrDamus2".to_string(),
        };
        let client2 = NostrClient::new(&config2)
            .await
            .expect("failed to create nostr client 2");

        // and a contact we want to send an event to
        let contact =
            get_identity_public_data(&keys2.get_public_key(), "payee@example.com", Some(&url));
        let mut event = create_test_event(&BillEventType::BillSigned);
        event.node_id = contact.node_id.to_owned();

        // expect the receiver to check if the sender contact is known
        let mut contact_service = MockContactServiceApi::new();
        contact_service
            .expect_is_known_npub()
            .with(predicate::eq(keys1.get_nostr_npub_as_hex()))
            .returning(|_| Ok(true));

        // expect a handler that is subscribed to the event type w sent
        let mut handler = MockNotificationHandler::new();
        handler
            .expect_handles_event()
            .with(predicate::eq(&EventType::Bill))
            .returning(|_| true);

        // expect a handler receiving the event we sent
        let expected_event: Event<TestEventPayload> = event.clone();
        handler
            .expect_handle_event()
            .withf(move |e, i| {
                let expected = expected_event.clone();
                let received: Event<TestEventPayload> =
                    e.clone().try_into().expect("could not convert event");
                let valid_type = received.event_type == expected.event_type;
                let valid_receiver = received.node_id == expected.node_id;
                let valid_payload = received.data.foo == expected.data.foo;
                let valid_identity = i == keys2.get_public_key();
                valid_type && valid_receiver && valid_payload && valid_identity
            })
            .returning(|_, _| Ok(()));

        let mut offset_store = MockNostrEventOffsetStoreApiMock::new();

        // expect the offset store to return the current offset once on start
        offset_store
            .expect_current_offset()
            .returning(|_| Ok(1000))
            .once();

        // should also check if the event has been processed already
        offset_store
            .expect_is_processed()
            .withf(|e: &str| !e.is_empty())
            .returning(|_| Ok(false))
            .once();

        // when done processing the event, add it to the offset store
        offset_store
            .expect_add_event()
            .withf(|e: &NostrEventOffset| e.success)
            .returning(|_| Ok(()))
            .once();

        // we start the consumer
        let consumer = NostrConsumer::new(
            vec![Arc::new(client2)],
            Arc::new(contact_service),
            vec![Box::new(handler)],
            Arc::new(offset_store),
        );

        // run in a local set
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let handle = tokio::task::spawn_local(async move {
                    consumer
                        .start()
                        .await
                        .expect("failed to start nostr consumer");
                });
                // and send an event
                client1
                    .send(&contact, event.try_into().expect("could not convert event"))
                    .await
                    .expect("failed to send event");

                // give it a little bit of time to process the event
                time::sleep(Duration::from_millis(100)).await;
                handle.abort();
            })
            .await;
    }
}
