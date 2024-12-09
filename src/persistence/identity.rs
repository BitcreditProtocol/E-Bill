use super::Result;
use async_trait::async_trait;
use libp2p::{identity::Keypair, PeerId};

use crate::service::identity_service::{Identity, IdentityWithAll};
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
    /// Gets the local identity with it's peer id and key pair
    async fn get_full(&self) -> Result<IdentityWithAll>;
    /// Saves the peer id
    async fn save_peer_id(&self, peer_id: &PeerId) -> Result<()>;
    /// Gets the local peer id
    async fn get_peer_id(&self) -> Result<PeerId>;
    /// Saves the given key pair
    async fn save_key_pair(&self, key_pair: &Keypair) -> Result<()>;
    /// Gets the local key pair
    async fn get_key_pair(&self) -> Result<Keypair>;
}
