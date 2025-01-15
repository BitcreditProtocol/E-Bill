use borsh_derive::{BorshDeserialize, BorshSerialize};
use rocket::fs::TempFile;
use rocket::FromForm;
use serde::{Deserialize, Serialize};

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
pub struct SellBitcreditBillPayload {
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
pub struct SwitchIdentity {
    pub node_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToPayBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeIdentityPayload {
    pub name: Option<String>,
    pub email: Option<String>,
    pub postal_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityPayload {
    pub name: String,
    pub date_of_birth: String,
    pub city_of_birth: String,
    pub country_of_birth: String,
    pub email: String,
    pub postal_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewContactPayload {
    #[serde(rename = "type")]
    pub t: u64,
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub postal_address: String,
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
    pub postal_address: Option<String>,
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
