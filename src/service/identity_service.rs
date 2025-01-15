use super::Result;
use crate::CONFIG;
use crate::{persistence::identity::IdentityStoreApi, util::BcrKeys};

use crate::blockchain::identity::{IdentityBlock, IdentityBlockchain, IdentityUpdateBlockData};
use crate::blockchain::Blockchain;
use crate::persistence::identity::IdentityChainStoreApi;
use async_trait::async_trait;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[async_trait]
pub trait IdentityServiceApi: Send + Sync {
    /// Updates the identity
    async fn update_identity(
        &self,
        name: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        timestamp: u64,
    ) -> Result<()>;
    /// Gets the full local identity, including the key pair and node id
    async fn get_full_identity(&self) -> Result<IdentityWithAll>;
    /// Gets the local identity
    async fn get_identity(&self) -> Result<Identity>;
    /// Checks if the identity has been created
    async fn identity_exists(&self) -> bool;
    /// Creates the identity and returns it with it's key pair and node id
    async fn create_identity(
        &self,
        name: String,
        date_of_birth: String,
        city_of_birth: String,
        country_of_birth: String,
        email: String,
        postal_address: String,
        timestamp: u64,
    ) -> Result<()>;
}

/// The identity service is responsible for managing the local identity and syncing it
/// with the dht data.
#[derive(Clone)]
pub struct IdentityService {
    store: Arc<dyn IdentityStoreApi>,
    blockchain_store: Arc<dyn IdentityChainStoreApi>,
}

impl IdentityService {
    pub fn new(
        store: Arc<dyn IdentityStoreApi>,
        blockchain_store: Arc<dyn IdentityChainStoreApi>,
    ) -> Self {
        Self {
            store,
            blockchain_store,
        }
    }
}

#[async_trait]
impl IdentityServiceApi for IdentityService {
    async fn get_full_identity(&self) -> Result<IdentityWithAll> {
        let identity = self.store.get_full().await?;
        Ok(identity)
    }

    async fn update_identity(
        &self,
        name: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        timestamp: u64,
    ) -> Result<()> {
        let mut identity = self.store.get().await?;
        let mut changed = false;

        if let Some(ref name_to_set) = name {
            if identity.name != name_to_set.trim() {
                identity.name = name_to_set.trim().to_owned();
                changed = true;
            }
        }

        if let Some(ref email_to_set) = email {
            if identity.email != email_to_set.trim() {
                identity.email = email_to_set.trim().to_owned();
                changed = true;
            }
        }

        if let Some(ref postal_address_to_set) = postal_address {
            if identity.postal_address != postal_address_to_set.trim() {
                identity.postal_address = postal_address_to_set.trim().to_owned();
                changed = true;
            }
        }

        if !changed {
            return Ok(());
        }

        let keys = self.store.get_key_pair().await?;

        let previous_block = self.blockchain_store.get_latest_block().await?;
        let new_block = IdentityBlock::create_block_for_update(
            &previous_block,
            &IdentityUpdateBlockData {
                name,
                email,
                postal_address,
            },
            &keys,
            timestamp,
        )?;
        self.blockchain_store.add_block(&new_block).await?;

        self.store.save(&identity).await?;
        Ok(())
    }

    async fn get_identity(&self) -> Result<Identity> {
        let identity = self.store.get().await?;
        Ok(identity)
    }

    async fn identity_exists(&self) -> bool {
        self.store.exists().await
    }

    async fn create_identity(
        &self,
        name: String,
        date_of_birth: String,
        city_of_birth: String,
        country_of_birth: String,
        email: String,
        postal_address: String,
        timestamp: u64,
    ) -> Result<()> {
        let keys = self.store.get_or_create_key_pair().await?;
        let node_id = keys.get_public_key();

        let identity = Identity {
            node_id: node_id.clone(),
            name,
            date_of_birth,
            city_of_birth,
            country_of_birth,
            email,
            postal_address,
            nostr_relay: Some(CONFIG.nostr_relay.to_owned()),
        };

        // create new identity chain and persist it
        let identity_chain = IdentityBlockchain::new(&identity.clone().into(), &keys, timestamp)?;
        let first_block = identity_chain.get_first_block();
        self.blockchain_store.add_block(first_block).await?;

        // persist the identity in the DB
        self.store.save(&identity).await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct IdentityWithAll {
    pub identity: Identity,
    pub key_pair: BcrKeys,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub node_id: String,
    pub date_of_birth: String,
    pub city_of_birth: String,
    pub country_of_birth: String,
    pub email: String,
    pub postal_address: String,
    pub nostr_relay: Option<String>,
}

impl Identity {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            node_id: "".to_string(),
            date_of_birth: "".to_string(),
            city_of_birth: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            country_of_birth: "".to_string(),
            nostr_relay: None,
        }
    }

