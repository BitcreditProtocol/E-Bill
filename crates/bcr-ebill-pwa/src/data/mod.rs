use async_trait::async_trait;
use bcr_ebill_api::{
    data::{
        File, GeneralSearchFilterItemType, GeneralSearchResult, OptionalPostalAddress,
        PostalAddress, UploadFilesResult,
    },
    util::file::{UploadFileHandler, detect_content_type_for_bytes},
};
use bill::LightBitcreditBillWeb;
use company::CompanyWeb;
use contact::ContactWeb;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

pub mod bill;
pub mod company;
pub mod contact;
pub mod identity;
pub mod notification;

pub trait IntoWeb<T> {
    fn into_web(self) -> T;
}

pub trait FromWeb<T> {
    fn from_web(value: T) -> Self;
}

#[wasm_bindgen]
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub bitcoin_network: String,
    #[wasm_bindgen(getter_with_clone)]
    pub app_version: String,
}

#[wasm_bindgen]
impl StatusResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(bitcoin_network: String, app_version: String) -> Self {
        Self {
            bitcoin_network,
            app_version,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize)]
pub struct GeneralSearchResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub bills: Vec<LightBitcreditBillWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub contacts: Vec<ContactWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub companies: Vec<CompanyWeb>,
}

#[wasm_bindgen]
impl GeneralSearchResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(
        bills: Vec<LightBitcreditBillWeb>,
        contacts: Vec<ContactWeb>,
        companies: Vec<CompanyWeb>,
    ) -> Self {
        Self {
            bills,
            contacts,
            companies,
        }
    }
}

