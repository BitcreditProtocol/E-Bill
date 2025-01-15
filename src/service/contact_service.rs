use borsh_derive::{self, BorshDeserialize, BorshSerialize};
use std::sync::Arc;
use utoipa::ToSchema;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};

use crate::{
    persistence::{file_upload::FileUploadStoreApi, identity::IdentityStoreApi, ContactStoreApi},
    service::identity_service::Identity,
    util,
    web::data::File,
    CONFIG,
};

use super::{company_service::Company, Result};
use log::info;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ContactServiceApi: Send + Sync {
    /// Returns all contacts in short form
    async fn get_contacts(&self) -> Result<Vec<Contact>>;

    /// Returns the contact details for the given node_id
    async fn get_contact(&self, node_id: &str) -> Result<Contact>;

    /// Returns the contact by node id
    async fn get_identity_by_node_id(&self, node_id: &str) -> Result<Option<IdentityPublicData>>;

    /// Deletes the contact with the given node_id.
    async fn delete(&self, node_id: &str) -> Result<()>;

    /// Updates the contact with the given data.
    async fn update_contact(
        &self,
        node_id: &str,
        name: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        avatar_file_upload_id: Option<String>,
    ) -> Result<()>;

    /// Adds a new contact
    async fn add_contact(
        &self,
        node_id: &str,
        t: u64,
        name: String,
        email: String,
        postal_address: String,
        date_of_birth_or_registration: Option<String>,
        country_of_birth_or_registration: Option<String>,
        city_of_birth_or_registration: Option<String>,
        identification_number: Option<String>,
        avatar_file_upload_id: Option<String>,
        proof_document_file_upload_id: Option<String>,
    ) -> Result<Contact>;

    /// Returns whether a given npub (as hex) is in our contact list.
    async fn is_known_npub(&self, npub: &str) -> Result<bool>;

    /// opens and decrypts the attached file from the given contact
    async fn open_and_decrypt_file(
        &self,
        id: &str,
        file_name: &str,
        private_key: &str,
    ) -> Result<Vec<u8>>;
}

/// The contact service is responsible for managing the contacts and syncing them with the
/// dht data.
#[derive(Clone)]
pub struct ContactService {
    store: Arc<dyn ContactStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
}

impl ContactService {
    pub fn new(
        store: Arc<dyn ContactStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
    ) -> Self {
        Self {
            store,
            file_upload_store,
            identity_store,
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
        info!("Saved contact file {file_name} with hash {file_hash} for contact {node_id}");
        Ok(File {
            name: file_name.to_owned(),
            hash: file_hash,
        })
    }
}

#[async_trait]
impl ContactServiceApi for ContactService {
    async fn get_contacts(&self) -> Result<Vec<Contact>> {
        let contact_map = self.store.get_map().await?;
        let contact_list: Vec<Contact> = contact_map.into_values().collect();
        Ok(contact_list)
    }

    async fn get_contact(&self, node_id: &str) -> Result<Contact> {
        let res = self.store.get(node_id).await?;
        match res {
            None => Err(super::Error::NotFound),
            Some(contact) => Ok(contact),
        }
    }

    async fn get_identity_by_node_id(&self, node_id: &str) -> Result<Option<IdentityPublicData>> {
        let res = self.store.get(node_id).await?;
        Ok(res.map(|c| c.into()))
    }

    async fn delete(&self, node_id: &str) -> Result<()> {
        self.store.delete(node_id).await?;
        Ok(())
    }

    async fn update_contact(
        &self,
        node_id: &str,
        name: Option<String>,
        email: Option<String>,
        postal_address: Option<String>,
        avatar_file_upload_id: Option<String>,
    ) -> Result<()> {
        let mut contact = match self.store.get(node_id).await? {
            Some(contact) => contact,
            None => {
                return Err(super::Error::NotFound);
            }
        };

        let identity_public_key = self.identity_store.get_key_pair().await?.get_public_key();

        if let Some(ref name_to_set) = name {
            contact.name = name_to_set.clone();
        }

        if let Some(ref email_to_set) = email {
            contact.email = email_to_set.clone();
        }

        if let Some(ref postal_address_to_set) = postal_address {
            contact.postal_address = postal_address_to_set.clone();
        }

        let avatar_file = self
            .process_upload_file(&avatar_file_upload_id, node_id, &identity_public_key)
            .await?;
        contact.avatar_file = avatar_file;

        self.store.update(node_id, contact).await?;

        Ok(())
    }

