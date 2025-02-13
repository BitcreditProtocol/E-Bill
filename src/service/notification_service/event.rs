use super::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

/// The different types of events that can be sent via this service.
/// For now we only have Bill events and this needs some clippy
/// exceptions here. As soon as we have other event topics, we can
/// add new types here and remove the clippy exceptions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names, dead_code)]
pub enum EventType {
    BillSigned,
    BillAccepted,
    BillAcceptanceRequested,
    BillAcceptanceRejected,
    BillAcceptanceTimeout,
    BillAcceptanceRecourse,
    BillPaymentRequested,
    BillPaymentRejected,
    BillPaymentRecourse,
    BillRecourseRejected,
    BillRecourseTimeout,
    BillPaymentTimeout,
    BillSellOffered,
    BillBuyingRejected,
    BillPaid,
    BillRecoursePaid,
    BillEndorsed,
    BillSold,
    BillMintingRequested,
    BillNewQuote,
    BillQuoteApproved,
}

impl EventType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::BillSigned,
            Self::BillAccepted,
            Self::BillAcceptanceRequested,
            Self::BillAcceptanceRejected,
            Self::BillAcceptanceTimeout,
            Self::BillAcceptanceRecourse,
            Self::BillPaymentRequested,
            Self::BillPaymentRejected,
            Self::BillPaymentTimeout,
            Self::BillPaymentRecourse,
            Self::BillRecourseTimeout,
            Self::BillRecourseRejected,
            Self::BillSellOffered,
            Self::BillBuyingRejected,
            Self::BillPaid,
            Self::BillRecoursePaid,
            Self::BillEndorsed,
            Self::BillSold,
            Self::BillMintingRequested,
            Self::BillNewQuote,
            Self::BillQuoteApproved,
        ]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names, dead_code)]
pub enum ActionType {
    BuyBill,
    RecourseBill,
    AcceptBill,
    CheckBill,
    PayBill,
    CheckQuote,
}

impl ActionType {
    /// Return a corresponding rejected event type for the action type
    /// if the action has a rejected event type. If not, return None.
    pub fn get_rejected_event_type(&self) -> Option<EventType> {
        match self {
            Self::AcceptBill => Some(EventType::BillAcceptanceRejected),
            Self::PayBill => Some(EventType::BillPaymentRejected),
            Self::BuyBill => Some(EventType::BillBuyingRejected),
            Self::RecourseBill => Some(EventType::BillRecourseRejected),
            _ => None,
        }
    }

    /// Return a corresponding timeout event type for the action type
    /// if the action has a timeout event type. If not, return None.
    pub fn get_timeout_event_type(&self) -> Option<EventType> {
        match self {
            Self::AcceptBill => Some(EventType::BillAcceptanceTimeout),
            Self::PayBill => Some(EventType::BillPaymentTimeout),
            Self::RecourseBill => Some(EventType::BillRecourseTimeout),
            _ => None,
        }
    }

    // Return a corresponding recourse event type for the action type
    // if the action has a recourse event type. If not, return None.
    pub fn get_recourse_event_type(&self) -> Option<EventType> {
        match self {
            Self::AcceptBill => Some(EventType::BillAcceptanceRecourse),
            Self::PayBill => Some(EventType::BillPaymentRecourse),
            _ => None,
        }
    }
}

/// Can be used for all events that are just signalling an action
/// to be performed by the receiver. If we want to also notify
/// recipients via email or push notifications, we probably need to
/// add more fields here and create multiple event types.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BillActionEventPayload {
    pub bill_id: String,
    pub action_type: ActionType,
    pub sum: Option<u64>,
}

/// A generic event that can be sent to a specific recipient
/// and is serializable. The recipient is currently just a string,
/// and we have to decide what the identifier is.
/// This event should contain all the information that is needed
/// to send to different channels including email, push and Nostr.
#[derive(Serialize, Debug, Clone)]
pub struct Event<T: Serialize> {
    pub event_type: EventType,
    pub version: String,
    pub node_id: String,
    pub data: T,
}

impl<T: Serialize> Event<T> {
    #[allow(dead_code)]
    pub fn new(event_type: EventType, node_id: &str, data: T) -> Self {
        Self {
            event_type: event_type.to_owned(),
            version: get_version(&event_type),
            node_id: node_id.to_owned(),
            data,
        }
    }
}

/// The event version that is used for all events if no specific version
/// is set via get_version.
const DEFAULT_EVENT_VERSION: &str = "1.0";

/// If we want to bump the version of a single event type, we can do so
/// by matching the event type and returning the new version here.
fn get_version(_event_type: &EventType) -> String {
    DEFAULT_EVENT_VERSION.into()
}

/// When we receive an event, we need to know what type it is and
/// how to handle it. This payload envelope allows us to find out
/// the type of event to later deserialize the data into the correct
/// type.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventEnvelope {
    pub event_type: EventType,
    pub version: String,
    pub node_id: String,
    pub data: Value,
}

impl<T: Serialize> TryFrom<Event<T>> for EventEnvelope {
    type Error = super::Error;

    fn try_from(event: Event<T>) -> Result<Self> {
        Ok(Self {
            event_type: event.event_type,
            version: event.version,
            node_id: event.node_id,
            data: serde_json::to_value(event.data)?,
        })
    }
}

/// Allows generic deserialization of an event from an envelope.
/// # Example
///
/// ```
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct MyEventPayload {
///     foo: String,
///     bar: u32,
/// }
///
/// let payload = MyEventPayload {
///     foo: "foo".to_string(),
///     bar: 42,
/// };
///
/// let event = Event::new(EventType::BillSigned, "recipient".to_string(), payload);
/// let event: EventEnvelope = event.try_into().unwrap();
/// let deserialized_event: Event<MyEventPayload> = event.try_into().unwrap();
/// assert_eq!(deserialized_event.data, payload);
///
/// ```
///
impl<T: DeserializeOwned + Serialize> TryFrom<EventEnvelope> for Event<T> {
    type Error = super::Error;
    fn try_from(envelope: EventEnvelope) -> Result<Self> {
        let data: T = serde_json::from_value(envelope.data)?;
        Ok(Self {
            event_type: envelope.event_type,
            version: envelope.version,
            node_id: envelope.node_id,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use super::*;

    #[test]
    fn test_event_serialization() {
        // give payload
        let payload = create_test_event_payload();
        // create event
        let event = Event::new(EventType::BillSigned, "node_id", payload.clone());
        // create envelope
        let envelope: EventEnvelope = event.clone().try_into().unwrap();

        // check that the envelope is correct
        assert_eq!(
            &event.event_type, &envelope.event_type,
            "envelope has wrong event type"
        );
        assert_eq!(
            &event.node_id, &envelope.node_id,
            "envelope has wrong node id"
        );

        // check that the deserialization works
        let deserialized_event: Event<TestEventPayload> = envelope.try_into().unwrap();
        assert_eq!(
            &deserialized_event.data, &payload,
            "payload was not deserialized correctly"
        );
        assert_eq!(
            &deserialized_event.event_type, &event.event_type,
            "deserialized event has wrong event type"
        );
        assert_eq!(
            &deserialized_event.node_id, &event.node_id,
            "deserialized event has wrong node id"
        );
    }
}
