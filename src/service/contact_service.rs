use borsh::{self, BorshDeserialize, BorshSerialize};
use std::{collections::HashMap, sync::Arc};

use async_trait;
use serde::{Deserialize, Serialize};

use crate::{dht::network::Client, persistence::ContactStoreApi, Identity};

use super::Result;

#[async_trait]
pub trait ContactServiceApi: Send + Sync {
    /// Returns all contacts in short form
    async fn get_contacts(&self) -> Result<Vec<Contact>>;

    /// Returns the identity by name. Right now it will refresh the dht data on every
    /// call so some sort of caching might be needed in the future.
    async fn get_identity_by_name(&self, name: &str) -> Result<IdentityPublicData>;

    /// Creates a new identity with the given name and data.
    async fn create_identity(
        &self,
        name: &str,
        identity: IdentityPublicData,
    ) -> Result<IdentityPublicData>;

    /// Deletes the identity with the given name.
    async fn delete_identity_by_name(&self, name: &str) -> Result<()>;

    /// Updates the name of the identity from the given old name to to the new name.
    /// This acts like a primary key update and the entity will only be accessible via
    /// the new name.
    async fn update_identity_name(&self, old_name: &str, new_name: &str) -> Result<()>;

    /// Updates the identity with the given name with the new identity data.
    async fn update_identity(&self, name: &str, identity: IdentityPublicData) -> Result<()>;

    /// Adds a new peer identity to the identity with the given name. The data will be
    /// fetched from the dht. It will be stored with name and peer_id only if no dht entry
    /// exists.
    async fn add_peer_identity(&self, name: &str, peer_id: &str) -> Result<IdentityPublicData>;
}

/// The contact service is responsible for managing the contacts and syncing them with the
/// dht data.
#[derive(Clone)]
pub struct ContactService {
    client: Client,
    store: Arc<dyn ContactStoreApi>,
}

impl ContactService {
    pub fn new(client: Client, store: Arc<dyn ContactStoreApi>) -> Self {
        Self { client, store }
    }
}

#[async_trait]
impl ContactServiceApi for ContactService {
    async fn get_contacts(&self) -> Result<Vec<Contact>> {
        let identities = self.store.get_map().await?;
        Ok(as_contacts(identities))
    }

    async fn get_identity_by_name(&self, name: &str) -> Result<IdentityPublicData> {
        if let Some(identity) = self.store.by_name(name).await? {
            let public = self
                .client
                .clone()
                .get_identity_public_data_from_dht(identity.peer_id.clone())
                .await;

            if !public.name.is_empty() && public.ne(&identity) {
                self.update_identity(name, public.to_owned()).await?;
                Ok(public)
            } else {
                Ok(identity)
            }
        } else {
            Ok(IdentityPublicData::new_empty())
        }
    }

    async fn create_identity(
        &self,
        name: &str,
        identity: IdentityPublicData,
    ) -> Result<IdentityPublicData> {
        self.store.insert(name, identity.to_owned()).await?;
        Ok(identity)
    }

    async fn update_identity(&self, name: &str, identity: IdentityPublicData) -> Result<()> {
        self.store.update(name, identity).await?;
        Ok(())
    }

    async fn delete_identity_by_name(&self, name: &str) -> Result<()> {
        self.store.delete(name).await?;
        Ok(())
    }

    async fn update_identity_name(&self, old_name: &str, new_name: &str) -> Result<()> {
        self.store.update_name(old_name, new_name).await?;
        Ok(())
    }

    async fn add_peer_identity(&self, name: &str, peer_id: &str) -> Result<IdentityPublicData> {
        let default = IdentityPublicData::new_only_peer_id(peer_id.to_owned());
        let public = self
            .client
            .clone()
            .get_identity_public_data_from_dht(peer_id.to_owned())
            .await;

        if public.name.is_empty() {
            self.store.insert(name, default.clone()).await?;
            Ok(default)
        } else {
            self.store.insert(name, public.clone()).await?;
            Ok(public)
        }
    }
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Contact {
    pub name: String,
    pub peer_id: String,
}

// converts identity data to contact data
fn as_contacts(identities: HashMap<String, IdentityPublicData>) -> Vec<Contact> {
    let mut contacts_vec: Vec<Contact> = Vec::new();
    for (name, public_data) in identities {
        contacts_vec.push(Contact {
            name,
            peer_id: public_data.peer_id,
        });
    }
    contacts_vec
}

#[derive(
    BorshSerialize, BorshDeserialize, FromForm, Debug, Serialize, Deserialize, Clone, Eq, PartialEq,
)]
#[serde(crate = "rocket::serde")]
pub struct IdentityPublicData {
    pub peer_id: String,
    pub name: String,
    pub company: String,
    pub bitcoin_public_key: String,
    pub postal_address: String,
    pub email: String,
    pub rsa_public_key_pem: String,
}

impl IdentityPublicData {
    pub fn new(identity: Identity, peer_id: String) -> Self {
        Self {
            peer_id,
            name: identity.name,
            company: identity.company,
            bitcoin_public_key: identity.bitcoin_public_key,
            postal_address: identity.postal_address,
            email: identity.email,
            rsa_public_key_pem: identity.public_key_pem,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            peer_id: "".to_string(),
            name: "".to_string(),
            company: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            rsa_public_key_pem: "".to_string(),
        }
    }

    pub fn new_only_peer_id(peer_id: String) -> Self {
        Self {
            peer_id,
            name: "".to_string(),
            company: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            rsa_public_key_pem: "".to_string(),
        }
    }
}
