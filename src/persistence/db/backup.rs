use std::path::Path;

use crate::persistence::backup::BackupStoreApi;

use super::Result;
use async_trait::async_trait;
use futures::StreamExt;
use surrealdb::{engine::any::Any, Surreal};

pub struct SurrealBackupStore {
    db: Surreal<Any>,
}

impl SurrealBackupStore {
    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl BackupStoreApi for SurrealBackupStore {
    /// returns the whole database as a byte vector backup ready for encryption
    async fn backup(&self) -> Result<Vec<u8>> {
        let mut stream = self.db.export(()).await?;
        let mut buffer = Vec::new();
        while let Some(Ok(chunk)) = stream.next().await {
            buffer.extend_from_slice(&chunk);
        }
        Ok(buffer)
    }

    async fn restore(&self, file_path: &Path) -> Result<()> {
        self.db.import(file_path).await?;
        Ok(())
    }

    async fn drop_db(&self, name: &str) -> Result<()> {
        let _ = self.db.query(format!("REMOVE DATABASE {}", name)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    use crate::persistence::db::get_memory_db;

    use super::*;

    #[tokio::test]
    async fn test_backup() {
        let store = get_store("backup").await;
        let result = store.backup().await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_restore() {
        let tmp = env::temp_dir()
            .join("test.surql")
            .to_str()
            .unwrap()
            .to_string();
        let store = get_store("restore").await;
        let backup = store.backup().await.expect("could not backup");
        let mut file = File::create(&tmp).await.expect("could not create file");
        file.write_all(&backup)
            .await
            .expect("could not write to file");

        let result = store.restore(Path::new(&tmp)).await;
        assert!(result.is_ok());
    }

    async fn get_store(db_name: &str) -> SurrealBackupStore {
        let db = get_memory_db("test", db_name)
            .await
            .expect("could not create db");
        SurrealBackupStore::new(db)
    }
}