    async fn add_contact(
        &self,
        node_id: &str,
        t: u64,
        name: String,
        email: String,
        postal_address: String,
        date_of_birth_or_registration: Option<String>,
        country_of_birth_or_registration: Option<String>,
        city_of_birth_or_registration: Option<String>,
        identification_number: Option<String>,
        avatar_file_upload_id: Option<String>,
        proof_document_file_upload_id: Option<String>,
    ) -> Result<Contact> {
        let identity_public_key = self.identity_store.get_key_pair().await?.get_public_key();
        let avatar_file = self
            .process_upload_file(&avatar_file_upload_id, node_id, &identity_public_key)
            .await?;

        let proof_document_file = self
            .process_upload_file(
                &proof_document_file_upload_id,
                node_id,
                &identity_public_key,
            )
            .await?;

        let contact = Contact {
            node_id: node_id.to_owned(),
            t: ContactType::try_from(t)?,
            name,
            email,
            postal_address,
            date_of_birth_or_registration,
            country_of_birth_or_registration,
            city_of_birth_or_registration,
            identification_number,
            avatar_file,
            proof_document_file,
            nostr_relays: vec![CONFIG.nostr_relay.clone()], // Use the configured relay for now
        };

        self.store.insert(node_id, contact.clone()).await?;
        Ok(contact)
    }

