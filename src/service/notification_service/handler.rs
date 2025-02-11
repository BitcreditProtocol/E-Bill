use super::{EventEnvelope, EventType, Result};
use async_trait::async_trait;
use log::info;
#[cfg(test)]
use mockall::automock;

/// Handle an event when we receive it from a channel.
#[allow(dead_code)]
#[cfg_attr(test, automock)]
#[async_trait]
pub trait NotificationHandlerApi: Send + Sync {
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

/// Just a dummy handler that logs the event and returns Ok(())
#[async_trait]
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
    use super::super::test_utils::*;
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
}
