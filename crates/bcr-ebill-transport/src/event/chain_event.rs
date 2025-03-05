use bcr_ebill_core::{
    bill::BillKeys,
    blockchain::bill::{BillBlock, BillBlockchain},
    blockchain::{Block, Blockchain},
};

use crate::Result;

pub struct BillChainEvent {
    bill_id: String,
    bill_keys: BillKeys,
    chain: BillBlockchain,
    new_nodes: Vec<String>,
}

impl BillChainEvent {
    pub fn new(bill_id: String, chain: BillBlockchain, bill_keys: BillKeys) -> Self {
        Self {
            bill_id,
            chain,
            bill_keys,
            new_nodes: Vec::new(),
        }
    }

    pub fn new_node(mut self, node: &str) -> Self {
        self.new_nodes.push(node.to_string());
        self
    }

    pub fn bill_id(&self) -> String {
        self.bill_id.to_owned()
    }

    pub fn recipients(&self) -> Result<Vec<String>> {
        Ok(self.chain.get_all_nodes_from_bill(&self.bill_keys)?)
    }

    pub fn lastest_block(&self) -> BillBlock {
        self.chain.get_latest_block().clone()
    }

    pub fn timestamp(&self) -> u64 {
        self.chain.get_latest_block().timestamp()
    }
}
