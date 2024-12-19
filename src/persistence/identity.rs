use super::Result;
use async_trait::async_trait;
use libp2p::PeerId;

use crate::{
    blockchain::identity::IdentityBlock,
    service::identity_service::{Identity, IdentityWithAll},
    util::crypto::BcrKeys,
};
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait IdentityStoreApi: Send + Sync {
    /// Checks if the identity has been created
    async fn exists(&self) -> bool;
    /// Checks if the libp2p credentials for the identity have been created
    async fn libp2p_credentials_exist(&self) -> bool;
    /// Saves the given identity
    async fn save(&self, identity: &Identity) -> Result<()>;
    /// Gets the local identity
    async fn get(&self) -> Result<Identity>;
    /// Gets the local identity with it's node id and key pair
    async fn get_full(&self) -> Result<IdentityWithAll>;
    /// Saves the node id
    async fn save_node_id(&self, node_id: &PeerId) -> Result<()>;
    /// Gets the local node id
    async fn get_node_id(&self) -> Result<PeerId>;
    /// Saves the given key pair
    async fn save_key_pair(&self, key_pair: &BcrKeys) -> Result<()>;
    /// Gets the local key pair
    async fn get_key_pair(&self) -> Result<BcrKeys>;
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait IdentityChainStoreApi: Send + Sync {
    /// Gets the latest block of the chain
    async fn get_latest_block(&self) -> Result<IdentityBlock>;
    /// Adds the block to the chain
    async fn add_block(&self, block: &IdentityBlock) -> Result<()>;
}
