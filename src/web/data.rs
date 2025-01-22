use borsh_derive::{BorshDeserialize, BorshSerialize};
use rocket::fs::TempFile;
use rocket::FromForm;
use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize)]
pub struct BitcreditBillPayload {
    pub bill_jurisdiction: String,
    pub place_of_drawing: String,
    pub currency_code: String,
    pub amount_numbers: u64,
    pub language: String,
    pub drawee: String,
    pub payee: String,
    pub place_of_payment: String,
    pub maturity_date: String,
    pub drawer_is_payee: bool,
    pub drawer_is_drawee: bool,
    pub file_upload_id: Option<String>,
}

#[derive(Debug, FromForm)]
pub struct UploadBillFilesForm<'r> {
    pub files: Vec<TempFile<'r>>,
}

#[derive(Debug, FromForm)]
pub struct UploadFileForm<'r> {
    pub file: TempFile<'r>,
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
    pub amount_numbers: u64,
    pub currency_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptMintBitcreditBillPayload {
    pub amount: u64,
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
    pub amount_numbers: u64,
    pub currency_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToAcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillCombinedBitcoinKey {
    pub private_key: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SwitchIdentity {
    pub node_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToPayBitcreditBillPayload {
    pub bill_id: String,
    pub currency_code: String,
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
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityPayload {
    pub name: String,
    pub date_of_birth: String,
    pub city_of_birth: String,
    pub country_of_birth: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: PostalAddress,
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

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq)]
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
    pub zip: String,
    pub address: String,
}

impl fmt::Display for PostalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}, {} {}, {}",
            self.address, self.zip, self.city, self.country
        )
    }
}

impl PostalAddress {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            country: "".to_string(),
            city: "".to_string(),
            zip: "".to_string(),
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
