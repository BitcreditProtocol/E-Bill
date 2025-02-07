use super::Result;
use crate::{persistence::identity::IdentityStoreApi, util::BcrKeys};
use crate::{util, CONFIG};

use crate::blockchain::identity::{IdentityBlock, IdentityBlockchain, IdentityUpdateBlockData};
use crate::blockchain::Blockchain;
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::IdentityChainStoreApi;
use crate::web::data::{File, OptionalPostalAddress};
use async_trait::async_trait;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[async_trait]
pub trait IdentityServiceApi: Send + Sync {
    /// Updates the identity
    async fn update_identity(
        &self,
        name: Option<String>,
        email: Option<String>,
        postal_address: OptionalPostalAddress,
        profile_picture_file_upload_id: Option<String>,
        timestamp: u64,
    ) -> Result<()>;
    /// Gets the full local identity, including the key pair and node id
    async fn get_full_identity(&self) -> Result<IdentityWithAll>;
    /// Gets the local identity
    async fn get_identity(&self) -> Result<Identity>;
    /// Checks if the identity has been created
    async fn identity_exists(&self) -> bool;
    /// Creates the identity
    async fn create_identity(
        &self,
        name: String,
        email: String,
        postal_address: OptionalPostalAddress,
        date_of_birth: Option<String>,
        country_of_birth: Option<String>,
        city_of_birth: Option<String>,
        identification_number: Option<String>,
        profile_picture_file_upload_id: Option<String>,
        identity_document_file_upload_id: Option<String>,
        timestamp: u64,
    ) -> Result<()>;
    async fn get_seedphrase(&self) -> Result<String>;
    /// Recovers the private keys in the identity from a seed phrase
    async fn recover_from_seedphrase(&self, seed: &str) -> Result<()>;

    /// opens and decrypts the attached file from the identity
    async fn open_and_decrypt_file(
        &self,
        id: &str,
        file_name: &str,
        private_key: &str,
    ) -> Result<Vec<u8>>;
}

#[repr(u8)]
#[derive(
    Debug,
    Clone,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    PartialEq,
    Eq,
    ToSchema,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
pub enum IdentityType {
    Person = 0,
    Company = 1,
}

impl TryFrom<u64> for IdentityType {
    type Error = super::Error;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(IdentityType::Person),
            1 => Ok(IdentityType::Company),
            _ => Err(super::Error::Validation(format!(
                "Invalid identity type found: {value}"
            ))),
        }
    }
}

/// The identity service is responsible for managing the local identity and syncing it
/// with the dht data.
#[derive(Clone)]
pub struct IdentityService {
    store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    blockchain_store: Arc<dyn IdentityChainStoreApi>,
}

impl IdentityService {
    pub fn new(
        store: Arc<dyn IdentityStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        blockchain_store: Arc<dyn IdentityChainStoreApi>,
    ) -> Self {
        Self {
            store,
            file_upload_store,
            blockchain_store,
        }
    }

