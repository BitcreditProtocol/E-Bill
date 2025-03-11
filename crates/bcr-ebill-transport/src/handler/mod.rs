use crate::Result;
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use log::info;
#[cfg(test)]
use mockall::automock;

use super::EventEnvelope;
use bcr_ebill_core::notification::EventType;

mod bill_action_event_handler;

pub use bill_action_event_handler::BillActionEventHandler;

#[cfg(test)]
impl ServiceTraitBounds for MockNotificationHandlerApi {}

/// Handle an event when we receive it from a channel.
#[allow(dead_code)]
#[cfg_attr(test, automock)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait NotificationHandlerApi: ServiceTraitBounds {
    /// Whether this handler handles the given event type.
    fn handles_event(&self, event_type: &EventType) -> bool;

    /// Handle the event. This is called by the notification processor which should
    /// have checked the event type before calling this method. The actual implementation
    /// should be able to deserialize the data into its T type because the EventType
    /// determines the T type. Identity represents the active identity that is receiving
    /// the event.
    async fn handle_event(&self, event: EventEnvelope, node_id: &str) -> Result<()>;
}

/// Logs all events that are received and registered in the event_types.
pub struct LoggingEventHandler {
    pub event_types: Vec<EventType>,
}

impl ServiceTraitBounds for LoggingEventHandler {}

/// Just a dummy handler that logs the event and returns Ok(())
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl NotificationHandlerApi for LoggingEventHandler {
    fn handles_event(&self, event_type: &EventType) -> bool {
        self.event_types.contains(event_type)
    }

    async fn handle_event(&self, event: EventEnvelope, identity: &str) -> Result<()> {
        info!("########### EVENT RECEIVED #############");
        info!("Received event: {event:?} for identity: {identity}");
        info!("########################################");
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use serde::{Deserialize, Serialize, de::DeserializeOwned};
    use tokio::sync::Mutex;

    use crate::Event;

    use super::*;

    #[tokio::test]
    async fn test_event_handling() {
        let accepted_event = EventType::BillPaid;

        // given a handler that accepts the event type
        let event_handler: TestEventHandler<TestEventPayload> =
            TestEventHandler::new(Some(accepted_event.to_owned()));

        // event type should be accepted
        assert!(event_handler.handles_event(&accepted_event));

        // given an event and encode it to an envelope
        let event = create_test_event(&EventType::BillPaid);
        let envelope: EventEnvelope = event.clone().try_into().unwrap();

        // handler should run successfully
        event_handler
            .handle_event(envelope, "identity")
            .await
            .expect("event was not handled");

        // handler should have been invoked
        let called = event_handler.called.lock().await;
        assert!(*called, "event was not handled");

        // and the event should have been received
        let received = event_handler.received_event.lock().await.clone().unwrap();
        assert_eq!(event.data, received.data, "handled payload was not correct");
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    struct TestEventPayload {
        pub foo: String,
        pub bar: u32,
    }

    struct TestEventHandler<T: Serialize + DeserializeOwned> {
        pub called: Mutex<bool>,
        pub received_event: Mutex<Option<Event<T>>>,
        pub accepted_event: Option<EventType>,
    }

    impl<T: Serialize + DeserializeOwned + Send + Sync> ServiceTraitBounds for TestEventHandler<T> {}

    impl<T: Serialize + DeserializeOwned> TestEventHandler<T> {
        pub fn new(accepted_event: Option<EventType>) -> Self {
            Self {
                called: Mutex::new(false),
                received_event: Mutex::new(None),
                accepted_event,
            }
        }
    }

    #[async_trait]
    impl NotificationHandlerApi for TestEventHandler<TestEventPayload> {
        fn handles_event(&self, event_type: &EventType) -> bool {
            match &self.accepted_event {
                Some(e) => e == event_type,
                None => true,
            }
        }

        async fn handle_event(&self, event: EventEnvelope, _: &str) -> Result<()> {
            *self.called.lock().await = true;
            let event: Event<TestEventPayload> = event.try_into()?;
            *self.received_event.lock().await = Some(event);
            Ok(())
        }
    }

    fn create_test_event_payload() -> TestEventPayload {
        TestEventPayload {
            foo: "foo".to_string(),
            bar: 42,
        }
    }

    fn create_test_event(event_type: &EventType) -> Event<TestEventPayload> {
        Event::new(
            event_type.to_owned(),
            "node_id",
            create_test_event_payload(),
        )
    }
}
