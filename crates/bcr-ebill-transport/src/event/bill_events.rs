use bcr_ebill_core::notification::ActionType;
use serde::{Deserialize, Serialize};

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
