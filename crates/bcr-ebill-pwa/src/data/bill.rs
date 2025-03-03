use bcr_ebill_api::data::{
    bill::{
        BillCombinedBitcoinKey, BillsFilterRole, BitcreditBillResult, Endorsement,
        LightBitcreditBillResult, LightSignedBy, PastEndorsee,
    },
    contact::{IdentityPublicData, LightIdentityPublicData, LightIdentityPublicDataWithAddress},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use super::{
    FileWeb, FromWeb, IntoWeb, PostalAddressWeb, contact::ContactTypeWeb,
    notification::NotificationWeb,
};

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct BillId {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
}

#[wasm_bindgen]
impl BillId {
    #[wasm_bindgen(constructor)]
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcreditBillPayload {
    pub t: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_issuing: String,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_issuing: String,
    #[wasm_bindgen(getter_with_clone)]
    pub issue_date: String,
    #[wasm_bindgen(getter_with_clone)]
    pub maturity_date: String,
    #[wasm_bindgen(getter_with_clone)]
    pub payee: String,
    #[wasm_bindgen(getter_with_clone)]
    pub drawee: String,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_payment: String,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_payment: String,
    #[wasm_bindgen(getter_with_clone)]
    pub language: String,
    #[wasm_bindgen(getter_with_clone)]
    pub file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl BitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: u64,
        country_of_issuing: String,
        city_of_issuing: String,
        issue_date: String,
        maturity_date: String,
        payee: String,
        drawee: String,
        sum: String,
        currency: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        file_upload_id: Option<String>,
    ) -> Self {
        Self {
            t,
            country_of_issuing,
            city_of_issuing,
            issue_date,
            maturity_date,
            payee,
            drawee,
            sum,
            currency,
            country_of_payment,
            city_of_payment,
            language,
            file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillNumbersToWordsForSum {
    pub sum: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub sum_as_words: String,
}

#[wasm_bindgen]
impl BillNumbersToWordsForSum {
    #[wasm_bindgen(constructor)]
    pub fn new(sum: u64, sum_as_words: String) -> Self {
        Self { sum, sum_as_words }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndorseBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub endorsee: String,
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
}

#[wasm_bindgen]
impl EndorseBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(endorsee: String, bill_id: String) -> Self {
        Self { endorsee, bill_id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub mint_node: String,
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
}

#[wasm_bindgen]
impl MintBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(mint_node: String, bill_id: String, sum: String, currency: String) -> Self {
        Self {
            mint_node,
            bill_id,
            sum,
            currency,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestToMintBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub mint_node: String,
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
}

#[wasm_bindgen]
impl RequestToMintBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(mint_node: String, bill_id: String) -> Self {
        Self { mint_node, bill_id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferToSellBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub buyer: String,
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
}

#[wasm_bindgen]
impl OfferToSellBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(buyer: String, bill_id: String, sum: String, currency: String) -> Self {
        Self {
            buyer,
            bill_id,
            sum,
            currency,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestToPayBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
}

#[wasm_bindgen]
impl RequestToPayBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String, currency: String) -> Self {
        Self { bill_id, currency }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestRecourseForPaymentPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub recoursee: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
}

#[wasm_bindgen]
impl RequestRecourseForPaymentPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String, recoursee: String, currency: String, sum: String) -> Self {
        Self {
            bill_id,
            recoursee,
            currency,
            sum,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestRecourseForAcceptancePayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub recoursee: String,
}

#[wasm_bindgen]
impl RequestRecourseForAcceptancePayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String, recoursee: String) -> Self {
        Self { bill_id, recoursee }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
}

#[wasm_bindgen]
impl AcceptBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String) -> Self {
        Self { bill_id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestToAcceptBitcreditBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
}

#[wasm_bindgen]
impl RequestToAcceptBitcreditBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String) -> Self {
        Self { bill_id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectActionBillPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
}

#[wasm_bindgen]
impl RejectActionBillPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(bill_id: String) -> Self {
        Self { bill_id }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillCombinedBitcoinKeyWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub private_key: String,
}

#[wasm_bindgen]
impl BillCombinedBitcoinKeyWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(private_key: String) -> Self {
        Self { private_key }
    }
}

impl IntoWeb<BillCombinedBitcoinKeyWeb> for BillCombinedBitcoinKey {
    fn into_web(self) -> BillCombinedBitcoinKeyWeb {
        BillCombinedBitcoinKeyWeb {
            private_key: self.private_key,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct PastEndorseeWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub pay_to_the_order_of: LightIdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub signed: LightSignedByWeb,
    pub signing_timestamp: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub signing_address: PostalAddressWeb,
}

#[wasm_bindgen]
impl PastEndorseeWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        pay_to_the_order_of: LightIdentityPublicDataWeb,
        signed: LightSignedByWeb,
        signing_timestamp: u64,
        signing_address: PostalAddressWeb,
    ) -> Self {
        Self {
            pay_to_the_order_of,
            signed,
            signing_timestamp,
            signing_address,
        }
    }
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

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct LightSignedByWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub data: LightIdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub signatory: Option<LightIdentityPublicDataWeb>,
}

#[wasm_bindgen]
impl LightSignedByWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        data: LightIdentityPublicDataWeb,
        signatory: Option<LightIdentityPublicDataWeb>,
    ) -> Self {
        Self { data, signatory }
    }
}

impl IntoWeb<LightSignedByWeb> for LightSignedBy {
    fn into_web(self) -> LightSignedByWeb {
        LightSignedByWeb {
            data: self.data.into_web(),
            signatory: self.signatory.map(|s| s.into_web()),
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct EndorsementWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub pay_to_the_order_of: LightIdentityPublicDataWithAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub signed: LightSignedByWeb,
    pub signing_timestamp: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub signing_address: PostalAddressWeb,
}

#[wasm_bindgen]
impl EndorsementWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        pay_to_the_order_of: LightIdentityPublicDataWithAddressWeb,
        signed: LightSignedByWeb,
        signing_timestamp: u64,
        signing_address: PostalAddressWeb,
    ) -> Self {
        Self {
            pay_to_the_order_of,
            signed,
            signing_timestamp,
            signing_address,
        }
    }
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

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillsSearchFilterPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub filter: BillsSearchFilter,
}

#[wasm_bindgen]
impl BillsSearchFilterPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(filter: BillsSearchFilter) -> Self {
        Self { filter }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    #[wasm_bindgen(getter_with_clone)]
    pub from: String,
    #[wasm_bindgen(getter_with_clone)]
    pub to: String,
}

#[wasm_bindgen]
impl DateRange {
    #[wasm_bindgen(constructor)]
    pub fn new(from: String, to: String) -> Self {
        Self { from, to }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillsSearchFilter {
    #[wasm_bindgen(getter_with_clone)]
    pub search_term: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub date_range: Option<DateRange>,
    #[wasm_bindgen(getter_with_clone)]
    pub role: BillsFilterRoleWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct BillsResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub bills: Vec<BitcreditBillWeb>,
}

#[wasm_bindgen]
impl BillsResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(bills: Vec<BitcreditBillWeb>) -> Self {
        Self { bills }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct LightBillsResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub bills: Vec<LightBitcreditBillWeb>,
}

#[wasm_bindgen]
impl LightBillsResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(bills: Vec<LightBitcreditBillWeb>) -> Self {
        Self { bills }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct EndorsementsResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub endorsements: Vec<EndorsementWeb>,
}

#[wasm_bindgen]
impl EndorsementsResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(endorsements: Vec<EndorsementWeb>) -> Self {
        Self { endorsements }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub struct PastEndorseesResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub past_endorsees: Vec<PastEndorseeWeb>,
}

#[wasm_bindgen]
impl PastEndorseesResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(past_endorsees: Vec<PastEndorseeWeb>) -> Self {
        Self { past_endorsees }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BitcreditEbillQuote {
    #[wasm_bindgen(getter_with_clone)]
    pub bill_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub quote_id: String,
    pub sum: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub mint_node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub mint_url: String,
    pub accepted: bool,
    #[wasm_bindgen(getter_with_clone)]
    pub token: String,
}

#[wasm_bindgen]
impl BitcreditEbillQuote {
    #[wasm_bindgen(constructor)]
    pub fn new(
        bill_id: String,
        quote_id: String,
        sum: u64,
        mint_node_id: String,
        mint_url: String,
        accepted: bool,
        token: String,
    ) -> Self {
        Self {
            bill_id,
            quote_id,
            sum,
            mint_node_id,
            mint_url,
            accepted,
            token,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BitcreditBillWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_issuing: String,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_issuing: String,
    #[wasm_bindgen(getter_with_clone)]
    pub drawee: IdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub drawer: IdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub payee: IdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub endorsee: Option<IdentityPublicDataWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
    #[wasm_bindgen(getter_with_clone)]
    pub maturity_date: String,
    #[wasm_bindgen(getter_with_clone)]
    pub issue_date: String,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_payment: String,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_payment: String,
    #[wasm_bindgen(getter_with_clone)]
    pub language: String,
    pub accepted: bool,
    pub endorsed: bool,
    pub requested_to_pay: bool,
    pub requested_to_accept: bool,
    pub paid: bool,
    pub waiting_for_payment: bool,
    #[wasm_bindgen(getter_with_clone)]
    pub buyer: Option<IdentityPublicDataWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub seller: Option<IdentityPublicDataWeb>,
    pub in_recourse: bool,
    #[wasm_bindgen(getter_with_clone)]
    pub recourser: Option<IdentityPublicDataWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub recoursee: Option<IdentityPublicDataWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub link_for_buy: String,
    #[wasm_bindgen(getter_with_clone)]
    pub link_to_pay: String,
    #[wasm_bindgen(getter_with_clone)]
    pub link_to_pay_recourse: String,
    #[wasm_bindgen(getter_with_clone)]
    pub address_to_pay: String,
    #[wasm_bindgen(getter_with_clone)]
    pub mempool_link_for_address_to_pay: String,
    #[wasm_bindgen(getter_with_clone)]
    pub files: Vec<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub active_notification: Option<NotificationWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub bill_participants: Vec<String>,
    pub endorsements_count: u64,
}

#[wasm_bindgen]
impl BitcreditBillWeb {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        time_of_drawing: u64,
        time_of_maturity: u64,
        country_of_issuing: String,
        city_of_issuing: String,
        drawee: IdentityPublicDataWeb,
        drawer: IdentityPublicDataWeb,
        payee: IdentityPublicDataWeb,
        endorsee: Option<IdentityPublicDataWeb>,
        currency: String,
        sum: String,
        maturity_date: String,
        issue_date: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        accepted: bool,
        endorsed: bool,
        requested_to_pay: bool,
        requested_to_accept: bool,
        paid: bool,
        waiting_for_payment: bool,
        buyer: Option<IdentityPublicDataWeb>,
        seller: Option<IdentityPublicDataWeb>,
        in_recourse: bool,
        recourser: Option<IdentityPublicDataWeb>,
        recoursee: Option<IdentityPublicDataWeb>,
        link_for_buy: String,
        link_to_pay: String,
        link_to_pay_recourse: String,
        address_to_pay: String,
        mempool_link_for_address_to_pay: String,
        files: Vec<FileWeb>,
        active_notification: Option<NotificationWeb>,
        bill_participants: Vec<String>,
        endorsements_count: u64,
    ) -> Self {
        Self {
            id,
            time_of_drawing,
            time_of_maturity,
            country_of_issuing,
            city_of_issuing,
            drawee,
            drawer,
            payee,
            endorsee,
            currency,
            sum,
            maturity_date,
            issue_date,
            country_of_payment,
            city_of_payment,
            language,
            accepted,
            endorsed,
            requested_to_pay,
            requested_to_accept,
            paid,
            waiting_for_payment,
            buyer,
            seller,
            in_recourse,
            recourser,
            recoursee,
            link_for_buy,
            link_to_pay,
            link_to_pay_recourse,
            address_to_pay,
            mempool_link_for_address_to_pay,
            files,
            active_notification,
            bill_participants,
            endorsements_count,
        }
    }
}

impl IntoWeb<BitcreditBillWeb> for BitcreditBillResult {
    fn into_web(self) -> BitcreditBillWeb {
        BitcreditBillWeb {
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
            country_of_issuing: self.country_of_issuing,
            city_of_issuing: self.city_of_issuing,
            maturity_date: self.maturity_date,
            country_of_payment: self.country_of_payment,
            city_of_payment: self.city_of_payment,
            language: self.language,
            accepted: self.accepted,
            endorsed: self.endorsed,
            requested_to_pay: self.requested_to_pay,
            requested_to_accept: self.requested_to_accept,
            paid: self.paid,
            waiting_for_payment: self.waiting_for_payment,
            buyer: self.buyer.map(|b| b.into_web()),
            seller: self.seller.map(|b| b.into_web()),
            in_recourse: self.in_recourse,
            recourser: self.recourser.map(|r| r.into_web()),
            recoursee: self.recoursee.map(|r| r.into_web()),
            link_for_buy: self.link_for_buy,
            link_to_pay: self.link_to_pay,
            link_to_pay_recourse: self.link_to_pay_recourse,
            address_to_pay: self.address_to_pay,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
            files: self.files.into_iter().map(|f| f.into_web()).collect(),
            bill_participants: self.bill_participants,
            endorsements_count: self.endorsements_count,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LightBitcreditBillWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub drawee: LightIdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub drawer: LightIdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub payee: LightIdentityPublicDataWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub endorsee: Option<LightIdentityPublicDataWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub active_notification: Option<NotificationWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub sum: String,
    #[wasm_bindgen(getter_with_clone)]
    pub currency: String,
    #[wasm_bindgen(getter_with_clone)]
    pub issue_date: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
}

#[wasm_bindgen]
impl LightBitcreditBillWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: String,
        drawee: LightIdentityPublicDataWeb,
        drawer: LightIdentityPublicDataWeb,
        payee: LightIdentityPublicDataWeb,
        endorsee: Option<LightIdentityPublicDataWeb>,
        active_notification: Option<NotificationWeb>,
        sum: String,
        currency: String,
        issue_date: String,
        time_of_drawing: u64,
        time_of_maturity: u64,
    ) -> Self {
        Self {
            id,
            drawee,
            drawer,
            payee,
            endorsee,
            active_notification,
            sum,
            currency,
            issue_date,
            time_of_drawing,
            time_of_maturity,
        }
    }
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

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityPublicDataWeb {
    pub t: ContactTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub email: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub nostr_relay: Option<String>,
}

#[wasm_bindgen]
impl IdentityPublicDataWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: ContactTypeWeb,
        node_id: String,
        name: String,
        postal_address: PostalAddressWeb,
        email: Option<String>,
        nostr_relay: Option<String>,
    ) -> Self {
        Self {
            t,
            node_id,
            name,
            postal_address,
            email,
            nostr_relay,
        }
    }
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

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LightIdentityPublicDataWithAddressWeb {
    pub t: ContactTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
}

#[wasm_bindgen]
impl LightIdentityPublicDataWithAddressWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: ContactTypeWeb,
        name: String,
        node_id: String,
        postal_address: PostalAddressWeb,
    ) -> Self {
        Self {
            t,
            name,
            node_id,
            postal_address,
        }
    }
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

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LightIdentityPublicDataWeb {
    pub t: ContactTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
}

#[wasm_bindgen]
impl LightIdentityPublicDataWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(t: ContactTypeWeb, name: String, node_id: String) -> Self {
        Self { t, name, node_id }
    }
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
