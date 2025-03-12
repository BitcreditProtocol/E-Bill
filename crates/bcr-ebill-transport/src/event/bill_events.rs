use bcr_ebill_core::{
    blockchain::bill::BillBlock,
    notification::{ActionType, BillEventType},
};
use serde::{Deserialize, Serialize};

/// Used to signal a change in the blockchain of a bill and an optional
/// action event. Given some bill_id, this can signal an action to be
/// performed by the receiver and a change in the blockchain. If the
/// recipient is a new chain participant, the recipient receives the full
/// chain otherwise just the most recent block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct BillChainEventPayload {
    pub event_type: BillEventType,
    pub bill_id: String,
    pub action_type: Option<ActionType>,
    pub sum: Option<u64>,
    pub blocks: Vec<BillBlock>,
}
