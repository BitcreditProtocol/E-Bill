use crate::service::contact_service::IdentityPublicData;
use borsh::{self, BorshDeserialize, BorshSerialize};
use std::{collections::HashMap, fs, path::Path};

use super::{file_storage_path, Error, Result};
use async_trait;

#[async_trait]
pub trait ContactStoreApi: Send + Sync {
    async fn list(&self) -> Result<Vec<IdentityPublicData>>;
    async fn get_map(&self) -> Result<HashMap<String, IdentityPublicData>>;
    async fn by_name(&self, name: &str) -> Result<Option<IdentityPublicData>>;
    async fn insert(&self, name: &str, data: IdentityPublicData) -> Result<()>;
    async fn delete(&self, name: &str) -> Result<()>;
    async fn update_name(&self, name: &str, new_name: &str) -> Result<()>;
    async fn update(&self, name: &str, data: IdentityPublicData) -> Result<()>;
}

#[derive(Clone)]
pub struct FileBasedContactStore {
    file: String,
}

/// Just some shortcuts for read and write here
impl FileBasedContactStore {
    pub fn new(data_dir: &str, path: &str, file_name: &str) -> Result<Self> {
        let directory = file_storage_path(data_dir, path)?;
        Ok(Self {
            file: format!("{}/{}", directory, file_name),
        })
    }

    fn write(&self, contacts: HashMap<String, IdentityPublicData>) -> Result<()> {
        write_contacts_map(&self.file, contacts)
    }

    fn read(&self) -> Result<HashMap<String, IdentityPublicData>> {
        read_contacts_map(&self.file)
    }
}

#[async_trait]
impl ContactStoreApi for FileBasedContactStore {
    async fn list(&self) -> Result<Vec<IdentityPublicData>> {
        let list = self.read()?.values().cloned().collect();
        Ok(list)
    }

    async fn get_map(&self) -> Result<HashMap<String, IdentityPublicData>> {
        Ok(self.read()?)
    }

    async fn by_name(&self, name: &str) -> Result<Option<IdentityPublicData>> {
        let contact = self.read()?.get(name).map(|e| e.to_owned());
        Ok(contact)
    }

    async fn insert(&self, name: &str, data: IdentityPublicData) -> Result<()> {
        let mut current = self.read()?;
        current.insert(name.to_owned(), data);
        self.write(current)?;
        Ok(())
    }

    async fn update(&self, name: &str, data: IdentityPublicData) -> Result<()> {
        self.insert(name, data).await?;
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let mut current = self.read()?;
        current.remove(name);
        self.write(current)?;
        Ok(())
    }

    async fn update_name(&self, name: &str, new_name: &str) -> Result<()> {
        let mut all = self.read()?;
        match all.get(name) {
            Some(identity) => {
                all.insert(new_name.to_owned(), identity.to_owned());
                all.remove(name);
                self.write(all)?;
                Ok(())
            }
            None => Err(Error::NoSuchEntity("contact".to_string(), name.to_string())),
        }
    }
}

fn write_contacts_map(file: &str, map: HashMap<String, IdentityPublicData>) -> Result<()> {
    let contacts_byte = map.try_to_vec()?;
    fs::write(file, contacts_byte)?;
    Ok(())
}

fn read_contacts_map(file: &str) -> Result<HashMap<String, IdentityPublicData>> {
    if !Path::new(file).exists() {
        create_contacts_map(file)?;
    }
    let data: Vec<u8> = fs::read(file)?;
    let contacts: HashMap<String, IdentityPublicData> = HashMap::try_from_slice(&data)?;
    Ok(contacts)
}

fn create_contacts_map(file: &str) -> Result<()> {
    let contacts: HashMap<String, IdentityPublicData> = HashMap::new();
    write_contacts_map(file, contacts)?;
    Ok(())
}
