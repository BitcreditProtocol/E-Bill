use super::Result;
use crate::persistence::company::CompanyStoreApi;
use crate::CONFIG;
use crate::{
    error,
    persistence::{file_upload::FileUploadStoreApi, identity::IdentityStoreApi, ContactStoreApi},
    util,
    web::data::File,
};
use async_trait::async_trait;
use borsh_derive::{self, BorshDeserialize, BorshSerialize};
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[async_trait]
pub trait CompanyServiceApi: Send + Sync {
    /// Get a list of companies
    async fn get_list_of_companies(&self) -> Result<Vec<CompanyToReturn>>;

    /// Get a company by id
    async fn get_company_by_id(&self, id: &str) -> Result<CompanyToReturn>;

    /// Create a new company
    async fn create_company(
        &self,
        legal_name: String,
        country_of_registration: String,
        city_of_registration: String,
        postal_address: String,
        legal_email: String,
        registration_number: String,
        registration_date: String,
        proof_of_registration_file_upload_id: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Result<CompanyToReturn>;

    /// Changes the given company fields for the given company, if they are set
    async fn edit_company(
        &self,
        id: &str,
        legal_name: Option<String>,
        legal_email: Option<String>,
        postal_address: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Result<()>;

    /// Adds another signatory to the given company
    async fn add_signatory(&self, id: &str, signatory_node_id: String) -> Result<()>;

    /// Removes a signatory from the given company
    async fn remove_signatory(&self, id: &str, signatory_node_id: String) -> Result<()>;

    /// Encrypts and saves the given uploaded file, returning the file name, as well as the hash of
    /// the unencrypted file
    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        id: &str,
        public_key: &str,
    ) -> Result<File>;

    /// opens and decrypts the attached file from the given company
    async fn open_and_decrypt_file(
        &self,
        id: &str,
        file_name: &str,
        private_key: &str,
    ) -> Result<Vec<u8>>;
}

/// The company service is responsible for managing the companies
#[derive(Clone)]
pub struct CompanyService {
    store: Arc<dyn CompanyStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    contact_store: Arc<dyn ContactStoreApi>,
}

impl CompanyService {
    pub fn new(
        store: Arc<dyn CompanyStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
        contact_store: Arc<dyn ContactStoreApi>,
    ) -> Self {
        Self {
            store,
            file_upload_store,
            identity_store,
            contact_store,
        }
    }

