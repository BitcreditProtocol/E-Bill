use std::collections::HashSet;

use super::Result;
use crate::{
    blockchain::bill::{BillBlock, BillBlockchain, BillOpCode},
    data::bill::BillKeys,
};
use async_trait::async_trait;

use borsh::{from_slice, to_vec};
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BillStoreApi: Send + Sync {
    /// Checks if the given bill exists
    async fn exists(&self, id: &str) -> bool;
    /// Gets all bill ids
    async fn get_ids(&self) -> Result<Vec<String>>;
    /// Saves the keys
    async fn save_keys(&self, id: &str, keys: &BillKeys) -> Result<()>;
    /// Get bill keys
    async fn get_keys(&self, id: &str) -> Result<BillKeys>;
    /// Check if the given bill was paid
    async fn is_paid(&self, id: &str) -> Result<bool>;
    /// Set the given bill to paid on the given payment address
    async fn set_to_paid(&self, id: &str, payment_address: &str) -> Result<()>;
    /// Gets all bills with a RequestToPay block, which are not paid already
    async fn get_bill_ids_waiting_for_payment(&self) -> Result<Vec<String>>;
    /// Gets all bills where the latest block is OfferToSell, which are still waiting for payment
    async fn get_bill_ids_waiting_for_sell_payment(&self) -> Result<Vec<String>>;
    /// Gets all bills where the latest block is RequestRecourse, which are still waiting for payment
    async fn get_bill_ids_waiting_for_recourse_payment(&self) -> Result<Vec<String>>;
    /// Returns all bill ids that are currently within the given op codes and block not
    /// older than the given timestamp.
    async fn get_bill_ids_with_op_codes_since(
        &self,
        op_code: HashSet<BillOpCode>,
        since: u64,
    ) -> Result<Vec<String>>;
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BillChainStoreApi: Send + Sync {
    /// Gets the latest block of the chain
    async fn get_latest_block(&self, id: &str) -> Result<BillBlock>;
    /// Adds the block to the chain
    async fn add_block(&self, id: &str, block: &BillBlock) -> Result<()>;
    /// Get the whole blockchain
    async fn get_chain(&self, id: &str) -> Result<BillBlockchain>;
}

pub fn bill_chain_from_bytes(bytes: &[u8]) -> Result<BillBlockchain> {
    let chain: BillBlockchain = from_slice(bytes)?;
    Ok(chain)
}

pub fn bill_keys_from_bytes(bytes: &[u8]) -> Result<BillKeys> {
    let keys: BillKeys = from_slice(bytes)?;
    Ok(keys)
}

pub fn bill_keys_to_bytes(keys: &BillKeys) -> Result<Vec<u8>> {
    let bytes = to_vec(&keys)?;
    Ok(bytes)
}

pub fn bill_chain_to_bytes(chain: &BillBlockchain) -> Result<Vec<u8>> {
    let bytes = to_vec(&chain)?;
    Ok(bytes)
}
