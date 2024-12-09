use crate::service::contact_service::IdentityPublicData;
use std::collections::HashMap;

use super::Result;
use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ContactStoreApi: Send + Sync {
    async fn get_map(&self) -> Result<HashMap<String, IdentityPublicData>>;
    async fn by_name(&self, name: &str) -> Result<Option<IdentityPublicData>>;
    async fn insert(&self, name: &str, data: IdentityPublicData) -> Result<()>;
    async fn delete(&self, name: &str) -> Result<()>;
    async fn update_name(&self, name: &str, new_name: &str) -> Result<()>;
    async fn update(&self, name: &str, data: IdentityPublicData) -> Result<()>;
    async fn get_by_npub(&self, npub: &str) -> Result<Option<IdentityPublicData>>;
}
