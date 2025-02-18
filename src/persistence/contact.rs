use crate::data::contact::Contact;
use std::collections::HashMap;

use super::Result;
use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ContactStoreApi: Send + Sync {
    async fn search(&self, search_term: &str) -> Result<Vec<Contact>>;
    async fn get_map(&self) -> Result<HashMap<String, Contact>>;
    async fn get(&self, node_id: &str) -> Result<Option<Contact>>;
    async fn insert(&self, node_id: &str, data: Contact) -> Result<()>;
    async fn delete(&self, node_id: &str) -> Result<()>;
    async fn update(&self, node_id: &str, data: Contact) -> Result<()>;
}