    pub fn get_nostr_name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityToReturn {
    pub name: String,
    pub node_id: String,
    pub bitcoin_public_key: String,
    pub npub: String,
    pub date_of_birth: String,
    pub city_of_birth: String,
    pub country_of_birth: String,
    pub email: String,
    pub postal_address: String,
    pub nostr_relay: Option<String>,
}

impl IdentityToReturn {
    pub fn from(identity: Identity, keys: BcrKeys) -> Result<Self> {
        Ok(Self {
            name: identity.name,
            node_id: identity.node_id.clone(),
            bitcoin_public_key: identity.node_id.clone(),
            npub: keys.get_nostr_npub()?,
            date_of_birth: identity.date_of_birth,
            city_of_birth: identity.city_of_birth,
            country_of_birth: identity.country_of_birth,
            email: identity.email,
            postal_address: identity.postal_address,
            nostr_relay: identity.nostr_relay,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::persistence::{
        self,
        identity::{MockIdentityChainStoreApi, MockIdentityStoreApi},
    };

    fn get_service(mock_storage: MockIdentityStoreApi) -> IdentityService {
        IdentityService::new(
            Arc::new(mock_storage),
            Arc::new(MockIdentityChainStoreApi::new()),
        )
    }

    fn get_service_with_chain_storage(
        mock_storage: MockIdentityStoreApi,
        mock_chain_storage: MockIdentityChainStoreApi,
    ) -> IdentityService {
        IdentityService::new(Arc::new(mock_storage), Arc::new(mock_chain_storage))
    }

    #[tokio::test]
    async fn create_identity_baseline() {
        let mut storage = MockIdentityStoreApi::new();
        storage
            .expect_get_or_create_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        storage.expect_save().returning(move |_| Ok(()));
        storage
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        let mut chain_storage = MockIdentityChainStoreApi::new();
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .create_identity(
                "name".to_string(),
                "date_of_birth".to_string(),
                "city_of_birth".to_string(),
                "country_of_birth".to_string(),
                "email".to_string(),
                "postal_address".to_string(),
                1731593928,
            )
            .await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn update_identity_calls_storage() {
        let keys = BcrKeys::new();
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_save().returning(|_| Ok(()));
        storage
            .expect_get_key_pair()
            .returning(move || Ok(keys.clone()));
        storage.expect_get().returning(move || {
            let identity = Identity::new_empty();
            Ok(identity)
        });
        let mut chain_storage = MockIdentityChainStoreApi::new();
        chain_storage.expect_get_latest_block().returning(|| {
            let identity = Identity::new_empty();
            Ok(
                IdentityBlockchain::new(&identity.into(), &BcrKeys::new(), 1731593928)
                    .unwrap()
                    .get_latest_block()
                    .clone(),
            )
        });
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .update_identity(Some("new_name".to_string()), None, None, 1731593928)
            .await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn update_identity_returns_if_no_changes_were_made() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_save().returning(|_| Ok(()));
        storage
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        storage.expect_get().returning(move || {
            let mut identity = Identity::new_empty();
            identity.name = "name".to_string();
            Ok(identity)
        });

        let service = get_service(storage);
        let res = service
            .update_identity(Some("name".to_string()), None, None, 1731593928)
            .await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn update_identity_propagates_errors() {
        let mut storage = MockIdentityStoreApi::new();
        storage
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        storage.expect_get().returning(move || {
            let identity = Identity::new_empty();
            Ok(identity)
        });
        storage.expect_save().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let mut chain_storage = MockIdentityChainStoreApi::new();
        chain_storage.expect_get_latest_block().returning(|| {
            let identity = Identity::new_empty();
            Ok(
                IdentityBlockchain::new(&identity.into(), &BcrKeys::new(), 1731593928)
                    .unwrap()
                    .get_latest_block()
                    .clone(),
            )
        });
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .update_identity(Some("new_name".to_string()), None, None, 1731593928)
            .await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn identity_exists_calls_storage() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_exists().returning(|| true);

        let service = get_service(storage);
        let res = service.identity_exists().await;

        assert!(res);
    }

    #[tokio::test]
    async fn get_identity_calls_storage() {
        let identity = Identity::new_empty();
        let mut storage = MockIdentityStoreApi::new();
        storage
            .expect_get()
            .returning(move || Ok(Identity::new_empty()));

        let service = get_service(storage);
        let res = service.get_identity().await;

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), identity);
    }

    #[tokio::test]
    async fn get_identity_propagates_errors() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_get().returning(|| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });

        let service = get_service(storage);
        let res = service.get_identity().await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_full_identity_calls_storage() {
        let identity = IdentityWithAll {
            identity: Identity::new_empty(),
            key_pair: BcrKeys::new(),
        };
        let arced = Arc::new(identity.clone());
        let mut storage = MockIdentityStoreApi::new();
        storage
            .expect_get_full()
            .returning(move || Ok((*arced.clone()).clone()));

        let service = get_service(storage);
        let res = service.get_full_identity().await;

        assert!(res.is_ok());
        assert_eq!(res.unwrap().identity, identity.identity);
    }

    #[tokio::test]
    async fn get_full_identity_propagates_errors() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_get_full().returning(|| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });

        let service = get_service(storage);
        let res = service.get_full_identity().await;

        assert!(res.is_err());
    }
}
