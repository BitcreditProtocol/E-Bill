use std::path::Path;

use super::Result;
use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

/// Backup and restore the database from/to bytes.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait BackupStoreApi: Send + Sync {
    /// creates a backup of the currently active database as a byte vector
    /// ready for encryption
    async fn backup(&self) -> Result<Vec<u8>>;

    /// Restores the default database from given surqul file
    async fn restore(&self, file_path: &Path) -> Result<()>;

    /// drops the database with the given name
    async fn drop_db(&self, name: &str) -> Result<()>;
}
