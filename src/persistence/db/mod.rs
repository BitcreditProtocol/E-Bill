use super::Result;
use log::error;
use surrealdb::{
    engine::any::{connect, Any},
    Surreal,
};

pub mod company;
pub mod contact;
pub mod identity;
pub mod nostr_event_offset;

/// Configuration for the SurrealDB connection string, namespace and
/// database name
#[derive(Clone, Debug)]
pub struct SurrealDbConfig {
    connection_string: String,
    namespace: String,
    database: String,
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