    async fn process_upload_file(
        &self,
        upload_id: &Option<String>,
        id: &str,
        public_key: &str,
    ) -> Result<Option<File>> {
        if let Some(upload_id) = upload_id {
            let files = self
                .file_upload_store
                .read_temp_upload_files(upload_id)
                .await?;
            if !files.is_empty() {
                let (file_name, file_bytes) = &files[0];
                let file = self
                    .encrypt_and_save_uploaded_file(file_name, file_bytes, id, public_key)
                    .await?;
                return Ok(Some(file));
            }
        }
        Ok(None)
    }

    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        node_id: &str,
        public_key: &str,
    ) -> Result<File> {
        let file_hash = util::sha256_hash(file_bytes);
        let encrypted = util::crypto::encrypt_ecies(file_bytes, public_key)?;
        self.file_upload_store
            .save_attached_file(&encrypted, node_id, file_name)
            .await?;
        info!("Saved identity file {file_name} with hash {file_hash} for identity {node_id}");
        Ok(File {
            name: file_name.to_owned(),
            hash: file_hash,
        })
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
        postal_address: OptionalPostalAddress,
        profile_picture_file_upload_id: Option<String>,
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

        match identity.postal_address.country {
            Some(_) => {
                if let Some(ref postal_address_country_to_set) = postal_address.country {
                    identity.postal_address.country = Some(postal_address_country_to_set.clone());
                    changed = true;
                } else {
                    identity.postal_address.country = None;
                    changed = true;
                }
            }
            None => {
                if let Some(ref postal_address_country_to_set) = postal_address.country {
                    identity.postal_address.country = Some(postal_address_country_to_set.clone());
                    changed = true;
                }
            }
        };

        match identity.postal_address.city {
            Some(_) => {
                if let Some(ref postal_address_city_to_set) = postal_address.city {
                    identity.postal_address.city = Some(postal_address_city_to_set.clone());
                    changed = true;
                } else {
                    identity.postal_address.city = None;
                    changed = true;
                }
            }
            None => {
                if let Some(ref postal_address_city_to_set) = postal_address.city {
                    identity.postal_address.city = Some(postal_address_city_to_set.clone());
                    changed = true;
                }
            }
        };

        match identity.postal_address.zip {
            Some(_) => {
                if let Some(ref postal_address_zip_to_set) = postal_address.zip {
                    identity.postal_address.zip = Some(postal_address_zip_to_set.clone());
                    changed = true;
                } else {
                    identity.postal_address.zip = None;
                    changed = true;
                }
            }
            None => {
                if let Some(ref postal_address_zip_to_set) = postal_address.zip {
                    identity.postal_address.zip = Some(postal_address_zip_to_set.clone());
                    changed = true;
                }
            }
        };

        match identity.postal_address.address {
            Some(_) => {
                if let Some(ref postal_address_address_to_set) = postal_address.address {
                    identity.postal_address.address = Some(postal_address_address_to_set.clone());
                    changed = true;
                } else {
                    identity.postal_address.address = None;
                    changed = true;
                }
            }
            None => {
                if let Some(ref postal_address_address_to_set) = postal_address.address {
                    identity.postal_address.address = Some(postal_address_address_to_set.clone());
                    changed = true;
                }
            }
        };

        if !changed && profile_picture_file_upload_id.is_none() {
            return Ok(());
        }

        let keys = self.store.get_key_pair().await?;
        let profile_picture_file = self
            .process_upload_file(
                &profile_picture_file_upload_id,
                &identity.node_id,
                &keys.get_public_key(),
            )
            .await?;
        identity.profile_picture_file = profile_picture_file.clone();

        let previous_block = self.blockchain_store.get_latest_block().await?;
        let new_block = IdentityBlock::create_block_for_update(
            &previous_block,
            &IdentityUpdateBlockData {
                name,
                email,
                postal_address,
                profile_picture_file,
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
        email: String,
        postal_address: OptionalPostalAddress,
        date_of_birth: Option<String>,
        country_of_birth: Option<String>,
        city_of_birth: Option<String>,
        identification_number: Option<String>,
        profile_picture_file_upload_id: Option<String>,
        identity_document_file_upload_id: Option<String>,
        timestamp: u64,
    ) -> Result<()> {
        let keys = self.store.get_or_create_key_pair().await?;
        let node_id = keys.get_public_key();

        let profile_picture_file = self
            .process_upload_file(
                &profile_picture_file_upload_id,
                &node_id,
                &keys.get_public_key(),
            )
            .await?;

        let identity_document_file = self
            .process_upload_file(
                &identity_document_file_upload_id,
                &node_id,
                &keys.get_public_key(),
            )
            .await?;

        let identity = Identity {
            node_id: node_id.clone(),
            name,
            email,
            postal_address,
            date_of_birth,
            country_of_birth,
            city_of_birth,
            identification_number,
            profile_picture_file,
            identity_document_file,
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

    /// Recovers keys from a seed phrase and stores them into the identity
    async fn recover_from_seedphrase(&self, seed: &str) -> Result<()> {
        let key_pair = BcrKeys::from_seedphrase(seed)?;
        self.store.save_key_pair(&key_pair, seed).await?;
        Ok(())
    }

    async fn get_seedphrase(&self) -> Result<String> {
        let res = self.store.get_seedphrase().await?;
        Ok(res)
    }

    async fn open_and_decrypt_file(
        &self,
        id: &str,
        file_name: &str,
        private_key: &str,
    ) -> Result<Vec<u8>> {
        let read_file = self
            .file_upload_store
            .open_attached_file(id, file_name)
            .await?;
        let decrypted = util::crypto::decrypt_ecies(&read_file, private_key)?;
        Ok(decrypted)
    }
}

#[derive(Clone, Debug)]
pub struct IdentityWithAll {
    pub identity: Identity,
    pub key_pair: BcrKeys,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Identity {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub postal_address: OptionalPostalAddress,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub nostr_relay: Option<String>,
    pub profile_picture_file: Option<File>,
    pub identity_document_file: Option<File>,
}

impl Identity {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            node_id: "".to_string(),
            name: "".to_string(),
            email: "".to_string(),
            postal_address: OptionalPostalAddress::new_empty(),
            date_of_birth: None,
            country_of_birth: None,
            city_of_birth: None,
            identification_number: None,
            nostr_relay: None,
            profile_picture_file: None,
            identity_document_file: None,
        }
    }

    pub fn get_nostr_name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityToReturn {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub bitcoin_public_key: String,
    pub npub: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddress,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file: Option<File>,
    pub identity_document_file: Option<File>,
    pub nostr_relay: Option<String>,
}

impl IdentityToReturn {
    pub fn from(identity: Identity, keys: BcrKeys) -> Result<Self> {
        Ok(Self {
            node_id: identity.node_id.clone(),
            name: identity.name,
            email: identity.email,
            bitcoin_public_key: identity.node_id.clone(),
            npub: keys.get_nostr_npub()?,
            postal_address: identity.postal_address,
            date_of_birth: identity.date_of_birth,
            country_of_birth: identity.country_of_birth,
            city_of_birth: identity.city_of_birth,
            identification_number: identity.identification_number,
            profile_picture_file: identity.profile_picture_file,
            identity_document_file: identity.identity_document_file,
            nostr_relay: identity.nostr_relay,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{
        self,
        file_upload::MockFileUploadStoreApi,
        identity::{MockIdentityChainStoreApi, MockIdentityStoreApi},
    };
    use mockall::predicate::eq;

    fn get_service(mock_storage: MockIdentityStoreApi) -> IdentityService {
        IdentityService::new(
            Arc::new(mock_storage),
            Arc::new(MockFileUploadStoreApi::new()),
            Arc::new(MockIdentityChainStoreApi::new()),
        )
    }

    fn get_service_with_chain_storage(
        mock_storage: MockIdentityStoreApi,
        mock_chain_storage: MockIdentityChainStoreApi,
    ) -> IdentityService {
        IdentityService::new(
            Arc::new(mock_storage),
            Arc::new(MockFileUploadStoreApi::new()),
            Arc::new(mock_chain_storage),
        )
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
                "email".to_string(),
                OptionalPostalAddress::new_empty(),
                None,
                None,
                None,
                None,
                None,
                None,
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
            .update_identity(
                Some("new_name".to_string()),
                None,
                OptionalPostalAddress::new_empty(),
                None,
                1731593928,
            )
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
            .update_identity(
                Some("name".to_string()),
                None,
                OptionalPostalAddress::new_empty(),
                None,
                1731593928,
            )
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
            .update_identity(
                Some("new_name".to_string()),
                None,
                OptionalPostalAddress::new_empty(),
                None,
                1731593928,
            )
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

    #[tokio::test]
    async fn recover_from_seedphrase_stores_key_and_seed() {
        let seed = "forward paper connect economy twelve debate cart isolate accident creek bind predict captain rifle glory cradle hip whisper wealth save buddy place develop dolphin";
        let expected_key = BcrKeys::from_private_key(
            "f31e0373f6fa9f4835d49a278cd48f47ea115af7480edf435275a3c2dbb1f982",
        )
        .expect("valid seed phrase");
        let mut storage = MockIdentityStoreApi::new();
        storage
            .expect_save_key_pair()
            .with(eq(expected_key), eq(seed))
            .returning(|_, _| Ok(()));
        let service = get_service(storage);
        service
            .recover_from_seedphrase(seed)
            .await
            .expect("could not recover from seedphrase")
    }
}
