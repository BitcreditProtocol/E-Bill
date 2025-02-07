use crate::service::identity_service::IdentityType;
use crate::service::{
    bill_service::LightBitcreditBillToReturn,
    company_service::CompanyToReturn,
    contact_service::{Contact, LightIdentityPublicData},
    Error,
};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use rocket::fs::TempFile;
use rocket::FromForm;
use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct PastEndorseesResponse {
    pub past_endorsees: Vec<PastEndorsee>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PastEndorsee {
    pub pay_to_the_order_of: LightIdentityPublicData,
    pub signed: LightSignedBy,
    pub signing_timestamp: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LightSignedBy {
    #[serde(flatten)]
    pub data: LightIdentityPublicData,
    pub signatory: Option<LightIdentityPublicData>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GeneralSearchResponse {
    pub bills: Vec<LightBitcreditBillToReturn>,
    pub contacts: Vec<Contact>,
    pub companies: Vec<CompanyToReturn>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BillsResponse<T: Serialize> {
    pub bills: Vec<T>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContactsResponse<T: Serialize> {
    pub contacts: Vec<T>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CompaniesResponse<T: Serialize> {
    pub companies: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralSearchFilterPayload {
    pub filter: GeneralSearchFilter,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GeneralSearchFilterItemType {
    Company,
    Bill,
    Contact,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralSearchFilter {
    pub search_term: String,
    pub currency: String,
    pub item_types: Vec<GeneralSearchFilterItemType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillsSearchFilterPayload {
    pub filter: BillsSearchFilter,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillsSearchFilter {
    pub search_term: Option<String>,
    pub date_range: Option<DateRange>,
    pub role: BillsFilterRole,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BillsFilterRole {
    All,
    Payer,
    Payee,
    Contingent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OverviewResponse {
    pub currency: String,
    pub balances: OverviewBalanceResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OverviewBalanceResponse {
    pub payee: BalanceResponse,
    pub payer: BalanceResponse,
    pub contingent: BalanceResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub sum: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrenciesResponse {
    pub currencies: Vec<CurrencyResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrencyResponse {
    pub code: String,
}

#[repr(u8)]
#[derive(
    Debug,
    Clone,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    PartialEq,
    Eq,
    ToSchema,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
pub enum BillType {
    PromissoryNote = 0, // Drawer pays to payee
    SelfDrafted = 1,    // Drawee pays to drawer
    ThreeParties = 2,   // Drawee pays to payee
}

impl TryFrom<u64> for BillType {
    type Error = Error;

    fn try_from(value: u64) -> std::result::Result<Self, Error> {
        match value {
            0 => Ok(BillType::PromissoryNote),
            1 => Ok(BillType::SelfDrafted),
            2 => Ok(BillType::ThreeParties),
            _ => Err(Error::Validation(format!(
                "Invalid bill type found: {value}"
            ))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BitcreditBillPayload {
    #[serde(rename = "type")]
    pub t: u64,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    pub issue_date: String,
    pub maturity_date: String,
    pub payee: String,
    pub drawee: String,
    pub sum: String,
    pub currency: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub file_upload_id: Option<String>,
}

#[derive(Debug, FromForm)]
pub struct UploadBillFilesForm<'r> {
    pub files: Vec<TempFile<'r>>,
}

#[derive(Debug, FromForm, ToSchema)]
pub struct UploadFileForm<'r> {
    #[schema(value_type = String, format = Binary)]
    pub file: TempFile<'r>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillId {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillNumbersToWordsForSum {
    pub sum: u64,
    pub sum_as_words: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EndorseBitcreditBillPayload {
    pub endorsee: String,
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptMintBitcreditBillPayload {
    pub sum: String,
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestToMintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OfferToSellBitcreditBillPayload {
    pub buyer: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToAcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RejectActionBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillCombinedBitcoinKey {
    pub private_key: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SwitchIdentity {
    #[serde(rename = "type")]
    pub t: Option<IdentityType>,
    pub node_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToPayBitcreditBillPayload {
    pub bill_id: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestRecourseForPaymentPayload {
    pub bill_id: String,
    pub recoursee: String,
    pub currency: String,
    pub sum: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestRecourseForAcceptancePayload {
    pub bill_id: String,
    pub recoursee: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChangeIdentityPayload {
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddress,
    pub profile_picture_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewIdentityPayload {
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddress,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file_upload_id: Option<String>,
    pub identity_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewContactPayload {
    #[serde(rename = "type")]
    pub t: u64,
    pub node_id: String,
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: PostalAddress,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file_upload_id: Option<String>,
    pub proof_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EditContactPayload {
    pub node_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddress,
    pub avatar_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UploadFilesResponse {
    pub file_upload_id: String,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema,
)]
pub struct File {
    pub name: String,
    pub hash: String,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema,
)]
pub struct OptionalPostalAddress {
    pub country: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub address: Option<String>,
}

impl OptionalPostalAddress {
    pub fn is_none(&self) -> bool {
        self.country.is_none()
            && self.city.is_none()
            && self.zip.is_none()
            && self.address.is_none()
    }

    pub fn is_fully_set(&self) -> bool {
        self.country.is_some() && self.city.is_some() && self.address.is_some()
    }

    pub fn to_full_postal_address(&self) -> Option<PostalAddress> {
        if self.is_fully_set() {
            return Some(PostalAddress {
                country: self.country.clone().expect("checked above"),
                city: self.city.clone().expect("checked above"),
                zip: self.zip.clone(),
                address: self.address.clone().expect("checked above"),
            });
        }
        None
    }

    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            country: None,
            city: None,
            zip: None,
            address: None,
        }
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema,
)]
pub struct PostalAddress {
    pub country: String,
    pub city: String,
    pub zip: Option<String>,
    pub address: String,
}

impl fmt::Display for PostalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.zip {
            Some(ref zip) => {
                write!(
                    f,
                    "{}, {} {}, {}",
                    self.address, zip, self.city, self.country
                )
            }
            None => {
                write!(f, "{}, {}, {}", self.address, self.city, self.country)
            }
        }
    }
}

impl PostalAddress {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            country: "".to_string(),
            city: "".to_string(),
            zip: None,
            address: "".to_string(),
        }
    }
}

/// Response for a private key seeed backup
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SeedPhrase {
    /// The seed phrase of the current private key
    pub seed_phrase: String,
}
