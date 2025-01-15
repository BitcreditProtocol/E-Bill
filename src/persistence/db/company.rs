use super::Result;
use crate::{
    persistence::{company::CompanyStoreApi, Error},
    service::company_service::{Company, CompanyKeys},
    web::data::File,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::{engine::any::Any, sql::Thing, Surreal};

#[derive(Clone)]
pub struct SurrealCompanyStore {
    db: Surreal<Any>,
}

impl SurrealCompanyStore {
    const DATA_TABLE: &'static str = "company";
    const KEYS_TABLE: &'static str = "company_keys";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CompanyStoreApi for SurrealCompanyStore {
    async fn exists(&self, id: &str) -> bool {
        self.get(id).await.map(|_| true).unwrap_or(false)
            && self.get_key_pair(id).await.map(|_| true).unwrap_or(false)
    }

    async fn get(&self, id: &str) -> Result<Company> {
        let result: Option<CompanyDb> = self.db.select((Self::DATA_TABLE, id)).await?;
        match result {
            None => Err(Error::NoSuchEntity("company".to_string(), id.to_owned())),
            Some(c) => Ok(c.into()),
        }
    }

    async fn get_all(&self) -> Result<HashMap<String, (Company, CompanyKeys)>> {
        let companies: Vec<CompanyDb> = self.db.select(Self::DATA_TABLE).await?;
        let company_keys: Vec<CompanyKeysDb> = self.db.select(Self::KEYS_TABLE).await?;
        let companies_map: HashMap<String, CompanyDb> = companies
            .into_iter()
            .map(|company| (company.id.id.to_raw(), company))
            .collect();
        let companies_keys_map: HashMap<String, CompanyKeysDb> = company_keys
            .into_iter()
            .filter_map(|keys| keys.id.clone().map(|id| (id.id.to_raw(), keys)))
            .collect();
        let combined: HashMap<String, (Company, CompanyKeys)> = companies_map
            .into_iter()
            .filter_map(|(id, company)| {
                companies_keys_map
                    .get(&id)
                    .map(|keys| (id, (company.into(), keys.clone().into())))
            })
            .collect();
        Ok(combined)
    }

    async fn insert(&self, data: &Company) -> Result<()> {
        let id = data.id.to_owned();
        let entity: CompanyDb = data.into();
        let _: Option<CompanyDb> = self
            .db
            .create((Self::DATA_TABLE, id))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn update(&self, id: &str, data: &Company) -> Result<()> {
        let entity: CompanyDb = data.into();
        let _: Option<CompanyDb> = self
            .db
            .update((Self::DATA_TABLE, id))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<()> {
        let _: Option<CompanyDb> = self.db.delete((Self::DATA_TABLE, id)).await?;
        let _: Option<CompanyKeysDb> = self.db.delete((Self::KEYS_TABLE, id)).await?;
        Ok(())
    }

    async fn save_key_pair(&self, id: &str, key_pair: &CompanyKeys) -> Result<()> {
        let entity: CompanyKeysDb = key_pair.into();
        let _: Option<CompanyKeysDb> = self
            .db
            .create((Self::KEYS_TABLE, id))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn get_key_pair(&self, id: &str) -> Result<CompanyKeys> {
        let result: Option<CompanyKeysDb> = self.db.select((Self::KEYS_TABLE, id)).await?;
        match result {
            None => Err(Error::NoSuchEntity("company".to_string(), id.to_owned())),
            Some(c) => Ok(c.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyDb {
    pub id: Thing,
    pub name: String,
    pub country_of_registration: String,
    pub city_of_registration: String,
    pub postal_address: String,
    pub email: String,
    pub registration_number: String,
    pub registration_date: String,
    pub proof_of_registration_file: Option<FileDb>,
    pub logo_file: Option<FileDb>,
    pub signatories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDb {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyKeysDb {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Thing>,
    pub public_key: String,
    pub private_key: String,
}

impl From<CompanyDb> for Company {
    fn from(value: CompanyDb) -> Self {
        Self {
            id: value.id.id.to_raw(),
            name: value.name,
            country_of_registration: value.country_of_registration,
            city_of_registration: value.city_of_registration,
            postal_address: value.postal_address,
            email: value.email,
            registration_number: value.registration_number,
            registration_date: value.registration_date,
            proof_of_registration_file: value.proof_of_registration_file.map(|f| f.into()),
            logo_file: value.logo_file.map(|f| f.into()),
            signatories: value.signatories,
        }
    }
}

impl From<&Company> for CompanyDb {
    fn from(value: &Company) -> Self {
        Self {
            id: (SurrealCompanyStore::DATA_TABLE.to_owned(), value.id.clone()).into(),
            name: value.name.clone(),
            country_of_registration: value.country_of_registration.clone(),
            city_of_registration: value.city_of_registration.clone(),
            postal_address: value.postal_address.clone(),
            email: value.email.clone(),
            registration_number: value.registration_number.clone(),
            registration_date: value.registration_date.clone(),
            proof_of_registration_file: value
                .proof_of_registration_file
                .clone()
                .map(|f| (&f).into()),
            logo_file: value.logo_file.clone().map(|f| (&f).into()),
            signatories: value.signatories.clone(),
        }
    }
}

impl From<CompanyKeysDb> for CompanyKeys {
    fn from(value: CompanyKeysDb) -> Self {
        Self {
            public_key: value.public_key,
            private_key: value.private_key,
        }
    }
}

impl From<&CompanyKeys> for CompanyKeysDb {
    fn from(value: &CompanyKeys) -> Self {
        Self {
            id: None,
            public_key: value.public_key.clone(),
            private_key: value.private_key.clone(),
        }
    }
}

impl From<FileDb> for File {
    fn from(value: FileDb) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        persistence::db::get_memory_db,
        tests::test::{TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP},
        util::BcrKeys,
    };

    async fn get_store() -> SurrealCompanyStore {
        let mem_db = get_memory_db("test", "company")
            .await
            .expect("could not create memory db");
        SurrealCompanyStore::new(mem_db)
    }

    fn get_baseline_company() -> Company {
        Company {
            id: TEST_PUB_KEY_SECP.to_owned(),
            name: "some_name".to_string(),
            country_of_registration: "AT".to_string(),
            city_of_registration: "Vienna".to_string(),
            postal_address: "some address".to_string(),
            email: "company@example.com".to_string(),
            registration_number: "some_number".to_string(),
            registration_date: "2012-01-01".to_string(),
            proof_of_registration_file: None,
            logo_file: None,
            signatories: vec!["1234".to_string()],
        }
    }

    #[tokio::test]
    async fn test_exists() {
        let store = get_store().await;
        assert!(!store.exists(TEST_PUB_KEY_SECP).await);
        store.insert(&get_baseline_company()).await.unwrap();
        assert!(!store.exists(TEST_PUB_KEY_SECP).await);
        store
            .save_key_pair(
                TEST_PUB_KEY_SECP,
                &CompanyKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_string(),
                    public_key: TEST_PUB_KEY_SECP.to_string(),
                },
            )
            .await
            .unwrap();
        assert!(store.exists(TEST_PUB_KEY_SECP).await)
    }

    #[tokio::test]
    async fn test_get() {
        let store = get_store().await;
        store.insert(&get_baseline_company()).await.unwrap();
        let company = store.get(TEST_PUB_KEY_SECP).await.unwrap();
        assert_eq!(company.name, "some_name".to_owned());
    }

    #[tokio::test]
    async fn test_remove() {
        let store = get_store().await;
        store.insert(&get_baseline_company()).await.unwrap();
        store
            .save_key_pair(
                TEST_PUB_KEY_SECP,
                &CompanyKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_string(),
                    public_key: TEST_PUB_KEY_SECP.to_string(),
                },
            )
            .await
            .unwrap();
        assert!(store.exists(TEST_PUB_KEY_SECP).await);
        store.remove(TEST_PUB_KEY_SECP).await.unwrap();
        assert!(!store.exists(TEST_PUB_KEY_SECP).await);
    }

    #[tokio::test]
    async fn test_get_key_pair() {
        let store = get_store().await;
        store
            .save_key_pair(
                TEST_PUB_KEY_SECP,
                &CompanyKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_string(),
                    public_key: TEST_PUB_KEY_SECP.to_string(),
                },
            )
            .await
            .unwrap();
        let company_keys = store.get_key_pair(TEST_PUB_KEY_SECP).await.unwrap();
        assert_eq!(company_keys.public_key, TEST_PUB_KEY_SECP.to_string());
    }

    #[tokio::test]
    async fn test_update() {
        let store = get_store().await;
        store.insert(&get_baseline_company()).await.unwrap();
        let mut company = store.get(TEST_PUB_KEY_SECP).await.unwrap();
        company.name = "some other company".to_string();
        store.update(TEST_PUB_KEY_SECP, &company).await.unwrap();
        let changed_company = store.get(TEST_PUB_KEY_SECP).await.unwrap();
        assert_eq!(changed_company.name, "some other company".to_owned());
    }

    #[tokio::test]
    async fn test_get_all() {
        let store = get_store().await;
        let mut company = get_baseline_company();
        company.name = "first".to_string();
        store.insert(&company).await.unwrap();
        store
            .save_key_pair(
                TEST_PUB_KEY_SECP,
                &CompanyKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                },
            )
            .await
            .unwrap();
        let mut company2 = get_baseline_company();
        company2.id = BcrKeys::new().get_public_key();
        store.insert(&company2).await.unwrap();
        store
            .save_key_pair(
                &company2.id,
                &CompanyKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_string(),
                    public_key: TEST_PUB_KEY_SECP.to_string(),
                },
            )
            .await
            .unwrap();
        let companies = store.get_all().await.unwrap();
        assert_eq!(companies.len(), 2);
        assert_eq!(
            companies.get(TEST_PUB_KEY_SECP).as_ref().unwrap().0.name,
            "first".to_owned()
        );
        assert_eq!(
            companies
                .get(TEST_PUB_KEY_SECP)
                .as_ref()
                .unwrap()
                .1
                .public_key,
            TEST_PUB_KEY_SECP.to_owned()
        );
        assert_eq!(
            companies.get(&company2.id).as_ref().unwrap().0.name,
            "some_name".to_owned()
        );
        assert_eq!(
            companies.get(&company2.id).as_ref().unwrap().1.public_key,
            TEST_PUB_KEY_SECP.to_owned()
        );
    }
}
