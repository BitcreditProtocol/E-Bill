use async_trait::async_trait;
use log::{error, trace, warn};
use nostr_sdk::prelude::*;
use nostr_sdk::Timestamp;
use std::str::FromStr;
use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::persistence::NostrEventOffset;
use crate::persistence::NostrEventOffsetStoreApi;
use crate::service::contact_service::ContactServiceApi;
use crate::util::{crypto, BcrKeys};

use super::super::contact_service::IdentityPublicData;
use super::handler::NotificationHandlerApi;
use super::{EventEnvelope, NotificationJsonTransportApi, Result};

#[derive(Clone, Debug)]
pub struct NostrConfig {
    keys: BcrKeys,
    relays: Vec<String>,
    name: String,
}

impl NostrConfig {
    pub fn new(keys: BcrKeys, relays: Vec<String>, name: String) -> Self {
        Self { keys, relays, name }
    }

    #[allow(dead_code)]
    pub fn get_npub(&self) -> Result<String> {
        Ok(self.keys.get_nostr_npub()?)
    }
}

/// A wrapper around nostr_sdk that implements the NotificationJsonTransportApi.
///
/// # Example:
/// ```
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
            client.add_relay(relay).await?;
        }
        client.connect().await;
        let metadata = Metadata::new()
            .name(&config.name)
            .display_name(&config.name);
        client.set_metadata(&metadata).await?;
        Ok(Self { keys, client })
    }

    /// Subscribe to some nostr events with a filter
    pub async fn subscribe(&self, subscription: Filter) -> Result<()> {
        self.client.subscribe(vec![subscription], None).await?;
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

#[async_trait]
impl NotificationJsonTransportApi for NostrClient {
    async fn send(&self, recipient: &IdentityPublicData, event: EventEnvelope) -> Result<()> {
        if let Ok(npub) = crypto::get_nostr_npub_as_hex_from_node_id(&recipient.node_id) {
            let public_key = PublicKey::from_str(&npub)?;
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
    client: NostrClient,
    event_handlers: Arc<Vec<Box<dyn NotificationHandlerApi>>>,
    contact_service: Arc<dyn ContactServiceApi>,
    offset_store: Arc<dyn NostrEventOffsetStoreApi>,
}

impl NostrConsumer {
    #[allow(dead_code)]
    pub fn new(
        client: NostrClient,
        contact_service: Arc<dyn ContactServiceApi>,
        event_handlers: Vec<Box<dyn NotificationHandlerApi>>,
        offset_store: Arc<dyn NostrEventOffsetStoreApi>,
    ) -> Self {
        Self {
            client,
            event_handlers: Arc::new(event_handlers),
            contact_service,
            offset_store,
        }
    }

    #[allow(dead_code)]
    pub async fn start(&self) -> Result<JoinHandle<()>> {
        // move dependencies into thread scope
        let client = self.client.clone();
        let event_handlers = self.event_handlers.clone();
        let _contact_service = self.contact_service.clone();
        let offset_store = self.offset_store.clone();

        // continue where we left off
        let offset_ts = get_offset(&offset_store).await;
        let public_key = client.keys.get_nostr_keys().public_key();
        let node_id = client.keys.get_public_key();

        // subscribe only to private messages sent to our pubkey
        client
            .subscribe(
                Filter::new()
                    .pubkey(public_key)
                    .kind(Kind::GiftWrap)
                    .since(offset_ts),
            )
            .await
            .expect("Failed to subscribe to Nostr events");

        // run subscription in a tokio task
        let handle = tokio::spawn(async move {
            client
                .client
                .handle_notifications(|note| async {
                    if let Some((envelope, sender, event_id, time)) =
                        client.unwrap_envelope(note).await
                    {
                        if !offset_store.is_processed(&event_id.to_hex()).await? {
                            if let Ok(sender_npub) = sender.to_bech32() {
                                let sender_node_id = sender.to_hex();
                                trace!("Received event: {envelope:?} from {sender_npub:?} (hex: {sender_node_id})");
                                // We use hex here, so we can compare it with our node_ids
                                // TODO: re-enable after presentation: if contact_service.is_known_npub(&sender_node_id).await? {
                                    trace!("Processing event: {envelope:?}");
                                    handle_event(envelope, &node_id, &event_handlers).await?;
                                // }
                            }

                            // store the new event offset
                            add_offset(&offset_store, event_id, time, true).await;
                        }
                    };
                    Ok(false)
                })
                .await
                .expect("Nostr notification handler failed");
        });
        Ok(handle)
    }
}

async fn get_offset(db: &Arc<dyn NostrEventOffsetStoreApi>) -> Timestamp {
    Timestamp::from_secs(
        db.current_offset()
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
) {
    db.add_event(NostrEventOffset {
        event_id: event_id.to_hex(),
        time: time.as_u64(),
        success,
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

    use mockall::predicate;
    use tokio::time;

    use super::super::test_utils::get_mock_relay;
    use super::{NostrClient, NostrConfig, NostrConsumer};
    use crate::persistence::nostr::MockNostrEventOffsetStoreApi;
    use crate::persistence::NostrEventOffset;
    use crate::service::notification_service::event::Event;
    use crate::service::{
        contact_service::MockContactServiceApi,
        notification_service::{
            handler::MockNotificationHandlerApi, test_utils::*, EventType,
            NotificationJsonTransportApi,
        },
    };
    use crate::util::BcrKeys;

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
        let mut event = create_test_event(&EventType::BillSigned);
        event.node_id = contact.node_id.to_owned();

        // expect the receiver to check if the sender contact is known
        let mut contact_service = MockContactServiceApi::new();
        contact_service
            .expect_is_known_npub()
            .with(predicate::eq(keys1.get_nostr_npub_as_hex()))
            .returning(|_| Ok(true));

        // expect a handler that is subscribed to the event type w sent
        let mut handler = MockNotificationHandlerApi::new();
        handler
            .expect_handles_event()
            .with(predicate::eq(&EventType::BillSigned))
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

        let mut offset_store = MockNostrEventOffsetStoreApi::new();

        // expect the offset store to return the current offset once on start
        offset_store
            .expect_current_offset()
            .returning(|| Ok(1000))
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
            client2,
            Arc::new(contact_service),
            vec![Box::new(handler)],
            Arc::new(offset_store),
        );
        let handle = consumer
            .start()
            .await
            .expect("failed to start nostr consumer");

        // and send an event
        client1
            .send(&contact, event.try_into().expect("could not convert event"))
            .await
            .expect("failed to send event");

        // give it a little bit of time to process the event
        time::sleep(Duration::from_millis(100)).await;
        handle.abort();
    }
}
