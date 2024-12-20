use super::Result;
use crate::CONFIG;
use crate::{
    dht::Client,
    persistence::identity::IdentityStoreApi,
    util::{self, BcrKeys},
};

use crate::blockchain::identity::{IdentityBlock, IdentityBlockchain, IdentityUpdateBlockData};
use crate::blockchain::Blockchain;
use crate::persistence::identity::IdentityChainStoreApi;
use async_trait::async_trait;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use libp2p::PeerId;
use rocket::serde::{Deserialize, Serialize};
use std::sync::Arc;

#[async_trait]
pub trait IdentityServiceApi: Send + Sync {
    /// Updates the identity
    async fn update_identity(
        &self,
        name: Option<String>,
        company: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        timestamp: i64,
    ) -> Result<()>;
    /// Gets the full local identity, including the key pair and node id
    async fn get_full_identity(&self) -> Result<IdentityWithAll>;
    /// Gets the local identity
    async fn get_identity(&self) -> Result<Identity>;
    /// Gets the local node
    async fn get_node_id(&self) -> Result<PeerId>;
    /// Checks if the identity has been created
    async fn identity_exists(&self) -> bool;
    /// Creates the identity and returns it with it's key pair and node id
    async fn create_identity(
        &self,
        name: String,
        company: String,
        date_of_birth: String,
        city_of_birth: String,
        country_of_birth: String,
        email: String,
        postal_address: String,
        timestamp: i64,
    ) -> Result<()>;
}

/// The identity service is responsible for managing the local identity and syncing it
/// with the dht data.
#[derive(Clone)]
pub struct IdentityService {
    client: Client,
    store: Arc<dyn IdentityStoreApi>,
    blockchain_store: Arc<dyn IdentityChainStoreApi>,
}

