use std::collections::HashMap;

use bcr_ebill_core::{
    bill::{BillKeys, BitcreditBill},
    blockchain::{
        Blockchain,
        bill::{BillBlock, BillBlockchain},
    },
    notification::{ActionType, BillEventType},
};
use log::error;

use crate::{BillChainEventPayload, Error, Result};

use super::{Event, EventType};

pub struct BillChainEvent {
    pub bill: BitcreditBill,
    chain: BillBlockchain,
    participants: HashMap<String, usize>,
    bill_keys: BillKeys,
}

impl BillChainEvent {
    /// Create a new BillChainEvent instance.
    pub fn new(bill: &BitcreditBill, chain: &BillBlockchain, bill_keys: &BillKeys) -> Result<Self> {
        let participants = chain
            .get_all_nodes_with_added_block_height(bill_keys)
            .map_err(|e| {
                error!("Failed to get participants from blockchain: {}", e);
                Error::BlockChain(
                    "Failed to get participants from blockchain when creating a new chain event"
                        .to_string(),
                )
            })?;
        Ok(Self {
            bill: bill.clone(),
            chain: chain.clone(),
            participants,
            bill_keys: bill_keys.clone(),
        })
    }

    // Returns the latest block in the chain.
    fn latest_block(&self) -> BillBlock {
        self.chain.get_latest_block().clone()
    }

    // Returns all blocks for newly added participants, otherwise just the latest block or no
    // blocks if the node is not a participant.
    fn get_blocks_for_node(&self, node_id: &str) -> Vec<BillBlock> {
        match self.participants.get(node_id) {
            Some(height) if *height == self.chain.block_height() => self.chain.blocks().clone(),
            Some(_) => vec![self.latest_block()],
            None => Vec::new(),
        }
    }

    fn get_keys_for_node(&self, node_id: &str) -> Option<BillKeys> {
        match self.participants.get(node_id) {
            Some(height) if *height == self.chain.block_height() => Some(self.bill_keys.clone()),
            _ => None,
        }
    }

    /// Generates bill block events for all participants in the chain. Individual node_ids can be
    /// assigned a specific event and action type by providing an override.
    pub fn generate_action_messages(
        &self,
        event_overrides: HashMap<String, (BillEventType, ActionType)>,
    ) -> Vec<Event<BillChainEventPayload>> {
        self.participants
            .keys()
            .map(|node_id| {
                let (event_type, action) = event_overrides
                    .get(node_id)
                    .map(|(event_type, action)| (event_type.clone(), Some(action.clone())))
                    .unwrap_or((BillEventType::BillBlock, None));

                Event::new(
                    EventType::Bill,
                    node_id,
                    BillChainEventPayload {
                        event_type,
                        bill_id: self.bill.id.to_owned(),
                        action_type: action,
                        sum: Some(self.bill.sum),
                        blocks: self.get_blocks_for_node(node_id),
                        keys: self.get_keys_for_node(node_id),
                    },
                )
            })
            .collect()
    }
}
