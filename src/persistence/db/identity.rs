use super::{FileDb, OptionalPostalAddressDb, Result};
use crate::{
    data::identity::{Identity, IdentityWithAll},
    persistence::{identity::IdentityStoreApi, Error},
    util::BcrKeys,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, Surreal};

#[derive(Clone)]
pub struct SurrealIdentityStore {
    db: Surreal<Any>,
}

impl SurrealIdentityStore {
    const IDENTITY_TABLE: &'static str = "identity";
    const KEY_TABLE: &'static str = "identity_key";
    const UNIQUE_ID: &'static str = "unique_record";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }
}

impl SurrealIdentityStore {
    async fn get_db_keys(&self) -> Result<Option<KeyDb>> {
        Ok(self.db.select((Self::KEY_TABLE, Self::UNIQUE_ID)).await?)
    }
}

#[async_trait]
impl IdentityStoreApi for SurrealIdentityStore {
    async fn exists(&self) -> bool {
        self.get().await.map(|_| true).unwrap_or(false)
    }

    async fn libp2p_credentials_exist(&self) -> bool {
        self.get_key_pair().await.map(|_| true).unwrap_or(false)
    }

    async fn save(&self, identity: &Identity) -> Result<()> {
        let entity: IdentityDb = identity.into();
        let _: Option<IdentityDb> = self
            .db
            .upsert((Self::IDENTITY_TABLE, Self::UNIQUE_ID))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn get(&self) -> Result<Identity> {
        let result: Option<IdentityDb> = self
            .db
            .select((Self::IDENTITY_TABLE, Self::UNIQUE_ID))
            .await?;
        match result {
            None => Err(Error::NoIdentity),
            Some(i) => Ok(i.into()),
        }
    }

    async fn get_full(&self) -> Result<IdentityWithAll> {
        let results = tokio::join!(self.get(), self.get_key_pair());
        match results {
            (Ok(identity), Ok(key_pair)) => Ok(IdentityWithAll { identity, key_pair }),
            _ => {
                if let Err(e) = results.0 {
                    Err(e)
                } else if let Err(e) = results.1 {
                    Err(e)
                } else {
                    unreachable!("one of the tasks has to have failed");
                }
            }
        }
    }

    async fn save_key_pair(&self, key_pair: &BcrKeys, seed: &str) -> Result<()> {
        let entity: KeyDb = KeyDb::from_generated_keys(key_pair, seed);
        let _: Option<KeyDb> = self
            .db
            .upsert((Self::KEY_TABLE, Self::UNIQUE_ID))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn get_key_pair(&self) -> Result<BcrKeys> {
        let result: Option<KeyDb> = self.get_db_keys().await?;
        match result {
            None => Err(Error::NoIdentityKey),
            Some(value) => value.try_into(),
        }
    }

    async fn get_or_create_key_pair(&self) -> Result<BcrKeys> {
        let keys = match self.get_key_pair().await {
            Ok(keys) => keys,
            _ => {
                let (new_keys, seed) = BcrKeys::new_with_seed_phrase()?;
                self.save_key_pair(&new_keys, &seed).await?;
                new_keys
            }
        };
        Ok(keys)
    }

    async fn get_seedphrase(&self) -> Result<String> {
        let result = self.get_db_keys().await?;
        match result {
            Some(key_db) => Ok(key_db.seed_phrase),
            None => Err(Error::NoSeedPhrase),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityDb {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub postal_address: OptionalPostalAddressDb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub nostr_relay: Option<String>,
    pub profile_picture_file: Option<FileDb>,
    pub identity_document_file: Option<FileDb>,
}

impl From<IdentityDb> for Identity {
    fn from(identity: IdentityDb) -> Self {
        Self {
            node_id: identity.node_id,
            name: identity.name,
            email: identity.email,
            postal_address: identity.postal_address.into(),
            date_of_birth: identity.date_of_birth,
            country_of_birth: identity.country_of_birth,
            city_of_birth: identity.city_of_birth,
            identification_number: identity.identification_number,
            nostr_relay: identity.nostr_relay,
            profile_picture_file: identity.profile_picture_file.map(|f| f.into()),
            identity_document_file: identity.identity_document_file.map(|f| f.into()),
        }
    }
}

impl From<&Identity> for IdentityDb {
    fn from(identity: &Identity) -> Self {
        Self {
            node_id: identity.node_id.clone(),
            name: identity.name.clone(),
            email: identity.email.clone(),
            postal_address: OptionalPostalAddressDb::from(&identity.postal_address),
            date_of_birth: identity.date_of_birth.clone(),
            country_of_birth: identity.country_of_birth.clone(),
            city_of_birth: identity.city_of_birth.clone(),
            identification_number: identity.identification_number.clone(),
            nostr_relay: identity.nostr_relay.clone(),
            profile_picture_file: identity.profile_picture_file.clone().map(|f| (&f).into()),
            identity_document_file: identity.identity_document_file.clone().map(|f| (&f).into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDb {
    pub key: String,
    pub seed_phrase: String,
}

impl KeyDb {
    fn from_generated_keys(key_pair: &BcrKeys, seed: &str) -> Self {
        Self {
            key: key_pair.get_private_key_string(),
            seed_phrase: seed.to_string(),
        }
    }
}

impl TryFrom<KeyDb> for BcrKeys {
    type Error = crate::persistence::Error;
    fn try_from(value: KeyDb) -> Result<Self> {
        let key_pair = BcrKeys::from_private_key(&value.key)?;
        Ok(key_pair)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::db::get_memory_db;

    async fn get_store() -> SurrealIdentityStore {
        let mem_db = get_memory_db("test", "identity")
            .await
            .expect("could not create memory db");
        SurrealIdentityStore::new(mem_db)
    }

    #[tokio::test]
    async fn test_exists() {
        let store = get_store().await;
        assert!(!store.exists().await);
        store.save(&Identity::new_empty()).await.unwrap();
        assert!(store.exists().await)
    }

    #[tokio::test]
    async fn test_libp2p_credentials_exist() {
        let store = get_store().await;
        assert!(!store.libp2p_credentials_exist().await);
        let (keys, seed) = BcrKeys::new_with_seed_phrase().expect("key could not be generated");
        store.save_key_pair(&keys, &seed).await.unwrap();
        assert!(store.libp2p_credentials_exist().await)
    }

    #[tokio::test]
    async fn test_identity() {
        let store = get_store().await;
        let mut identity = Identity::new_empty();
        identity.name = "Minka".to_string();
        store.save(&identity).await.unwrap();
        let fetched_identity = store.get().await.unwrap();
        assert_eq!(identity, fetched_identity);
    }

    #[tokio::test]
    async fn test_full_identity() {
        let store = get_store().await;
        let mut identity = Identity::new_empty();
        identity.name = "Minka".to_string();
        let (keys, seed) = BcrKeys::new_with_seed_phrase().expect("key could not be generated");
        store.save(&identity).await.unwrap();
        store.save_key_pair(&keys, &seed).await.unwrap();
        let fetched_full_identity = store.get_full().await.unwrap();
        assert_eq!(identity.name, fetched_full_identity.identity.name);
        assert_eq!(
            keys.get_public_key(),
            fetched_full_identity.key_pair.get_public_key()
        );
    }

    #[tokio::test]
    async fn test_key_pair() {
        let store = get_store().await;
        let (keys, seed) = BcrKeys::new_with_seed_phrase().expect("key could not be generated");
        store.save_key_pair(&keys, &seed).await.unwrap();
        let fetched_key_pair = store.get_key_pair().await.unwrap();
        assert_eq!(keys.get_public_key(), fetched_key_pair.get_public_key());
    }
}
