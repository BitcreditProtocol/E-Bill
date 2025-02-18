use super::Result;
use crate::data::{File, OptionalPostalAddress, PostalAddress};
use log::error;
use serde::{Deserialize, Serialize};
use surrealdb::{
    engine::any::{connect, Any},
    Surreal,
};

pub mod backup;
pub mod bill;
pub mod bill_chain;
pub mod company;
pub mod company_chain;
pub mod contact;
pub mod identity;
pub mod identity_chain;
pub mod nostr_event_offset;
pub mod notification;

/// Configuration for the SurrealDB connection string, namespace and
/// database name
#[derive(Clone, Debug)]
pub struct SurrealDbConfig {
    pub connection_string: String,
    pub namespace: String,
    pub database: String,
}

impl SurrealDbConfig {
    pub fn new(connection_string: &str) -> Self {
        Self {
            connection_string: connection_string.to_owned(),
            ..Default::default()
        }
    }
}

impl Default for SurrealDbConfig {
    fn default() -> Self {
        Self {
            connection_string: "rocksdb://data/surrealdb".to_owned(),
            namespace: "default".to_owned(),
            database: "ebills".to_owned(),
        }
    }
}

/// Connect to the SurrealDB instance using the provided configuration.
pub async fn get_surreal_db(config: &SurrealDbConfig) -> Result<Surreal<Any>> {
    let db = connect(&config.connection_string).await.map_err(|e| {
        error!("Error connecting to SurrealDB with config: {config:?}. Error: {e}");
        e
    })?;
    db.use_ns(&config.namespace)
        .use_db(&config.database)
        .await?;
    Ok(db)
}

/// This is handy for testing db queries. I have added the mem:// storage backend
/// feature as a dev dependency in Cargo.toml. The mem storage backend is still a
/// drag in terms of compile time but I think it is worth it for testing.
#[cfg(test)]
pub async fn get_memory_db(namespace: &str, database: &str) -> Result<Surreal<Any>> {
    let db = connect("mem://").await?;
    db.use_ns(namespace).use_db(database).await?;
    Ok(db)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDb {
    pub name: String,
    pub hash: String,
}

impl From<FileDb> for File {
    fn from(value: FileDb) -> Self {
        Self {
            name: value.name,
            hash: value.hash,
        }
    }
}

impl From<File> for FileDb {
    fn from(value: File) -> Self {
        Self {
            name: value.name,
            hash: value.hash,
        }
    }
}

impl From<&File> for FileDb {
    fn from(value: &File) -> Self {
        Self {
            name: value.name.clone(),
            hash: value.hash.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalPostalAddressDb {
    pub country: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub address: Option<String>,
}

impl From<OptionalPostalAddressDb> for OptionalPostalAddress {
    fn from(value: OptionalPostalAddressDb) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl From<OptionalPostalAddress> for OptionalPostalAddressDb {
    fn from(value: OptionalPostalAddress) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl From<&OptionalPostalAddress> for OptionalPostalAddressDb {
    fn from(value: &OptionalPostalAddress) -> Self {
        Self {
            country: value.country.clone(),
            city: value.city.clone(),
            zip: value.zip.clone(),
            address: value.address.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostalAddressDb {
    pub country: String,
    pub city: String,
    pub zip: Option<String>,
    pub address: String,
}

impl From<PostalAddressDb> for PostalAddress {
    fn from(value: PostalAddressDb) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl From<PostalAddress> for PostalAddressDb {
    fn from(value: PostalAddress) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl From<&PostalAddress> for PostalAddressDb {
    fn from(value: &PostalAddress) -> Self {
        Self {
            country: value.country.clone(),
            city: value.city.clone(),
            zip: value.zip.clone(),
            address: value.address.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_surreal_db() {
        let config = SurrealDbConfig::new("mem://");
        let _ = get_surreal_db(&config).await.expect("could not create db");
    }

    #[tokio::test]
    async fn test_get_memory_db() {
        let _ = get_memory_db("test", "test")
            .await
            .expect("could not create db");
    }
}
