use std::collections::HashMap;

use bcr_ebill_core::{
    bill::{BillKeys, BitcreditBill},
    blockchain::{
        Blockchain,
        bill::{BillBlock, BillBlockchain},
    },
    notification::{ActionType, EventType},
};

use crate::{BillChainEventPayload, Result};

use super::Event;

pub struct BillChainEvent {
    pub bill: BitcreditBill,
    bill_keys: BillKeys,
    chain: BillBlockchain,
    participants: HashMap<String, usize>,
}

impl BillChainEvent {
    /// Create a new BillChainEvent instance.
    pub fn new(bill: &BitcreditBill, chain: &BillBlockchain, bill_keys: &BillKeys) -> Result<Self> {
        let participants = chain.get_all_nodes_with_added_block_height(bill_keys)?;
        Ok(Self {
            bill: bill.clone(),
            chain: chain.clone(),
            bill_keys: bill_keys.clone(),
            participants,
        })
    }

    // Returns the latest block in the chain.
    fn lastest_block(&self) -> BillBlock {
        self.chain.get_latest_block().clone()
    }

    // Returns all blocks for newly added participants, otherwise just the latest block or no
    // blocks if the node is not a participant.
    fn get_blocks_for_node(&self, node_id: &str) -> Vec<BillBlock> {
        match self.participants.get(node_id) {
            Some(height) if *height == self.chain.block_height() => self.chain.blocks().clone(),
            Some(_) => vec![self.lastest_block()],
            None => Vec::new(),
        }
    }

    // Returns some bill keys in case the node is a new participant, otherwise none.
    fn get_keys_for_node(&self, node_id: &str) -> Option<BillKeys> {
        match self.participants.get(node_id) {
            Some(height) if *height == self.chain.block_height() => Some(self.bill_keys.clone()),
            _ => None,
        }
    }

    /// Generates a bill action event for a single node id. Will leave out chain information
    /// in case the node is not a participant.
    pub fn generate_action_message(
        &self,
        node_id: &str,
        event_type: EventType,
        action: ActionType,
    ) -> Event<BillChainEventPayload> {
        Event::new(
            event_type,
            node_id,
            BillChainEventPayload {
                bill_id: self.bill.id.to_owned(),
                action_type: Some(action),
                sum: Some(self.bill.sum),
                keys: self.get_keys_for_node(node_id),
                blocks: self.get_blocks_for_node(node_id),
            },
        )
    }

    /// Generates bill block events for all participants in the chain. Individual node_ids can be
    /// assigned a specific event and action type by providing an override.
    pub fn generate_action_messages(
        &self,
        event_overrides: HashMap<String, (EventType, ActionType)>,
    ) -> Vec<Event<BillChainEventPayload>> {
        self.participants
            .keys()
            .map(|node_id| {
                let (event_type, action) = event_overrides
                    .get(node_id)
                    .map(|(event_type, action)| (event_type.clone(), Some(action.clone())))
                    .unwrap_or((EventType::BillBlock, None));

                Event::new(
                    event_type,
                    node_id,
                    BillChainEventPayload {
                        bill_id: self.bill.id.to_owned(),
                        action_type: action,
                        sum: Some(self.bill.sum),
                        keys: self.get_keys_for_node(node_id),
                        blocks: self.get_blocks_for_node(node_id),
                    },
                )
            })
            .collect()
    }
}
