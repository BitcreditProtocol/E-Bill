use async_trait::async_trait;
use bcr_ebill_api::data::{
    File, GeneralSearchFilterItemType, GeneralSearchResult, OptionalPostalAddress, PostalAddress,
    UploadFilesResult,
    bill::{
        BillAcceptanceStatus, BillCombinedBitcoinKey, BillCurrentWaitingState, BillData,
        BillParticipants, BillPaymentStatus, BillRecourseStatus, BillSellStatus, BillStatus,
        BillWaitingForPaymentState, BillWaitingForRecourseState, BillWaitingForSellState,
        BillsFilterRole, BitcreditBillResult, Endorsement, LightBitcreditBillResult, LightSignedBy,
        PastEndorsee,
    },
    company::Company,
    contact::{
        Contact, ContactType, IdentityPublicData, LightIdentityPublicData,
        LightIdentityPublicDataWithAddress,
    },
    identity::{Identity, IdentityType},
    notification::{Notification, NotificationType},
};
use bcr_ebill_api::service::Error;
use bcr_ebill_api::util::file::{UploadFileHandler, detect_content_type_for_bytes};
use bcr_ebill_api::util::{BcrKeys, date::DateTimeUtc};
use rocket::FromForm;
use rocket::fs::TempFile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

pub trait IntoWeb<T> {
    fn into_web(self) -> T;
}

pub trait FromWeb<T> {
    fn from_web(value: T) -> Self;
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    pub bitcoin_network: String,
    pub app_version: String,
}

/// A dummy response type signaling success of a request
#[derive(Debug, Serialize, ToSchema)]
pub struct SuccessResponse {
    pub success: bool,
}

