use super::{file_storage_path, Result};
use crate::{
    blockchain::Chain,
    service::bill_service::{BillKeys, BitcreditBill},
    util::file::is_not_hidden_or_directory_async,
};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::{
    fs::{read, read_dir, write},
    task,
};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BillStoreApi: Send + Sync {
    /// Checks if the given bill exists
    async fn bill_exists(&self, bill_name: &str) -> bool;

    /// Reads the given bill as bytes
    async fn get_bill_as_bytes(&self, bill_name: &str) -> Result<Vec<u8>>;

    /// Reads the keys for the given bill as bytes
    async fn get_bill_keys_as_bytes(&self, bill_name: &str) -> Result<Vec<u8>>;

    /// Gets all bill names
    async fn get_bill_names(&self) -> Result<Vec<String>>;

    /// Gets all bills
    async fn get_bills(&self) -> Result<Vec<BitcreditBill>>;

    /// Reads the blockchain of the given bill from disk
    async fn read_bill_chain_from_file(&self, bill_name: &str) -> Result<Chain>;

    /// Writes bill keys to file
    async fn write_bill_keys_to_file(
        &self,
        bill_name: String,
        private_key: String,
        public_key: String,
    ) -> Result<()>;

    /// Writes the given pretty printed chain as JSON to a file
    async fn write_blockchain_to_file(
        &self,
        bill_name: &str,
        pretty_printed_chain_as_json: String,
    ) -> Result<()>;

    /// Reads bill keys from file
    async fn read_bill_keys_from_file(&self, bill_name: &str) -> Result<BillKeys>;
}

#[derive(Clone)]
pub struct FileBasedBillStore {
    folder: String,
    keys_folder: String,
}

impl FileBasedBillStore {
    pub async fn new(data_dir: &str, path: &str, keys_path: &str) -> Result<Self> {
        let folder = file_storage_path(data_dir, path).await?;
        let keys_folder = file_storage_path(data_dir, keys_path).await?;
        Ok(Self {
            folder,
            keys_folder,
        })
    }

    pub fn get_path_for_bills(&self) -> PathBuf {
        PathBuf::from(self.folder.as_str())
    }

    pub fn get_path_for_bill(&self, bill_name: &str) -> PathBuf {
        let mut path = PathBuf::from(self.folder.as_str()).join(bill_name);
        path.set_extension("json");
        path
    }

    pub fn get_path_for_bill_keys(&self, key_name: &str) -> PathBuf {
        let mut path = PathBuf::from(self.keys_folder.as_str()).join(key_name);
        path.set_extension("json");
        path
    }
}

pub fn bill_chain_from_bytes(bytes: &[u8]) -> Result<Chain> {
    let chain: Chain = serde_json::from_slice(bytes).map_err(super::Error::Json)?;
    Ok(chain)
}

pub fn bill_keys_from_bytes(bytes: &[u8]) -> Result<BillKeys> {
    let keys: BillKeys = serde_json::from_slice(bytes).map_err(super::Error::Json)?;
    Ok(keys)
}

#[async_trait]
impl BillStoreApi for FileBasedBillStore {
    async fn bill_exists(&self, bill_name: &str) -> bool {
        let bill_path = self.get_path_for_bill(bill_name).clone();
        task::spawn_blocking(move || bill_path.exists())
            .await
            .unwrap_or(false)
    }

    async fn get_bill_as_bytes(&self, bill_name: &str) -> Result<Vec<u8>> {
        let bill_path = self.get_path_for_bill(bill_name);
        let bytes = read(bill_path).await?;
        Ok(bytes)
    }

    async fn get_bill_keys_as_bytes(&self, bill_name: &str) -> Result<Vec<u8>> {
        let keys_path = self.get_path_for_bill_keys(bill_name);
        let bytes = read(keys_path).await?;
        Ok(bytes)
    }

    async fn get_bill_names(&self) -> Result<Vec<String>> {
        let mut res = vec![];
        let mut dir = read_dir(self.get_path_for_bills()).await?;
        while let Some(entry) = dir.next_entry().await? {
            if is_not_hidden_or_directory_async(&entry).await {
                if let Some(bill_name) = entry.path().file_stem() {
                    if let Some(bill_name_str) = bill_name.to_str() {
                        res.push(bill_name_str.to_owned());
                    }
                }
            }
        }
        Ok(res)
    }

    async fn get_bills(&self) -> Result<Vec<BitcreditBill>> {
        let mut bills = Vec::new();
        let mut dir = read_dir(self.get_path_for_bills()).await?;
        while let Some(entry) = dir.next_entry().await? {
            if is_not_hidden_or_directory_async(&entry).await {
                let bill_path = entry.path();
                if let Some(file_name) = bill_path.file_stem() {
                    if let Some(file_name_str) = file_name.to_str() {
                        let chain = self.read_bill_chain_from_file(file_name_str).await?;
                        let bill_keys = self.read_bill_keys_from_file(file_name_str).await?;
                        let bill = chain.get_last_version_bill(&bill_keys)?;
                        bills.push(bill);
                    }
                }
            }
        }
        Ok(bills)
    }

    async fn read_bill_chain_from_file(&self, bill_name: &str) -> Result<Chain> {
        let path = self.get_path_for_bill(bill_name);
        let bytes = read(path).await?;
        serde_json::from_slice(&bytes).map_err(super::Error::Json)
    }

    async fn write_bill_keys_to_file(
        &self,
        bill_name: String,
        private_key: String,
        public_key: String,
    ) -> Result<()> {
        let keys: BillKeys = BillKeys {
            private_key_pem: private_key,
            public_key_pem: public_key,
        };

        let output_path = self.get_path_for_bill_keys(&bill_name);
        write(
            output_path,
            serde_json::to_string_pretty(&keys).map_err(super::Error::Json)?,
        )
        .await?;
        Ok(())
    }

    async fn write_blockchain_to_file(
        &self,
        bill_name: &str,
        pretty_printed_chain_as_json: String,
    ) -> Result<()> {
        let path = self.get_path_for_bill(bill_name);
        write(path, pretty_printed_chain_as_json).await?;
        Ok(())
    }

    async fn read_bill_keys_from_file(&self, bill_name: &str) -> Result<BillKeys> {
        let input_path = self.get_path_for_bill_keys(bill_name);
        let bytes = read(&input_path).await?;
        serde_json::from_slice(&bytes).map_err(super::Error::Json)
    }
}
