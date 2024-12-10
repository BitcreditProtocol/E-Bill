use std::collections::HashMap;

use super::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, Surreal};

use crate::{persistence::ContactStoreApi, service::contact_service::IdentityPublicData};

#[derive(Clone)]
pub struct SurrealContactStore {
    db: Surreal<Any>,
}

impl SurrealContactStore {
    const TABLE: &'static str = "contacts";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ContactStoreApi for SurrealContactStore {
    async fn get_map(&self) -> Result<HashMap<String, IdentityPublicData>> {
        let all: Vec<ContactDb> = self.db.select(Self::TABLE).await?;
        let mut map = HashMap::new();
        for contact in all.into_iter() {
            map.insert(contact.name.clone(), contact.into());
        }
        Ok(map)
    }

    async fn by_name(&self, name: &str) -> Result<Option<IdentityPublicData>> {
        let result: Vec<ContactDb> = self
            .db
            .query("SELECT * FROM type::table($table) WHERE name = $name")
            .bind(("table", Self::TABLE))
            .bind(("name", name.to_owned()))
            .await?
            .take(0)?;
        Ok(result.first().map(|c| c.to_owned().into()))
    }

    async fn insert(&self, name: &str, data: IdentityPublicData) -> Result<()> {
        let mut entity: ContactDb = data.into();
        entity.name = name.to_owned();
        let _: Option<ContactDb> = self
            .db
            .create((Self::TABLE, entity.peer_id.to_owned()))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        self.db
            .query("DELETE FROM type::table($table) WHERE name = $name")
            .bind(("table", Self::TABLE))
            .bind(("name", name.to_owned()))
            .await?;
        Ok(())
    }

    async fn update_name(&self, name: &str, new_name: &str) -> Result<()> {
        self.db
            .query("UPDATE type::table($table) SET name = $new_name WHERE name = $name")
            .bind(("table", Self::TABLE))
            .bind(("new_name", new_name.to_owned()))
            .bind(("name", name.to_owned()))
            .await?;
        Ok(())
    }

    async fn update(&self, name: &str, data: IdentityPublicData) -> Result<()> {
        let mut entity: ContactDb = data.into();
        entity.name = name.to_owned();
        let _: Option<ContactDb> = self
            .db
            .update((Self::TABLE, entity.peer_id.to_owned()))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn get_by_npub(&self, npub: &str) -> Result<Option<IdentityPublicData>> {
        let result: Vec<ContactDb> = self
            .db
            .query("SELECT * FROM type::table($table) WHERE nostr_npub = $npub")
            .bind(("table", Self::TABLE))
            .bind(("npub", npub.to_owned()))
            .await?
            .take(0)?;
        Ok(result.first().map(|c| c.to_owned().into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactDb {
    pub peer_id: String,
    pub name: String,
    pub company: Option<String>,
    pub bitcoin_public_key: Option<String>,
    pub postal_address: Option<String>,
    pub email: Option<String>,
    pub rsa_public_key_pem: Option<String>,
    pub nostr_npub: Option<String>,
    pub nostr_relays: Vec<String>,
}

impl From<ContactDb> for IdentityPublicData {
    fn from(contact: ContactDb) -> Self {
        Self {
            peer_id: contact.peer_id,
            name: contact.name,
            company: contact.company.unwrap_or("".to_owned()),
            bitcoin_public_key: contact.bitcoin_public_key.unwrap_or("".to_owned()),
            postal_address: contact.postal_address.unwrap_or("".to_owned()),
            email: contact.email.unwrap_or("".to_owned()),
            rsa_public_key_pem: contact.rsa_public_key_pem.unwrap_or("".to_owned()),
            nostr_npub: contact.nostr_npub,
            nostr_relay: contact.nostr_relays.first().map(|v| v.to_owned()),
        }
    }
}

impl From<IdentityPublicData> for ContactDb {
    fn from(value: IdentityPublicData) -> Self {
        Self {
            peer_id: value.peer_id,
            name: value.name,
            company: as_opt(value.company),
            bitcoin_public_key: as_opt(value.bitcoin_public_key),
            postal_address: as_opt(value.postal_address),
            email: as_opt(value.email),
            rsa_public_key_pem: as_opt(value.rsa_public_key_pem),
            nostr_npub: value.nostr_npub,
            nostr_relays: value.nostr_relay.into_iter().collect(),
        }
    }
}

fn as_opt(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::db::get_memory_db;

    #[tokio::test]
    async fn test_insert_contact() {
        let store = get_store().await;
        let identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());
        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        let stored = store
            .by_name("name")
            .await
            .expect("could not query contact")
            .expect("could not find created contact");

        assert_eq!(&stored.name, "name");
        assert_eq!(&stored.peer_id, &identity.peer_id);
    }

    #[tokio::test]
    async fn test_delete_contact() {
        let store = get_store().await;
        let identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());
        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        let stored = store
            .by_name("name")
            .await
            .expect("could not query contact")
            .expect("could not find created contact");

        assert_eq!(&stored.name, "name");

        store
            .delete("name")
            .await
            .expect("could not delete contact");

        let empty = store
            .by_name("name")
            .await
            .expect("could not query deleted contact");
        assert!(empty.is_none());
    }

    #[tokio::test]
    async fn test_update_contact() {
        let store = get_store().await;
        let identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());

        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        let mut data = identity.clone();
        data.company = "company".to_string();
        store
            .update("name", data)
            .await
            .expect("could not update contact");

        let updated = store
            .by_name("name")
            .await
            .expect("could not query contact")
            .expect("could not find created contact");

        assert_eq!(&updated.name, "name");
        assert_eq!(&updated.company, "company");
    }

    #[tokio::test]
    async fn test_update_name() {
        let store = get_store().await;
        let identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());

        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        store
            .update_name("name", "new_name")
            .await
            .expect("could not update contact name");

        let updated = store
            .by_name("new_name")
            .await
            .expect("could not query contact")
            .expect("could not find contact with new name");

        assert_eq!(&updated.name, "new_name");
    }

    #[tokio::test]
    async fn test_get_map() {
        let store = get_store().await;
        let identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());
        let identity2 = IdentityPublicData::new_only_peer_id("peer_id2".to_string());
        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        store
            .insert("name2", identity2.clone())
            .await
            .expect("could not create contact 2");

        let all = store.get_map().await.expect("all query failed");
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("name"));
        assert!(all.contains_key("name2"));
        assert_eq!(all.get("name2").unwrap().peer_id, "peer_id2");
    }

    async fn get_store() -> SurrealContactStore {
        let mem_db = get_memory_db("test", "contact")
            .await
            .expect("could not create get_memory_db");
        SurrealContactStore::new(mem_db)
    }

    #[tokio::test]
    async fn test_get_by_npub() {
        let store = get_store().await;
        let mut identity = IdentityPublicData::new_only_peer_id("peer_id".to_string());
        identity.nostr_npub = Some("npub".to_owned());
        identity.nostr_relay = Some("wss://example.relay".to_owned());

        store
            .insert("name", identity.clone())
            .await
            .expect("could not create contact");

        let stored = store
            .get_by_npub("npub")
            .await
            .expect("could not query contact")
            .expect("could not find contact by npub");

        assert_eq!(&stored.name, "name");
        assert_eq!(&stored.peer_id, &identity.peer_id);
        assert_eq!(&stored.nostr_npub.unwrap(), "npub");
        assert_eq!(&stored.nostr_relay.unwrap(), "wss://example.relay");
    }
}
