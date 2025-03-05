use bcr_ebill_core::{
    blockchain::bill::{BillBlock, BillBlockchain},
    notification::ActionType,
};
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

/// Used to signal a change in the blockchain of a bill and an optional
/// action event. Given some bill_id, this can signal an action to be
/// performed by the receiver and a change in the blockchain. If the
/// recipient is a new chain participant, the recipient receives the full
/// chain otherwise just the most recent block.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BillChainEventPayload {
    pub bill_id: String,
    pub action_type: Option<ActionType>,
    pub sum: Option<u64>,
    pub block: Option<BillBlock>,
    pub chain: Option<BillBlockchain>,
}