    async fn is_known_npub(&self, npub: &str) -> Result<bool> {
        let node_id_list: Vec<String> = self.store.get_map().await?.into_keys().collect();
        Ok(node_id_list
            .iter()
            .any(|node_id| util::crypto::is_node_id_nostr_hex_npub(node_id, npub)))
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
pub enum ContactType {
    Person = 0,
    Company = 1,
}

impl TryFrom<u64> for ContactType {
    type Error = super::Error;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(ContactType::Person),
            1 => Ok(ContactType::Company),
            _ => Err(super::Error::Validation(format!(
                "Invalid contact type found: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub postal_address: String,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file: Option<File>,
    pub proof_document_file: Option<File>,
    pub nostr_relays: Vec<String>,
}

#[derive(
    BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Eq, PartialEq, ToSchema,
)]
pub struct IdentityPublicData {
    /// The type of identity (0 = person, 1 = company)
    #[serde(rename = "type")]
    pub t: ContactType,
    /// The P2P node id of the identity
    pub node_id: String,
    /// The name of the identity
    pub name: String,
    /// Full postal address of the identity
    pub postal_address: String,
    /// email address of the identity
    pub email: String,
    /// The preferred Nostr relay to deliver Nostr messages to
    pub nostr_relay: Option<String>,
}

impl From<Contact> for IdentityPublicData {
    fn from(value: Contact) -> Self {
        Self {
            t: value.t,
            node_id: value.node_id.clone(),
            name: value.name,
            postal_address: value.postal_address,
            email: value.email,
            nostr_relay: value.nostr_relays.first().cloned(),
        }
    }
}

impl From<Company> for IdentityPublicData {
    fn from(value: Company) -> Self {
        Self {
            t: ContactType::Company,
            node_id: value.id.clone(),
            name: value.name,
            postal_address: value.postal_address,
            email: value.email,
            nostr_relay: None,
        }
    }
}

impl IdentityPublicData {
    pub fn new(identity: Identity) -> Self {
        Self {
            t: ContactType::Person,
            node_id: identity.node_id,
            name: identity.name,
            postal_address: identity.postal_address,
            email: identity.email,
            nostr_relay: identity.nostr_relay,
        }
    }

    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            t: ContactType::Person,
            node_id: "".to_string(),
            name: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            nostr_relay: None,
        }
    }

    #[cfg(test)]
    pub fn new_only_node_id(node_id: String) -> Self {
        Self {
            t: ContactType::Person,
            node_id,
            name: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            nostr_relay: None,
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{
        persistence::{
            contact::MockContactStoreApi, db::contact::tests::get_baseline_contact,
            file_upload::MockFileUploadStoreApi, identity::MockIdentityStoreApi,
        },
        tests::test::{TEST_NODE_ID_SECP, TEST_NODE_ID_SECP_AS_NPUB_HEX},
    };
    use std::collections::HashMap;
    use util::BcrKeys;

    fn get_service(
        mock_storage: MockContactStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
    ) -> ContactService {
        ContactService::new(
            Arc::new(mock_storage),
            Arc::new(mock_file_upload_storage),
            Arc::new(mock_identity_storage),
        )
    }

    fn get_storages() -> (
        MockContactStoreApi,
        MockFileUploadStoreApi,
        MockIdentityStoreApi,
    ) {
        (
            MockContactStoreApi::new(),
            MockFileUploadStoreApi::new(),
            MockIdentityStoreApi::new(),
        )
    }

    #[tokio::test]
    async fn get_contacts_baseline() {
        let (mut store, file_upload_store, identity_store) = get_storages();
        store.expect_get_map().returning(|| {
            let mut contact = get_baseline_contact();
            contact.name = "Minka".to_string();
            let mut map = HashMap::new();
            map.insert(TEST_NODE_ID_SECP.to_string(), contact);
            Ok(map)
        });
        let result = get_service(store, file_upload_store, identity_store)
            .get_contacts()
            .await;
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().first().unwrap().name, *"Minka");
        assert_eq!(
            result.as_ref().unwrap().first().unwrap().node_id,
            *TEST_NODE_ID_SECP
        );
    }

    #[tokio::test]
    async fn get_identity_by_node_id_baseline() {
        let (mut store, file_upload_store, identity_store) = get_storages();
        store.expect_get().returning(|_| {
            let mut contact = get_baseline_contact();
            contact.name = "Minka".to_string();
            Ok(Some(contact))
        });
        let result = get_service(store, file_upload_store, identity_store)
            .get_identity_by_node_id(TEST_NODE_ID_SECP)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().as_ref().unwrap().name, *"Minka");
    }

    #[tokio::test]
    async fn delete_contact() {
        let (mut store, file_upload_store, identity_store) = get_storages();
        store.expect_delete().returning(|_| Ok(()));
        let result = get_service(store, file_upload_store, identity_store)
            .delete("some_name")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_contact_calls_store() {
        let (mut store, file_upload_store, mut identity_store) = get_storages();
        identity_store
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        store.expect_get().returning(|_| {
            let contact = get_baseline_contact();
            Ok(Some(contact))
        });
        store.expect_update().returning(|_, _| Ok(()));
        let result = get_service(store, file_upload_store, identity_store)
            .update_contact(
                TEST_NODE_ID_SECP,
                Some("new_name".to_string()),
                None,
                None,
                None,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_contact_calls_store() {
        let (mut store, file_upload_store, mut identity_store) = get_storages();
        identity_store
            .expect_get_key_pair()
            .returning(|| Ok(BcrKeys::new()));
        store.expect_insert().returning(|_, _| Ok(()));
        let result = get_service(store, file_upload_store, identity_store)
            .add_contact(
                TEST_NODE_ID_SECP,
                0,
                "some_name".to_string(),
                "some_email@example.com".to_string(),
                "some_address".to_string(),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn is_known_npub_calls_store() {
        let (mut store, file_upload_store, identity_store) = get_storages();
        store.expect_get_map().returning(|| {
            let contact = get_baseline_contact();
            let mut map = HashMap::new();
            map.insert(TEST_NODE_ID_SECP.to_string(), contact);
            Ok(map)
        });
        let result = get_service(store, file_upload_store, identity_store)
            .is_known_npub(TEST_NODE_ID_SECP_AS_NPUB_HEX)
            .await;
        assert!(result.is_ok());
        assert!(result.as_ref().unwrap());
    }
}