impl IntoWeb<GeneralSearchResponse> for GeneralSearchResult {
    fn into_web(self) -> GeneralSearchResponse {
        GeneralSearchResponse {
            bills: self.bills.into_iter().map(|b| b.into_web()).collect(),
            contacts: self.contacts.into_iter().map(|c| c.into_web()).collect(),
            companies: self.companies.into_iter().map(|c| c.into_web()).collect(),
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSearchFilterPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub filter: GeneralSearchFilter,
}

#[wasm_bindgen]
impl GeneralSearchFilterPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(filter: GeneralSearchFilter) -> Self {
        Self { filter }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GeneralSearchFilterItemTypeWeb {
    Company,
    Bill,
    Contact,
}

impl FromWeb<GeneralSearchFilterItemTypeWeb> for GeneralSearchFilterItemType {
    fn from_web(value: GeneralSearchFilterItemTypeWeb) -> Self {
        match value {
            GeneralSearchFilterItemTypeWeb::Company => GeneralSearchFilterItemType::Company,
            GeneralSearchFilterItemTypeWeb::Bill => GeneralSearchFilterItemType::Bill,
            GeneralSearchFilterItemTypeWeb::Contact => GeneralSearchFilterItemType::Contact,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSearchFilter {
    #[wasm_bindgen(getter_with_clone)]
    pub search_term: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub item_types: Vec<GeneralSearchFilterItemTypeWeb>,
}

#[wasm_bindgen]
impl GeneralSearchFilter {
    #[wasm_bindgen(constructor)]
    pub fn new(
        search_term: String,
        currency: String,
        item_types: Vec<GeneralSearchFilterItemTypeWeb>,
    ) -> Self {
        Self {
            search_term,
            currency,
            item_types,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub balances: OverviewBalanceResponse,
}

#[wasm_bindgen]
impl OverviewResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(currency: String, balances: OverviewBalanceResponse) -> Self {
        Self { currency, balances }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewBalanceResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub payee: BalanceResponse,
    #[wasm_bindgen(getter_with_clone)]
    pub payer: BalanceResponse,
    #[wasm_bindgen(getter_with_clone)]
    pub contingent: BalanceResponse,
}

#[wasm_bindgen]
impl OverviewBalanceResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(
        payee: BalanceResponse,
        payer: BalanceResponse,
        contingent: BalanceResponse,
    ) -> Self {
        Self {
            payee,
            payer,
            contingent,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
}

#[wasm_bindgen]
impl BalanceResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(sum: String) -> Self {
        Self { sum }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrenciesResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub currencies: Vec<CurrencyResponse>,
}

#[wasm_bindgen]
impl CurrenciesResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(currencies: Vec<CurrencyResponse>) -> Self {
        Self { currencies }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub code: String,
}

#[wasm_bindgen]
impl CurrencyResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(code: String) -> Self {
        Self { code }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalPostalAddressWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub country: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub zip: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub address: Option<String>,
}

#[wasm_bindgen]
impl OptionalPostalAddressWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        country: Option<String>,
        city: Option<String>,
        zip: Option<String>,
        address: Option<String>,
    ) -> Self {
        Self {
            country,
            city,
            zip,
            address,
        }
    }
}

impl OptionalPostalAddressWeb {
    pub fn is_none(&self) -> bool {
        self.country.is_none()
            && self.city.is_none()
            && self.zip.is_none()
            && self.address.is_none()
    }
}

impl FromWeb<OptionalPostalAddressWeb> for OptionalPostalAddress {
    fn from_web(value: OptionalPostalAddressWeb) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl IntoWeb<OptionalPostalAddressWeb> for OptionalPostalAddress {
    fn into_web(self) -> OptionalPostalAddressWeb {
        OptionalPostalAddressWeb {
            country: self.country,
            city: self.city,
            zip: self.zip,
            address: self.address,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostalAddressWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub country: String,
    #[wasm_bindgen(getter_with_clone)]
    pub city: String,
    #[wasm_bindgen(getter_with_clone)]
    pub zip: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub address: String,
}

#[wasm_bindgen]
impl PostalAddressWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(country: String, city: String, zip: Option<String>, address: String) -> Self {
        Self {
            country,
            city,
            zip,
            address,
        }
    }
}

impl FromWeb<PostalAddressWeb> for PostalAddress {
    fn from_web(value: PostalAddressWeb) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl IntoWeb<PostalAddressWeb> for PostalAddress {
    fn into_web(self) -> PostalAddressWeb {
        PostalAddressWeb {
            country: self.country,
            city: self.city,
            zip: self.zip,
            address: self.address,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub hash: String,
}

#[wasm_bindgen]
impl FileWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(name: String, hash: String) -> Self {
        Self { name, hash }
    }
}

impl FromWeb<FileWeb> for File {
    fn from_web(value: FileWeb) -> Self {
        Self {
            name: value.name,
            hash: value.hash,
        }
    }
}

impl IntoWeb<FileWeb> for File {
    fn into_web(self) -> FileWeb {
        FileWeb {
            name: self.name,
            hash: self.hash,
        }
    }
}

/// Just a wrapper struct to allow setting a content disposition header
#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryFileResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub data: Vec<u8>,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub content_type: String,
}

#[wasm_bindgen]
impl BinaryFileResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<u8>, name: String, content_type: String) -> Self {
        Self {
            data,
            name,
            content_type,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadFile {
    #[wasm_bindgen(getter_with_clone)]
    pub data: Vec<u8>,
    #[wasm_bindgen(getter_with_clone)]
    pub extension: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
}

#[wasm_bindgen]
impl UploadFile {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<u8>, extension: Option<String>, name: String) -> Self {
        Self {
            data,
            extension,
            name,
        }
    }
}

#[async_trait]
impl UploadFileHandler for UploadFile {
    async fn get_contents(&self) -> std::io::Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn extension(&self) -> Option<String> {
        self.extension.clone()
    }

    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn len(&self) -> u64 {
        self.data.len() as u64
    }
    async fn detect_content_type(&self) -> std::io::Result<Option<String>> {
        Ok(detect_content_type_for_bytes(&self.data))
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UploadFilesResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub file_upload_id: String,
}

impl IntoWeb<UploadFilesResponse> for UploadFilesResult {
    fn into_web(self) -> UploadFilesResponse {
        UploadFilesResponse {
            file_upload_id: self.file_upload_id,
        }
    }
}

#[wasm_bindgen]
impl UploadFilesResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(file_upload_id: String) -> Self {
        Self { file_upload_id }
    }
}