impl SuccessResponse {
    pub fn new() -> Self {
        Self { success: true }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EndorsementsResponse {
    pub endorsements: Vec<EndorsementWeb>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PastEndorseesResponse {
    pub past_endorsees: Vec<PastEndorseeWeb>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GeneralSearchResponse {
    pub bills: Vec<LightBitcreditBillWeb>,
    pub contacts: Vec<ContactWeb>,
    pub companies: Vec<CompanyWeb>,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GeneralSearchFilterPayload {
    pub filter: GeneralSearchFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GeneralSearchFilter {
    pub search_term: String,
    pub currency: String,
    pub item_types: Vec<GeneralSearchFilterItemTypeWeb>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillsSearchFilterPayload {
    pub filter: BillsSearchFilter,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillsSearchFilter {
    pub search_term: Option<String>,
    pub date_range: Option<DateRange>,
    pub role: BillsFilterRoleWeb,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OverviewResponse {
    pub currency: String,
    pub balances: OverviewBalanceResponse,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OverviewBalanceResponse {
    pub payee: BalanceResponse,
    pub payer: BalanceResponse,
    pub contingent: BalanceResponse,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BalanceResponse {
    pub sum: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CurrenciesResponse {
    pub currencies: Vec<CurrencyResponse>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CurrencyResponse {
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
    pub file_upload_ids: Vec<String>,
}

#[derive(Debug, FromForm, ToSchema)]
pub struct UploadFileForm<'r> {
    #[schema(value_type = String, format = Binary)]
    pub file: TempFile<'r>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillId {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillNumbersToWordsForSum {
    pub sum: u64,
    pub sum_as_words: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EndorseBitcreditBillPayload {
    pub endorsee: String,
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct RequestToMintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OfferToSellBitcreditBillPayload {
    pub buyer: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RequestToAcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RejectActionBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillCombinedBitcoinKeyWeb {
    pub private_key: String,
}

impl IntoWeb<BillCombinedBitcoinKeyWeb> for BillCombinedBitcoinKey {
    fn into_web(self) -> BillCombinedBitcoinKeyWeb {
        BillCombinedBitcoinKeyWeb {
            private_key: self.private_key,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub enum BillsFilterRoleWeb {
    All,
    Payer,
    Payee,
    Contingent,
}

impl FromWeb<BillsFilterRoleWeb> for BillsFilterRole {
    fn from_web(value: BillsFilterRoleWeb) -> Self {
        match value {
            BillsFilterRoleWeb::All => BillsFilterRole::All,
            BillsFilterRoleWeb::Payer => BillsFilterRole::Payer,
            BillsFilterRoleWeb::Payee => BillsFilterRole::Payee,
            BillsFilterRoleWeb::Contingent => BillsFilterRole::Contingent,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PastEndorseeWeb {
    pub pay_to_the_order_of: LightIdentityPublicDataWeb,
    pub signed: LightSignedByWeb,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddressWeb,
}

impl IntoWeb<PastEndorseeWeb> for PastEndorsee {
    fn into_web(self) -> PastEndorseeWeb {
        PastEndorseeWeb {
            pay_to_the_order_of: self.pay_to_the_order_of.into_web(),
            signed: self.signed.into_web(),
            signing_timestamp: self.signing_timestamp,
            signing_address: self.signing_address.into_web(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LightSignedByWeb {
    #[serde(flatten)]
    pub data: LightIdentityPublicDataWeb,
    pub signatory: Option<LightIdentityPublicDataWeb>,
}

impl IntoWeb<LightSignedByWeb> for LightSignedBy {
    fn into_web(self) -> LightSignedByWeb {
        LightSignedByWeb {
            data: self.data.into_web(),
            signatory: self.signatory.map(|s| s.into_web()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EndorsementWeb {
    pub pay_to_the_order_of: LightIdentityPublicDataWithAddressWeb,
    pub signed: LightSignedByWeb,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddressWeb,
}

impl IntoWeb<EndorsementWeb> for Endorsement {
    fn into_web(self) -> EndorsementWeb {
        EndorsementWeb {
            pay_to_the_order_of: self.pay_to_the_order_of.into_web(),
            signed: self.signed.into_web(),
            signing_timestamp: self.signing_timestamp,
            signing_address: self.signing_address.into_web(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SwitchIdentity {
    #[serde(rename = "type")]
    pub t: Option<IdentityTypeWeb>,
    pub node_id: String,
}

#[repr(u8)]
#[derive(
    Debug, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, PartialEq, Eq, ToSchema,
)]
pub enum IdentityTypeWeb {
    Person = 0,
    Company = 1,
}

impl IntoWeb<IdentityTypeWeb> for IdentityType {
    fn into_web(self) -> IdentityTypeWeb {
        match self {
            IdentityType::Person => IdentityTypeWeb::Person,
            IdentityType::Company => IdentityTypeWeb::Company,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RequestToPayBitcreditBillPayload {
    pub bill_id: String,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RequestRecourseForPaymentPayload {
    pub bill_id: String,
    pub recoursee: String,
    pub currency: String,
    pub sum: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RequestRecourseForAcceptancePayload {
    pub bill_id: String,
    pub recoursee: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChangeIdentityPayload {
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file_upload_id: Option<String>,
    pub identity_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewIdentityPayload {
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file_upload_id: Option<String>,
    pub identity_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewContactPayload {
    #[serde(rename = "type")]
    pub t: u64,
    pub node_id: String,
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file_upload_id: Option<String>,
    pub proof_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EditContactPayload {
    pub node_id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file_upload_id: Option<String>,
    pub proof_document_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UploadFilesResponse {
    pub file_upload_id: String,
}

impl IntoWeb<UploadFilesResponse> for UploadFilesResult {
    fn into_web(self) -> UploadFilesResponse {
        UploadFilesResponse {
            file_upload_id: self.file_upload_id,
        }
    }
}

/// Response for a private key seeed backup
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SeedPhrase {
    /// The seed phrase of the current private key
    pub seed_phrase: String,
}

// Company
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct CreateCompanyPayload {
    pub name: String,
    pub country_of_registration: Option<String>,
    pub city_of_registration: Option<String>,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub email: String,
    pub registration_number: Option<String>,
    pub registration_date: Option<String>,
    pub proof_of_registration_file_upload_id: Option<String>,
    pub logo_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct EditCompanyPayload {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub country_of_registration: Option<String>,
    pub city_of_registration: Option<String>,
    pub registration_number: Option<String>,
    pub registration_date: Option<String>,
    pub logo_file_upload_id: Option<String>,
    pub proof_of_registration_file_upload_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct AddSignatoryPayload {
    pub id: String,
    pub signatory_node_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct RemoveSignatoryPayload {
    pub id: String,
    pub signatory_node_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ListSignatoriesResponse {
    pub signatories: Vec<SignatoryResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct SignatoryResponse {
    #[serde(rename = "type")]
    pub t: ContactTypeWeb,
    pub node_id: String,
    pub name: String,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub avatar_file: Option<FileWeb>,
}

impl From<Contact> for SignatoryResponse {
    fn from(value: Contact) -> Self {
        Self {
            t: value.t.into_web(),
            node_id: value.node_id,
            name: value.name,
            postal_address: value.postal_address.into_web(),
            avatar_file: value.avatar_file.map(|f| f.into_web()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityWeb {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub bitcoin_public_key: String,
    pub npub: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file: Option<FileWeb>,
    pub identity_document_file: Option<FileWeb>,
    pub nostr_relay: Option<String>,
}

impl IdentityWeb {
    pub fn from(identity: Identity, keys: BcrKeys) -> Self {
        Self {
            node_id: identity.node_id.clone(),
            name: identity.name,
            email: identity.email,
            bitcoin_public_key: identity.node_id.clone(),
            npub: keys.get_nostr_npub(),
            postal_address: identity.postal_address.into_web(),
            date_of_birth: identity.date_of_birth,
            country_of_birth: identity.country_of_birth,
            city_of_birth: identity.city_of_birth,
            identification_number: identity.identification_number,
            profile_picture_file: identity.profile_picture_file.map(|f| f.into_web()),
            identity_document_file: identity.identity_document_file.map(|f| f.into_web()),
            nostr_relay: identity.nostr_relay,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PostalAddressWeb {
    pub country: String,
    pub city: String,
    pub zip: Option<String>,
    pub address: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OptionalPostalAddressWeb {
    pub country: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub address: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileWeb {
    pub name: String,
    pub hash: String,
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

#[repr(u8)]
#[derive(
    Debug, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, PartialEq, Eq, ToSchema,
)]
pub enum ContactTypeWeb {
    Person = 0,
    Company = 1,
}

impl TryFrom<u64> for ContactTypeWeb {
    type Error = Error;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(ContactTypeWeb::Person),
            1 => Ok(ContactTypeWeb::Company),
            _ => Err(Error::Validation(format!(
                "Invalid contact type found: {value}"
            ))),
        }
    }
}

impl IntoWeb<ContactTypeWeb> for ContactType {
    fn into_web(self) -> ContactTypeWeb {
        match self {
            ContactType::Person => ContactTypeWeb::Person,
            ContactType::Company => ContactTypeWeb::Company,
        }
    }
}

impl FromWeb<ContactTypeWeb> for ContactType {
    fn from_web(value: ContactTypeWeb) -> Self {
        match value {
            ContactTypeWeb::Person => ContactType::Person,
            ContactTypeWeb::Company => ContactType::Company,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContactWeb {
    #[serde(rename = "type")]
    pub t: ContactTypeWeb,
    pub node_id: String,
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file: Option<FileWeb>,
    pub proof_document_file: Option<FileWeb>,
    pub nostr_relays: Vec<String>,
}

impl IntoWeb<ContactWeb> for Contact {
    fn into_web(self) -> ContactWeb {
        ContactWeb {
            t: self.t.into_web(),
            node_id: self.node_id,
            name: self.name,
            email: self.email,
            postal_address: self.postal_address.into_web(),
            date_of_birth_or_registration: self.date_of_birth_or_registration,
            country_of_birth_or_registration: self.country_of_birth_or_registration,
            city_of_birth_or_registration: self.city_of_birth_or_registration,
            identification_number: self.identification_number,
            avatar_file: self.avatar_file.map(|f| f.into_web()),
            proof_document_file: self.proof_document_file.map(|f| f.into_web()),
            nostr_relays: self.nostr_relays,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct CompanyWeb {
    pub id: String,
    pub name: String,
    pub country_of_registration: Option<String>,
    pub city_of_registration: Option<String>,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub email: String,
    pub registration_number: Option<String>,
    pub registration_date: Option<String>,
    pub proof_of_registration_file: Option<FileWeb>,
    pub logo_file: Option<FileWeb>,
    pub signatories: Vec<String>,
}

impl IntoWeb<CompanyWeb> for Company {
    fn into_web(self) -> CompanyWeb {
        CompanyWeb {
            id: self.id,
            name: self.name,
            country_of_registration: self.country_of_registration,
            city_of_registration: self.city_of_registration,
            postal_address: self.postal_address.into_web(),
            email: self.email,
            registration_number: self.registration_number,
            registration_date: self.registration_date,
            proof_of_registration_file: self.proof_of_registration_file.map(|f| f.into_web()),
            logo_file: self.logo_file.map(|f| f.into_web()),
            signatories: self.signatories,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct BitcreditEbillQuote {
    pub bill_id: String,
    pub quote_id: String,
    pub sum: u64,
    pub mint_node_id: String,
    pub mint_url: String,
    pub accepted: bool,
    pub token: String,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BitcreditBillWeb {
    pub id: String,
    pub participants: BillParticipantsWeb,
    pub data: BillDataWeb,
    pub status: BillStatusWeb,
    pub current_waiting_state: Option<BillCurrentWaitingStateWeb>,
}

impl IntoWeb<BitcreditBillWeb> for BitcreditBillResult {
    fn into_web(self) -> BitcreditBillWeb {
        BitcreditBillWeb {
            id: self.id,
            participants: self.participants.into_web(),
            data: self.data.into_web(),
            status: self.status.into_web(),
            current_waiting_state: self.current_waiting_state.map(|cws| cws.into_web()),
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub enum BillCurrentWaitingStateWeb {
    Sell(BillWaitingForSellStateWeb),
    Payment(BillWaitingForPaymentStateWeb),
    Recourse(BillWaitingForRecourseStateWeb),
}

impl IntoWeb<BillCurrentWaitingStateWeb> for BillCurrentWaitingState {
    fn into_web(self) -> BillCurrentWaitingStateWeb {
        match self {
            BillCurrentWaitingState::Sell(state) => {
                BillCurrentWaitingStateWeb::Sell(state.into_web())
            }
            BillCurrentWaitingState::Payment(state) => {
                BillCurrentWaitingStateWeb::Payment(state.into_web())
            }
            BillCurrentWaitingState::Recourse(state) => {
                BillCurrentWaitingStateWeb::Recourse(state.into_web())
            }
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillWaitingForSellStateWeb {
    pub time_of_request: u64,
    pub buyer: IdentityPublicDataWeb,
    pub seller: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
}

impl IntoWeb<BillWaitingForSellStateWeb> for BillWaitingForSellState {
    fn into_web(self) -> BillWaitingForSellStateWeb {
        BillWaitingForSellStateWeb {
            time_of_request: self.time_of_request,
            buyer: self.buyer.into_web(),
            seller: self.seller.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillWaitingForPaymentStateWeb {
    pub time_of_request: u64,
    pub payer: IdentityPublicDataWeb,
    pub payee: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
}

impl IntoWeb<BillWaitingForPaymentStateWeb> for BillWaitingForPaymentState {
    fn into_web(self) -> BillWaitingForPaymentStateWeb {
        BillWaitingForPaymentStateWeb {
            time_of_request: self.time_of_request,
            payer: self.payer.into_web(),
            payee: self.payee.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillWaitingForRecourseStateWeb {
    pub time_of_request: u64,
    pub recourser: IdentityPublicDataWeb,
    pub recoursee: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
}
impl IntoWeb<BillWaitingForRecourseStateWeb> for BillWaitingForRecourseState {
    fn into_web(self) -> BillWaitingForRecourseStateWeb {
        BillWaitingForRecourseStateWeb {
            time_of_request: self.time_of_request,
            recourser: self.recourser.into_web(),
            recoursee: self.recoursee.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillStatusWeb {
    pub acceptance: BillAcceptanceStatusWeb,
    pub payment: BillPaymentStatusWeb,
    pub sell: BillSellStatusWeb,
    pub recourse: BillRecourseStatusWeb,
    pub redeemed_funds_available: bool,
}

impl IntoWeb<BillStatusWeb> for BillStatus {
    fn into_web(self) -> BillStatusWeb {
        BillStatusWeb {
            acceptance: self.acceptance.into_web(),
            payment: self.payment.into_web(),
            sell: self.sell.into_web(),
            recourse: self.recourse.into_web(),
            redeemed_funds_available: self.redeemed_funds_available,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillAcceptanceStatusWeb {
    pub requested_to_accept: bool,
    pub accepted: bool,
    pub request_to_accept_timed_out: bool,
    pub rejected_to_accept: bool,
}

impl IntoWeb<BillAcceptanceStatusWeb> for BillAcceptanceStatus {
    fn into_web(self) -> BillAcceptanceStatusWeb {
        BillAcceptanceStatusWeb {
            requested_to_accept: self.requested_to_accept,
            accepted: self.accepted,
            request_to_accept_timed_out: self.request_to_accept_timed_out,
            rejected_to_accept: self.rejected_to_accept,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillPaymentStatusWeb {
    pub requested_to_pay: bool,
    pub paid: bool,
    pub request_to_pay_timed_out: bool,
    pub rejected_to_pay: bool,
}
impl IntoWeb<BillPaymentStatusWeb> for BillPaymentStatus {
    fn into_web(self) -> BillPaymentStatusWeb {
        BillPaymentStatusWeb {
            requested_to_pay: self.requested_to_pay,
            paid: self.paid,
            request_to_pay_timed_out: self.request_to_pay_timed_out,
            rejected_to_pay: self.rejected_to_pay,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillSellStatusWeb {
    pub offered_to_sell: bool,
    pub offer_to_sell_timed_out: bool,
    pub rejected_offer_to_sell: bool,
}
impl IntoWeb<BillSellStatusWeb> for BillSellStatus {
    fn into_web(self) -> BillSellStatusWeb {
        BillSellStatusWeb {
            offered_to_sell: self.offered_to_sell,
            offer_to_sell_timed_out: self.offer_to_sell_timed_out,
            rejected_offer_to_sell: self.rejected_offer_to_sell,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillRecourseStatusWeb {
    pub requested_to_recourse: bool,
    pub request_to_recourse_timed_out: bool,
    pub rejected_request_to_recourse: bool,
}

impl IntoWeb<BillRecourseStatusWeb> for BillRecourseStatus {
    fn into_web(self) -> BillRecourseStatusWeb {
        BillRecourseStatusWeb {
            requested_to_recourse: self.requested_to_recourse,
            request_to_recourse_timed_out: self.request_to_recourse_timed_out,
            rejected_request_to_recourse: self.rejected_request_to_recourse,
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillDataWeb {
    pub language: String,
    pub time_of_drawing: u64,
    pub issue_date: String,
    pub time_of_maturity: u64,
    pub maturity_date: String,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub currency: String,
    pub sum: String,
    pub files: Vec<FileWeb>,
    pub active_notification: Option<NotificationWeb>,
}

impl IntoWeb<BillDataWeb> for BillData {
    fn into_web(self) -> BillDataWeb {
        BillDataWeb {
            language: self.language,
            time_of_drawing: self.time_of_drawing,
            issue_date: self.issue_date,
            time_of_maturity: self.time_of_maturity,
            maturity_date: self.maturity_date,
            country_of_issuing: self.country_of_issuing,
            city_of_issuing: self.city_of_issuing,
            country_of_payment: self.country_of_payment,
            city_of_payment: self.city_of_payment,
            currency: self.currency,
            sum: self.sum,
            files: self.files.into_iter().map(|f| f.into_web()).collect(),
            active_notification: self.active_notification.map(|an| an.into_web()),
        }
    }
}

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct BillParticipantsWeb {
    pub drawee: IdentityPublicDataWeb,
    pub drawer: IdentityPublicDataWeb,
    pub payee: IdentityPublicDataWeb,
    pub endorsee: Option<IdentityPublicDataWeb>,
    pub endorsements_count: u64,
    pub all_participant_node_ids: Vec<String>,
}

impl IntoWeb<BillParticipantsWeb> for BillParticipants {
    fn into_web(self) -> BillParticipantsWeb {
        BillParticipantsWeb {
            drawee: self.drawee.into_web(),
            drawer: self.drawer.into_web(),
            payee: self.payee.into_web(),
            endorsee: self.endorsee.map(|e| e.into_web()),
            endorsements_count: self.endorsements_count,
            all_participant_node_ids: self.all_participant_node_ids,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct LightBitcreditBillWeb {
    pub id: String,
    pub drawee: LightIdentityPublicDataWeb,
    pub drawer: LightIdentityPublicDataWeb,
    pub payee: LightIdentityPublicDataWeb,
    pub endorsee: Option<LightIdentityPublicDataWeb>,
    pub active_notification: Option<NotificationWeb>,
    pub sum: String,
    pub currency: String,
    pub issue_date: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
}
impl IntoWeb<LightBitcreditBillWeb> for LightBitcreditBillResult {
    fn into_web(self) -> LightBitcreditBillWeb {
        LightBitcreditBillWeb {
            id: self.id,
            drawee: self.drawee.into_web(),
            drawer: self.drawer.into_web(),
            payee: self.payee.into_web(),
            endorsee: self.endorsee.map(|e| e.into_web()),
            active_notification: self.active_notification.map(|n| n.into_web()),
            sum: self.sum,
            currency: self.currency,
            issue_date: self.issue_date,
            time_of_drawing: self.time_of_drawing,
            time_of_maturity: self.time_of_maturity,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct IdentityPublicDataWeb {
    #[serde(rename = "type")]
    pub t: ContactTypeWeb,
    pub node_id: String,
    pub name: String,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
    pub email: Option<String>,
    pub nostr_relay: Option<String>,
}

impl IntoWeb<IdentityPublicDataWeb> for IdentityPublicData {
    fn into_web(self) -> IdentityPublicDataWeb {
        IdentityPublicDataWeb {
            t: self.t.into_web(),
            name: self.name,
            node_id: self.node_id,
            postal_address: self.postal_address.into_web(),
            email: self.email,
            nostr_relay: self.nostr_relay,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct LightIdentityPublicDataWithAddressWeb {
    #[serde(rename = "type")]
    pub t: ContactTypeWeb,
    pub name: String,
    pub node_id: String,
    #[serde(flatten)]
    pub postal_address: PostalAddressWeb,
}

impl IntoWeb<LightIdentityPublicDataWithAddressWeb> for LightIdentityPublicDataWithAddress {
    fn into_web(self) -> LightIdentityPublicDataWithAddressWeb {
        LightIdentityPublicDataWithAddressWeb {
            t: self.t.into_web(),
            name: self.name,
            node_id: self.node_id,
            postal_address: self.postal_address.into_web(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct LightIdentityPublicDataWeb {
    #[serde(rename = "type")]
    pub t: ContactTypeWeb,
    pub name: String,
    pub node_id: String,
}

impl IntoWeb<LightIdentityPublicDataWeb> for LightIdentityPublicData {
    fn into_web(self) -> LightIdentityPublicDataWeb {
        LightIdentityPublicDataWeb {
            t: self.t.into_web(),
            name: self.name,
            node_id: self.node_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationWeb {
    pub id: String,
    pub node_id: Option<String>,
    pub notification_type: NotificationTypeWeb,
    pub reference_id: Option<String>,
    pub description: String,
    #[schema(value_type = chrono::DateTime<chrono::Utc>)]
    pub datetime: DateTimeUtc,
    pub active: bool,
    pub payload: Option<Value>,
}
impl IntoWeb<NotificationWeb> for Notification {
    fn into_web(self) -> NotificationWeb {
        NotificationWeb {
            id: self.id,
            node_id: self.node_id,
            notification_type: self.notification_type.into_web(),
            reference_id: self.reference_id,
            description: self.description,
            datetime: self.datetime,
            active: self.active,
            payload: self.payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum NotificationTypeWeb {
    General,
    Bill,
}

impl IntoWeb<NotificationTypeWeb> for NotificationType {
    fn into_web(self) -> NotificationTypeWeb {
        match self {
            NotificationType::Bill => NotificationTypeWeb::Bill,
            NotificationType::General => NotificationTypeWeb::General,
        }
    }
}

pub struct TempFileWrapper<'a>(pub &'a TempFile<'a>);

#[async_trait]
impl UploadFileHandler for TempFileWrapper<'_> {
    async fn get_contents(&self) -> std::io::Result<Vec<u8>> {
        let mut opened = self.0.open().await?;
        let mut buf = Vec::with_capacity(self.0.len() as usize);
        opened.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    fn extension(&self) -> Option<String> {
        self.0
            .content_type()
            .and_then(|c| c.extension().map(|e| e.to_string()))
    }

    fn name(&self) -> Option<String> {
        self.0.name().map(|s| s.to_owned())
    }

    fn len(&self) -> u64 {
        self.0.len()
    }
    async fn detect_content_type(&self) -> std::io::Result<Option<String>> {
        let mut buffer = vec![0; 256];
        let mut opened = self.0.open().await?;
        let _bytes_read = opened.read(&mut buffer).await?;
        Ok(detect_content_type_for_bytes(&buffer))
    }
}