impl IdentityService {
    pub fn new(
        client: Client,
        store: Arc<dyn IdentityStoreApi>,
        blockchain_store: Arc<dyn IdentityChainStoreApi>,
    ) -> Self {
        Self {
            client,
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
        company: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        timestamp: i64,
    ) -> Result<()> {
        let mut identity = self.store.get().await?;
        let mut changed = false;

        if let Some(ref name_to_set) = name {
            if identity.name != name_to_set.trim() {
                identity.name = name_to_set.trim().to_owned();
                changed = true;
            }
        }

        if let Some(ref company_to_set) = company {
            if identity.company != company_to_set.trim() {
                identity.company = company_to_set.trim().to_owned();
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
                company,
                email,
                postal_address,
            },
            &keys,
            &identity.public_key_pem,
            timestamp,
        )?;
        self.blockchain_store.add_block(&new_block).await?;

        self.store.save(&identity).await?;
        self.client
            .clone()
            .put_identity_public_data_in_dht()
            .await?;
        Ok(())
    }

    async fn get_identity(&self) -> Result<Identity> {
        let identity = self.store.get().await?;
        Ok(identity)
    }

    async fn get_node_id(&self) -> Result<PeerId> {
        let node_id = self.store.get_node_id().await?;
        Ok(node_id)
    }

    async fn identity_exists(&self) -> bool {
        self.store.exists().await
    }

    async fn create_identity(
        &self,
        name: String,
        company: String,
        date_of_birth: String,
        city_of_birth: String,
        country_of_birth: String,
        email: String,
        postal_address: String,
        timestamp: i64,
    ) -> Result<()> {
        let (private_key_pem, public_key_pem) = util::rsa::create_rsa_key_pair()?;

        let keys = self.store.get_key_pair().await?;
        let node_id = self.store.get_node_id().await?.to_string();

        let s = bitcoin::secp256k1::Secp256k1::new();

        let private_key = bitcoin::PrivateKey::new(
            s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
                .0,
            CONFIG.bitcoin_network(),
        );
        let public_key = private_key.public_key(&s).to_string();
        let private_key = private_key.to_string();

        let identity = Identity {
            name,
            company,
            date_of_birth,
            city_of_birth,
            country_of_birth,
            email,
            postal_address,
            public_key_pem,
            private_key_pem,
            bitcoin_public_key: public_key,
            bitcoin_private_key: private_key.clone(),
            nostr_npub: None,
            nostr_relay: None,
        };

        let rsa_pub_key = identity.public_key_pem.clone();
        // create new identity chain and persist it
        let identity_chain = IdentityBlockchain::new(
            &identity.clone().into(),
            &node_id,
            &keys,
            &rsa_pub_key,
            timestamp,
        )?;
        let first_block = identity_chain.get_first_block();
        self.blockchain_store.add_block(first_block).await?;

        // persist the identity in the DB
        self.store.save(&identity).await?;
        self.client
            .clone()
            .put_identity_public_data_in_dht()
            .await?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct IdentityWithAll {
    pub identity: Identity,
    pub node_id: PeerId,
    #[allow(dead_code)]
    pub key_pair: BcrKeys,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub company: String,
    pub date_of_birth: String,
    pub city_of_birth: String,
    pub country_of_birth: String,
    pub email: String,
    pub postal_address: String,
    pub public_key_pem: String,
    pub private_key_pem: String,
    pub bitcoin_public_key: String,
    pub bitcoin_private_key: String,
    pub nostr_npub: Option<String>,
    pub nostr_relay: Option<String>,
}

impl Identity {
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            company: "".to_string(),
            date_of_birth: "".to_string(),
            city_of_birth: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            public_key_pem: "".to_string(),
            email: "".to_string(),
            country_of_birth: "".to_string(),
            private_key_pem: "".to_string(),
            bitcoin_private_key: "".to_string(),
            nostr_npub: None,
            nostr_relay: None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        persistence::{
            self, bill::MockBillStoreApi, company::MockCompanyStoreApi,
            file_upload::MockFileUploadStoreApi, identity::MockIdentityChainStoreApi,
            identity::MockIdentityStoreApi,
        },
        tests::test::TEST_PUB_KEY,
    };
    use futures::channel::mpsc;

    fn get_service(mock_storage: MockIdentityStoreApi) -> IdentityService {
        let (sender, _) = mpsc::channel(0);
        let mut client_storage = MockIdentityStoreApi::new();
        client_storage.expect_exists().returning(|| false);
        IdentityService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(client_storage),
                Arc::new(MockFileUploadStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(MockIdentityChainStoreApi::new()),
        )
    }

    fn get_service_with_chain_storage(
        mock_storage: MockIdentityStoreApi,
        mock_chain_storage: MockIdentityChainStoreApi,
    ) -> IdentityService {
        let (sender, _) = mpsc::channel(0);
        let mut client_storage = MockIdentityStoreApi::new();
        client_storage.expect_exists().returning(|| false);
        IdentityService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(client_storage),
                Arc::new(MockFileUploadStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(mock_chain_storage),
        )
    }

    #[tokio::test]
    async fn create_identity_baseline() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_save().returning(move |_| Ok(()));
        storage
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        storage
            .expect_get_node_id()
            .returning(move || Ok(PeerId::random()));
        let mut chain_storage = MockIdentityChainStoreApi::new();
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .create_identity(
                "name".to_string(),
                "company".to_string(),
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
    async fn get_node_id_calls_storage() {
        let node_id = PeerId::random();
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_get_node_id().returning(move || Ok(node_id));

        let service = get_service(storage);
        let res = service.get_node_id().await;

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), node_id);
    }

    #[tokio::test]
    async fn get_node_id_propagates_errors() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_get_node_id().returning(|| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });

        let service = get_service(storage);
        let res = service.get_node_id().await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn update_identity_calls_storage() {
        let mut storage = MockIdentityStoreApi::new();
        storage.expect_save().returning(|_| Ok(()));
        storage
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        storage.expect_get().returning(move || {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(identity)
        });
        let mut chain_storage = MockIdentityChainStoreApi::new();
        chain_storage.expect_get_latest_block().returning(|| {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(IdentityBlockchain::new(
                &identity.into(),
                &PeerId::random().to_string(),
                &BcrKeys::new(),
                TEST_PUB_KEY,
                1731593928,
            )
            .unwrap()
            .get_latest_block()
            .clone())
        });
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .update_identity(Some("new_name".to_string()), None, None, None, 1731593928)
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
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            identity.name = "name".to_string();
            Ok(identity)
        });

        let service = get_service(storage);
        let res = service
            .update_identity(Some("name".to_string()), None, None, None, 1731593928)
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
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
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
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(IdentityBlockchain::new(
                &identity.into(),
                &PeerId::random().to_string(),
                &BcrKeys::new(),
                TEST_PUB_KEY,
                1731593928,
            )
            .unwrap()
            .get_latest_block()
            .clone())
        });
        chain_storage.expect_add_block().returning(|_| Ok(()));

        let service = get_service_with_chain_storage(storage, chain_storage);
        let res = service
            .update_identity(Some("new_name".to_string()), None, None, None, 1731593928)
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
            node_id: PeerId::random(),
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