    async fn process_upload_file(
        &self,
        upload_id: &Option<String>,
        id: &str,
        public_key: &str,
    ) -> Result<Option<File>> {
        if let Some(upload_id) = upload_id {
            let files = self
                .file_upload_store
                .read_temp_upload_files(upload_id)
                .await?;
            if !files.is_empty() {
                let (file_name, file_bytes) = &files[0];
                let file = self
                    .encrypt_and_save_uploaded_file(file_name, file_bytes, id, public_key)
                    .await?;
                return Ok(Some(file));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl CompanyServiceApi for CompanyService {
    async fn get_list_of_companies(&self) -> Result<Vec<CompanyToReturn>> {
        let results = self.store.get_all().await?;
        let companies: Vec<CompanyToReturn> = results
            .into_iter()
            .map(|(id, (company, keys))| CompanyToReturn::from(id, company, keys))
            .collect();
        Ok(companies)
    }

    async fn get_company_by_id(&self, id: &str) -> Result<CompanyToReturn> {
        if !self.store.exists(id).await {
            return Err(super::Error::Validation(format!(
                "No company with id: {id} found",
            )));
        }

        let company = self.store.get(id).await?;
        let keys = self.store.get_key_pair(id).await?;
        Ok(CompanyToReturn::from(id.to_owned(), company, keys))
    }

    async fn create_company(
        &self,
        legal_name: String,
        country_of_registration: String,
        city_of_registration: String,
        postal_address: String,
        legal_email: String,
        registration_number: String,
        registration_date: String,
        proof_of_registration_file_upload_id: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Result<CompanyToReturn> {
        let (private_key, public_key) = util::create_bitcoin_keypair(CONFIG.bitcoin_network());
        let id = util::sha256_hash(&public_key.to_bytes());

        let company_keys = CompanyKeys {
            private_key: private_key.to_string(),
            public_key: public_key.to_string(),
        };

        let identity = self.identity_store.get().await?;
        let peer_id = self.identity_store.get_peer_id().await?;

        let proof_of_registration_file = self
            .process_upload_file(
                &proof_of_registration_file_upload_id,
                &id,
                &identity.public_key_pem,
            )
            .await?;

        let logo_file = self
            .process_upload_file(&logo_file_upload_id, &id, &identity.public_key_pem)
            .await?;

        self.store.save_key_pair(&id, &company_keys).await?;
        let company = Company {
            legal_name,
            country_of_registration,
            city_of_registration,
            postal_address,
            legal_email,
            registration_number,
            registration_date,
            proof_of_registration_file,
            logo_file,
            signatories: vec![peer_id.to_string()], // add caller as signatory
        };
        self.store.insert(&id, &company).await?;

        // clean up temporary file uploads, if there are any, logging any errors
        for upload_id in [proof_of_registration_file_upload_id, logo_file_upload_id]
            .iter()
            .flatten()
        {
            if let Err(e) = self
                .file_upload_store
                .remove_temp_upload_folder(upload_id)
                .await
            {
                error!("Error while cleaning up temporary file uploads for {upload_id}: {e}");
            }
        }

        Ok(CompanyToReturn::from(id, company, company_keys))
    }

    async fn edit_company(
        &self,
        id: &str,
        legal_name: Option<String>,
        legal_email: Option<String>,
        postal_address: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Result<()> {
        if !self.store.exists(id).await {
            return Err(super::Error::Validation(format!(
                "No company with id: {id} found",
            )));
        }
        let peer_id = self.identity_store.get_peer_id().await?;
        let mut company = self.store.get(id).await?;

        if !company.signatories.contains(&peer_id.to_string()) {
            return Err(super::Error::Validation(String::from(
                "Caller must be signatory for company",
            )));
        }

        if let Some(legal_name_to_set) = legal_name {
            company.legal_name = legal_name_to_set;
        }
        if let Some(legal_email_to_set) = legal_email {
            company.legal_email = legal_email_to_set;
        }
        if let Some(postal_address_to_set) = postal_address {
            company.postal_address = postal_address_to_set;
        }
        let identity = self.identity_store.get().await?;
        let logo_file = self
            .process_upload_file(&logo_file_upload_id, id, &identity.public_key_pem)
            .await?;
        company.logo_file = logo_file;

        self.store.update(id, &company).await?;

        if let Some(upload_id) = logo_file_upload_id {
            if let Err(e) = self
                .file_upload_store
                .remove_temp_upload_folder(&upload_id)
                .await
            {
                error!("Error while cleaning up temporary file uploads for {upload_id}: {e}");
            }
        }

        Ok(())
    }

    async fn add_signatory(&self, id: &str, signatory_node_id: String) -> Result<()> {
        if !self.store.exists(id).await {
            return Err(super::Error::Validation(format!(
                "No company with id: {id} found.",
            )));
        }
        let contacts = self.contact_store.get_map().await?;
        let is_in_contacts = contacts
            .iter()
            .any(|(_name, identity)| identity.peer_id == signatory_node_id);
        if !is_in_contacts {
            return Err(super::Error::Validation(format!(
                "Node Id {signatory_node_id} is not in the contacts.",
            )));
        }

        let mut company = self.store.get(id).await?;
        if company.signatories.contains(&signatory_node_id) {
            return Err(super::Error::Validation(format!(
                "Node Id {signatory_node_id} is already a signatory.",
            )));
        }
        company.signatories.push(signatory_node_id);
        self.store.update(id, &company).await?;

        Ok(())
    }

    async fn remove_signatory(&self, id: &str, signatory_node_id: String) -> Result<()> {
        if !self.store.exists(id).await {
            return Err(super::Error::Validation(format!(
                "No company with id: {id} found.",
            )));
        }

        let mut company = self.store.get(id).await?;
        if company.signatories.len() == 1 {
            return Err(super::Error::Validation(String::from(
                "Can't remove last signatory.",
            )));
        }
        if !company.signatories.contains(&signatory_node_id) {
            return Err(super::Error::Validation(format!(
                "Node id {signatory_node_id} is not a signatory.",
            )));
        }

        let peer_id = self.identity_store.get_peer_id().await?;

        company.signatories.retain(|i| i != &signatory_node_id);
        self.store.update(id, &company).await?;

        if peer_id.to_string() == signatory_node_id {
            info!("Removing self from company {id}");
            self.store.remove(id).await?;
        }

        Ok(())
    }

    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        id: &str,
        public_key: &str,
    ) -> Result<File> {
        let file_hash = util::sha256_hash(file_bytes);
        let encrypted = util::rsa::encrypt_bytes_with_public_key(file_bytes, public_key);
        self.store
            .save_attached_file(&encrypted, id, file_name)
            .await?;
        info!("Saved company file {file_name} with hash {file_hash} for company {id}");
        Ok(File {
            name: file_name.to_owned(),
            hash: file_hash,
        })
    }

    async fn open_and_decrypt_file(
        &self,
        id: &str,
        file_name: &str,
        private_key: &str,
    ) -> Result<Vec<u8>> {
        let read_file = self.store.open_attached_file(id, file_name).await?;
        let decrypted = util::rsa::decrypt_bytes_with_private_key(&read_file, private_key);
        Ok(decrypted)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct CompanyToReturn {
    pub id: String,
    pub legal_name: String,
    pub country_of_registration: String,
    pub city_of_registration: String,
    pub postal_address: String,
    pub legal_email: String,
    pub registration_number: String,
    pub registration_date: String,
    pub proof_of_registration_file: Option<File>,
    pub logo_file: Option<File>,
    pub signatories: Vec<String>,
    pub public_key: String,
}

impl CompanyToReturn {
    fn from(id: String, company: Company, company_keys: CompanyKeys) -> CompanyToReturn {
        CompanyToReturn {
            id,
            legal_name: company.legal_name,
            country_of_registration: company.country_of_registration,
            city_of_registration: company.city_of_registration,
            postal_address: company.postal_address,
            legal_email: company.legal_email,
            registration_number: company.registration_number,
            registration_date: company.registration_date,
            proof_of_registration_file: company.proof_of_registration_file,
            logo_file: company.logo_file,
            signatories: company.signatories,
            public_key: company_keys.public_key,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct Company {
    pub legal_name: String,
    pub country_of_registration: String,
    pub city_of_registration: String,
    pub postal_address: String,
    pub legal_email: String,
    pub registration_number: String,
    pub registration_date: String,
    pub proof_of_registration_file: Option<File>,
    pub logo_file: Option<File>,
    pub signatories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct CompanyPublicData {
    pub id: String,
    pub legal_name: String,
    pub postal_address: String,
    pub legal_email: String,
    pub public_key: String,
}

impl CompanyPublicData {
    pub fn from_all(id: String, company: Company, company_keys: CompanyKeys) -> CompanyPublicData {
        CompanyPublicData {
            id,
            legal_name: company.legal_name,
            postal_address: company.postal_address,
            legal_email: company.legal_email,
            public_key: company_keys.public_key,
        }
    }
}

impl From<CompanyToReturn> for CompanyPublicData {
    fn from(company: CompanyToReturn) -> Self {
        Self {
            id: company.id,
            legal_name: company.legal_name,
            postal_address: company.postal_address,
            legal_email: company.legal_email,
            public_key: company.public_key,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompanyKeys {
    pub private_key: String,
    pub public_key: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        persistence::{
            self, company::MockCompanyStoreApi, contact::MockContactStoreApi,
            file_upload::MockFileUploadStoreApi, identity::MockIdentityStoreApi,
        },
        service::{contact_service::IdentityPublicData, identity_service::Identity},
        tests::test::{TEST_PRIVATE_KEY, TEST_PUB_KEY},
    };
    use libp2p::PeerId;
    use mockall::predicate::{always, eq};
    use std::collections::HashMap;

    fn get_service(
        mock_storage: MockCompanyStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        mock_contacts_storage: MockContactStoreApi,
    ) -> CompanyService {
        CompanyService::new(
            Arc::new(mock_storage),
            Arc::new(mock_file_upload_storage),
            Arc::new(mock_identity_storage),
            Arc::new(mock_contacts_storage),
        )
    }

    fn get_storages() -> (
        MockCompanyStoreApi,
        MockFileUploadStoreApi,
        MockIdentityStoreApi,
        MockContactStoreApi,
    ) {
        (
            MockCompanyStoreApi::new(),
            MockFileUploadStoreApi::new(),
            MockIdentityStoreApi::new(),
            MockContactStoreApi::new(),
        )
    }

    fn get_baseline_company_data() -> (String, (Company, CompanyKeys)) {
        (
            "some_id".to_string(),
            (
                Company {
                    legal_name: "some_name".to_string(),
                    country_of_registration: "AT".to_string(),
                    city_of_registration: "Vienna".to_string(),
                    postal_address: "some address".to_string(),
                    legal_email: "company@example.com".to_string(),
                    registration_number: "some_number".to_string(),
                    registration_date: "2012-01-01".to_string(),
                    proof_of_registration_file: None,
                    logo_file: None,
                    signatories: vec![],
                },
                CompanyKeys {
                    private_key: TEST_PRIVATE_KEY.to_string(),
                    public_key: TEST_PUB_KEY.to_string(),
                },
            ),
        )
    }

    #[tokio::test]
    async fn get_list_of_companies_baseline() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_get_all().returning(|| {
            let mut map = HashMap::new();
            let company_data = get_baseline_company_data();
            map.insert(company_data.0, company_data.1);
            Ok(map)
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        let res = service.get_list_of_companies().await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(res.as_ref().unwrap()[0].id, "some_id".to_string());
        assert_eq!(
            res.as_ref().unwrap()[0].public_key,
            TEST_PUB_KEY.to_string()
        );
    }

    #[tokio::test]
    async fn get_list_of_companies_propagates_persistence_errors() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_get_all().returning(|| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service.get_list_of_companies().await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_company_by_id_baseline() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage
            .expect_get()
            .returning(|_| Ok(get_baseline_company_data().1 .0));
        storage
            .expect_get_key_pair()
            .returning(|_| Ok(get_baseline_company_data().1 .1));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        let res = service.get_company_by_id("some_id").await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some_id".to_string());
        assert_eq!(res.as_ref().unwrap().public_key, TEST_PUB_KEY.to_string());
    }

    #[tokio::test]
    async fn get_company_by_id_fails_if_company_doesnt_exist() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| false);
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service.get_company_by_id("some_id").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_company_by_id_propagates_persistence_errors() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service.get_company_by_id("some_id").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn create_company_baseline() {
        let (mut storage, mut file_upload_store, mut identity_store, contact_store) =
            get_storages();
        storage
            .expect_save_attached_file()
            .returning(|_, _, _| Ok(()));
        storage.expect_save_key_pair().returning(|_, _| Ok(()));
        storage.expect_insert().returning(|_, _| Ok(()));
        identity_store.expect_get().returning(|| {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(identity)
        });
        identity_store
            .expect_get_peer_id()
            .returning(|| Ok(PeerId::random()));
        file_upload_store
            .expect_read_temp_upload_files()
            .returning(|_| {
                Ok(vec![(
                    "some_file".to_string(),
                    "hello_world".as_bytes().to_vec(),
                )])
            });
        file_upload_store
            .expect_remove_temp_upload_folder()
            .returning(|_| Ok(()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        let res = service
            .create_company(
                "legal_name".to_string(),
                "AT".to_string(),
                "Vienna".to_string(),
                "some Address".to_string(),
                "company@example.com".to_string(),
                "some_number".to_string(),
                "2012-01-01".to_string(),
                Some("some_file_id".to_string()),
                Some("some_other_file_id".to_string()),
            )
            .await;
        assert!(res.is_ok());
        assert!(!res.as_ref().unwrap().id.is_empty());
        assert_eq!(res.as_ref().unwrap().legal_name, "legal_name".to_string());
        assert_eq!(
            res.as_ref()
                .unwrap()
                .proof_of_registration_file
                .as_ref()
                .unwrap()
                .name,
            "some_file".to_string()
        );
        assert_eq!(
            res.as_ref().unwrap().logo_file.as_ref().unwrap().name,
            "some_file".to_string()
        );
        assert!(!res.as_ref().unwrap().public_key.is_empty());
    }

    #[tokio::test]
    async fn create_company_propagates_persistence_errors() {
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        storage.expect_save_key_pair().returning(|_, _| Ok(()));
        storage.expect_insert().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        identity_store.expect_get().returning(|| {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(identity)
        });
        identity_store
            .expect_get_peer_id()
            .returning(|| Ok(PeerId::random()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .create_company(
                "legal_name".to_string(),
                "AT".to_string(),
                "Vienna".to_string(),
                "some Address".to_string(),
                "company@example.com".to_string(),
                "some_number".to_string(),
                "2012-01-01".to_string(),
                None,
                None,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn edit_company_baseline() {
        let peer_id = PeerId::random();
        let (mut storage, mut file_upload_store, mut identity_store, contact_store) =
            get_storages();
        storage.expect_get().returning(move |_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories = vec![peer_id.to_string()];
            Ok(data)
        });
        storage.expect_exists().returning(|_| true);
        storage.expect_update().returning(|_, _| Ok(()));
        storage
            .expect_save_attached_file()
            .returning(|_, _, _| Ok(()));
        identity_store
            .expect_get_peer_id()
            .returning(move || Ok(peer_id));
        identity_store.expect_get().returning(|| {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(identity)
        });
        file_upload_store
            .expect_read_temp_upload_files()
            .returning(|_| {
                Ok(vec![(
                    "some_file".to_string(),
                    "hello_world".as_bytes().to_vec(),
                )])
            });
        file_upload_store
            .expect_remove_temp_upload_folder()
            .returning(|_| Ok(()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .edit_company(
                "some_id",
                Some("legal_name".to_string()),
                Some("some Address".to_string()),
                Some("company@example.com".to_string()),
                Some("some_file_id".to_string()),
            )
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn edit_company_fails_if_company_doesnt_exist() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| false);
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .edit_company(
                "some_id",
                Some("legal_name".to_string()),
                Some("some Address".to_string()),
                Some("company@example.com".to_string()),
                None,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn edit_company_fails_if_caller_is_not_signatory() {
        let peer_id = PeerId::random();
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| false);
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories = vec!["some_other_dude".to_string()];
            Ok(data)
        });
        identity_store
            .expect_get_peer_id()
            .returning(move || Ok(peer_id));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .edit_company(
                "some_id",
                Some("legal_name".to_string()),
                Some("some Address".to_string()),
                Some("company@example.com".to_string()),
                None,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn edit_company_propagates_persistence_errors() {
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        let peer_id = PeerId::random();
        storage.expect_get().returning(move |_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories = vec![peer_id.to_string()];
            Ok(data)
        });
        storage.expect_exists().returning(|_| true);
        storage.expect_update().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        identity_store.expect_get().returning(|| {
            let mut identity = Identity::new_empty();
            identity.public_key_pem = TEST_PUB_KEY.to_string();
            Ok(identity)
        });
        identity_store
            .expect_get_peer_id()
            .returning(move || Ok(peer_id));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .edit_company(
                "some_id",
                Some("legal_name".to_string()),
                Some("some Address".to_string()),
                Some("company@example.com".to_string()),
                None,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn add_signatory_baseline() {
        let (mut storage, file_upload_store, identity_store, mut contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_update().returning(|_, _| Ok(()));
        contact_store.expect_get_map().returning(|| {
            let mut map = HashMap::new();
            let mut identity = IdentityPublicData::new_empty();
            identity.peer_id = "new_signatory_node_id".to_string();
            map.insert("my best friend".to_string(), identity);
            Ok(map)
        });
        storage
            .expect_get()
            .returning(|_| Ok(get_baseline_company_data().1 .0));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .add_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn add_signatory_fails_if_signatory_not_in_contacts() {
        let (mut storage, file_upload_store, identity_store, mut contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        contact_store
            .expect_get_map()
            .returning(|| Ok(HashMap::new()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .add_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn add_signatory_fails_if_company_doesnt_exist() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| false);
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .add_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn add_signatory_fails_if_signatory_is_already_signatory() {
        let (mut storage, file_upload_store, identity_store, mut contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        contact_store.expect_get_map().returning(|| {
            let mut map = HashMap::new();
            let mut identity = IdentityPublicData::new_empty();
            identity.peer_id = "new_signatory_node_id".to_string();
            map.insert("my best friend".to_string(), identity);
            Ok(map)
        });
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories.push("new_signatory_node_id".to_string());
            Ok(data)
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .add_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn add_signatory_propagates_persistence_errors() {
        let (mut storage, file_upload_store, identity_store, mut contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        contact_store.expect_get_map().returning(|| {
            let mut map = HashMap::new();
            let mut identity = IdentityPublicData::new_empty();
            identity.peer_id = "new_signatory_node_id".to_string();
            map.insert("my best friend".to_string(), identity);
            Ok(map)
        });
        storage.expect_update().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        storage
            .expect_get()
            .returning(|_| Ok(get_baseline_company_data().1 .0));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .add_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn remove_signatory_baseline() {
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories.push("new_signatory_node_id".to_string());
            data.signatories
                .push("some_other_dude_or_dudette".to_string());
            Ok(data)
        });
        identity_store
            .expect_get_peer_id()
            .returning(|| Ok(PeerId::random()));
        storage.expect_update().returning(|_, _| Ok(()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn remove_signatory_fails_if_company_doesnt_exist() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| false);
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn remove_signatory_removing_self_removes_company() {
        let peer_id = PeerId::random();
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(move |_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories.push("the founder".to_string());
            data.signatories.push(peer_id.to_string());
            Ok(data)
        });
        identity_store
            .expect_get_peer_id()
            .returning(move || Ok(peer_id));
        storage.expect_update().returning(|_, _| Ok(()));
        storage.expect_remove().returning(|_| Ok(()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", peer_id.to_string())
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn remove_signatory_fails_if_signatory_is_not_in_company() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories
                .push("some_other_dude_or_dudette".to_string());
            data.signatories.push("the_founder".to_string());
            Ok(data)
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn remove_signatory_fails_on_last_signatory() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories.push("the_founder".to_string());
            Ok(data)
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn remove_signatory_propagates_persistence_errors() {
        let (mut storage, file_upload_store, mut identity_store, contact_store) = get_storages();
        storage.expect_exists().returning(|_| true);
        storage.expect_get().returning(|_| {
            let mut data = get_baseline_company_data().1 .0;
            data.signatories.push("new_signatory_node_id".to_string());
            data.signatories
                .push("some_other_dude_or_dudette".to_string());
            Ok(data)
        });
        identity_store
            .expect_get_peer_id()
            .returning(|| Ok(PeerId::random()));
        storage.expect_update().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);
        let res = service
            .remove_signatory("some_id", "new_signatory_node_id".to_string())
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn save_encrypt_open_decrypt_compare_hashes() {
        let company_id = "00000000-0000-0000-0000-000000000000";
        let file_name = "file_00000000-0000-0000-0000-000000000000.pdf";
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let expected_encrypted =
            util::rsa::encrypt_bytes_with_public_key(&file_bytes, TEST_PUB_KEY);

        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage
            .expect_save_attached_file()
            .with(always(), eq(company_id), eq(file_name))
            .times(1)
            .returning(|_, _, _| Ok(()));

        storage
            .expect_open_attached_file()
            .with(eq(company_id), eq(file_name))
            .times(1)
            .returning(move |_, _| Ok(expected_encrypted.clone()));
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        let file = service
            .encrypt_and_save_uploaded_file(file_name, &file_bytes, company_id, TEST_PUB_KEY)
            .await
            .unwrap();
        assert_eq!(
            file.hash,
            String::from("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
        assert_eq!(file.name, String::from(file_name));

        let decrypted = service
            .open_and_decrypt_file(company_id, file_name, TEST_PRIVATE_KEY)
            .await
            .unwrap();
        assert_eq!(std::str::from_utf8(&decrypted).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn save_encrypt_propagates_write_file_error() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_save_attached_file().returning(|_, _, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        assert!(service
            .encrypt_and_save_uploaded_file("file_name", &[], "test", TEST_PUB_KEY)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn open_decrypt_propagates_read_file_error() {
        let (mut storage, file_upload_store, identity_store, contact_store) = get_storages();
        storage.expect_open_attached_file().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage, file_upload_store, identity_store, contact_store);

        assert!(service
            .open_and_decrypt_file("test", "test", TEST_PRIVATE_KEY)
            .await
            .is_err());
    }
}
