use crate::service::company_service::{Company, CompanyKeys};
use std::collections::HashMap;

use super::Result;
use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CompanyStoreApi: Send + Sync {
    /// Checks if the given company exists
    async fn exists(&self, id: &str) -> bool;

    /// Fetches the given company
    async fn get(&self, id: &str) -> Result<Company>;

    /// Returns all companies
    async fn get_all(&self) -> Result<HashMap<String, (Company, CompanyKeys)>>;

    /// Inserts the company with the given id
    async fn insert(&self, id: &str, data: &Company) -> Result<()>;

    /// Updates the company with the given id
    async fn update(&self, id: &str, data: &Company) -> Result<()>;

    /// Removes the company with the given id (e.g. if we're removed as signatory)
    async fn remove(&self, id: &str) -> Result<()>;

    /// Saves the key pair for the given company id
    async fn save_key_pair(&self, id: &str, key_pair: &CompanyKeys) -> Result<()>;

    /// Gets the key pair for the given company id
    async fn get_key_pair(&self, id: &str) -> Result<CompanyKeys>;
}

pub fn company_from_bytes(bytes: &[u8]) -> Result<Company> {
    let company: Company = serde_json::from_slice(bytes)?;
    Ok(company)
}

pub fn company_to_bytes(company: &Company) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&company)?;
    Ok(bytes)
}

pub fn company_keys_from_bytes(bytes: &[u8]) -> Result<CompanyKeys> {
    let company_keys: CompanyKeys = serde_json::from_slice(bytes)?;
    Ok(company_keys)
}

pub fn company_keys_to_bytes(company_keys: &CompanyKeys) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&company_keys)?;
    Ok(bytes)
}
