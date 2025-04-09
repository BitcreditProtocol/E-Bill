use bcr_ebill_api::data::{
    bill::{
        BillAcceptanceStatus, BillCombinedBitcoinKey, BillCurrentWaitingState, BillData,
        BillParticipants, BillPaymentStatus, BillRecourseStatus, BillSellStatus, BillStatus,
        BillWaitingForPaymentState, BillWaitingForRecourseState, BillWaitingForSellState,
        BillsFilterRole, BitcreditBillResult, Endorsement, LightBitcreditBillResult, LightSignedBy,
        PastEndorsee, PastPaymentDataPayment, PastPaymentDataRecourse, PastPaymentDataSell,
        PastPaymentResult, PastPaymentStatus,
    },
    contact::{IdentityPublicData, LightIdentityPublicData, LightIdentityPublicDataWithAddress},
};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use super::{
    FileWeb, FromWeb, IntoWeb, PostalAddressWeb, contact::ContactTypeWeb,
    notification::NotificationWeb,
};

#[derive(Tsify, Debug, Serialize)]
#[tsify(into_wasm_abi)]
pub struct BillId {
    pub id: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct BitcreditBillPayload {
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

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct BillNumbersToWordsForSum {
    pub sum: u64,
    pub sum_as_words: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct EndorseBitcreditBillPayload {
    pub endorsee: String,
    pub bill_id: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct MintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Tsify, Debug, Deserialize, Clone)]
#[tsify(from_wasm_abi)]
pub struct RequestToMintBitcreditBillPayload {
    pub mint_node: String,
    pub bill_id: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct OfferToSellBitcreditBillPayload {
    pub buyer: String,
    pub bill_id: String,
    pub sum: String,
    pub currency: String,
}

#[derive(Tsify, Debug, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct RequestToPayBitcreditBillPayload {
    pub bill_id: String,
    pub currency: String,
}

#[derive(Tsify, Debug, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct RequestRecourseForPaymentPayload {
    pub bill_id: String,
    pub recoursee: String,
    pub currency: String,
    pub sum: String,
}

#[derive(Tsify, Debug, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct RequestRecourseForAcceptancePayload {
    pub bill_id: String,
    pub recoursee: String,
}

#[derive(Tsify, Debug, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct AcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct RequestToAcceptBitcreditBillPayload {
    pub bill_id: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct RejectActionBillPayload {
    pub bill_id: String,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Clone, Copy, Deserialize)]
#[tsify(from_wasm_abi)]
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

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct LightSignedByWeb {
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

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct BillsSearchFilterPayload {
    pub filter: BillsSearchFilter,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

#[derive(Tsify, Debug, Clone, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct BillsSearchFilter {
    pub search_term: Option<String>,
    pub date_range: Option<DateRange>,
    pub role: BillsFilterRoleWeb,
    pub currency: String,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct BillsResponse {
    pub bills: Vec<BitcreditBillWeb>,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct LightBillsResponse {
    pub bills: Vec<LightBitcreditBillWeb>,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct EndorsementsResponse {
    pub endorsements: Vec<EndorsementWeb>,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct PastEndorseesResponse {
    pub past_endorsees: Vec<PastEndorseeWeb>,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub struct PastPaymentsResponse {
    pub past_payments: Vec<PastPaymentResultWeb>,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub enum PastPaymentResultWeb {
    Sell(PastPaymentDataSellWeb),
    Payment(PastPaymentDataPaymentWeb),
    Recourse(PastPaymentDataRecourseWeb),
}

impl IntoWeb<PastPaymentResultWeb> for PastPaymentResult {
    fn into_web(self) -> PastPaymentResultWeb {
        match self {
            PastPaymentResult::Sell(state) => PastPaymentResultWeb::Sell(state.into_web()),
            PastPaymentResult::Payment(state) => PastPaymentResultWeb::Payment(state.into_web()),
            PastPaymentResult::Recourse(state) => PastPaymentResultWeb::Recourse(state.into_web()),
        }
    }
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
pub enum PastPaymentStatusWeb {
    Paid(u64),
    Rejected(u64),
    Expired(u64),
}

impl IntoWeb<PastPaymentStatusWeb> for PastPaymentStatus {
    fn into_web(self) -> PastPaymentStatusWeb {
        match self {
            PastPaymentStatus::Paid(ts) => PastPaymentStatusWeb::Paid(ts),
            PastPaymentStatus::Rejected(ts) => PastPaymentStatusWeb::Rejected(ts),
            PastPaymentStatus::Expired(ts) => PastPaymentStatusWeb::Expired(ts),
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct PastPaymentDataSellWeb {
    pub time_of_request: u64,
    pub buyer: IdentityPublicDataWeb,
    pub seller: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub private_key_to_spend: String,
    pub mempool_link_for_address_to_pay: String,
    pub status: PastPaymentStatusWeb,
}

impl IntoWeb<PastPaymentDataSellWeb> for PastPaymentDataSell {
    fn into_web(self) -> PastPaymentDataSellWeb {
        PastPaymentDataSellWeb {
            time_of_request: self.time_of_request,
            buyer: self.buyer.into_web(),
            seller: self.seller.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            private_key_to_spend: self.private_key_to_spend,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
            status: self.status.into_web(),
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct PastPaymentDataPaymentWeb {
    pub time_of_request: u64,
    pub payer: IdentityPublicDataWeb,
    pub payee: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub private_key_to_spend: String,
    pub mempool_link_for_address_to_pay: String,
    pub status: PastPaymentStatusWeb,
}
impl IntoWeb<PastPaymentDataPaymentWeb> for PastPaymentDataPayment {
    fn into_web(self) -> PastPaymentDataPaymentWeb {
        PastPaymentDataPaymentWeb {
            time_of_request: self.time_of_request,
            payer: self.payer.into_web(),
            payee: self.payee.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            private_key_to_spend: self.private_key_to_spend,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
            status: self.status.into_web(),
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct PastPaymentDataRecourseWeb {
    pub time_of_request: u64,
    pub recourser: IdentityPublicDataWeb,
    pub recoursee: IdentityPublicDataWeb,
    pub currency: String,
    pub sum: String,
    pub link_to_pay: String,
    pub address_to_pay: String,
    pub private_key_to_spend: String,
    pub mempool_link_for_address_to_pay: String,
    pub status: PastPaymentStatusWeb,
}

impl IntoWeb<PastPaymentDataRecourseWeb> for PastPaymentDataRecourse {
    fn into_web(self) -> PastPaymentDataRecourseWeb {
        PastPaymentDataRecourseWeb {
            time_of_request: self.time_of_request,
            recourser: self.recourser.into_web(),
            recoursee: self.recoursee.into_web(),
            currency: self.currency,
            sum: self.sum,
            link_to_pay: self.link_to_pay,
            address_to_pay: self.address_to_pay,
            private_key_to_spend: self.private_key_to_spend,
            mempool_link_for_address_to_pay: self.mempool_link_for_address_to_pay,
            status: self.status.into_web(),
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BitcreditEbillQuote {
    pub bill_id: String,
    pub quote_id: String,
    pub sum: u64,
    pub mint_node_id: String,
    pub mint_url: String,
    pub accepted: bool,
    pub token: String,
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BillStatusWeb {
    pub acceptance: BillAcceptanceStatusWeb,
    pub payment: BillPaymentStatusWeb,
    pub sell: BillSellStatusWeb,
    pub recourse: BillRecourseStatusWeb,
    pub redeemed_funds_available: bool,
    pub has_requested_funds: bool,
}

impl IntoWeb<BillStatusWeb> for BillStatus {
    fn into_web(self) -> BillStatusWeb {
        BillStatusWeb {
            acceptance: self.acceptance.into_web(),
            payment: self.payment.into_web(),
            sell: self.sell.into_web(),
            recourse: self.recourse.into_web(),
            redeemed_funds_available: self.redeemed_funds_available,
            has_requested_funds: self.has_requested_funds,
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BillAcceptanceStatusWeb {
    pub time_of_request_to_accept: Option<u64>,
    pub requested_to_accept: bool,
    pub accepted: bool,
    pub request_to_accept_timed_out: bool,
    pub rejected_to_accept: bool,
}

impl IntoWeb<BillAcceptanceStatusWeb> for BillAcceptanceStatus {
    fn into_web(self) -> BillAcceptanceStatusWeb {
        BillAcceptanceStatusWeb {
            time_of_request_to_accept: self.time_of_request_to_accept,
            requested_to_accept: self.requested_to_accept,
            accepted: self.accepted,
            request_to_accept_timed_out: self.request_to_accept_timed_out,
            rejected_to_accept: self.rejected_to_accept,
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BillPaymentStatusWeb {
    pub time_of_request_to_pay: Option<u64>,
    pub requested_to_pay: bool,
    pub paid: bool,
    pub request_to_pay_timed_out: bool,
    pub rejected_to_pay: bool,
}
impl IntoWeb<BillPaymentStatusWeb> for BillPaymentStatus {
    fn into_web(self) -> BillPaymentStatusWeb {
        BillPaymentStatusWeb {
            time_of_request_to_pay: self.time_of_request_to_pay,
            requested_to_pay: self.requested_to_pay,
            paid: self.paid,
            request_to_pay_timed_out: self.request_to_pay_timed_out,
            rejected_to_pay: self.rejected_to_pay,
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BillSellStatusWeb {
    pub time_of_last_offer_to_sell: Option<u64>,
    pub sold: bool,
    pub offered_to_sell: bool,
    pub offer_to_sell_timed_out: bool,
    pub rejected_offer_to_sell: bool,
}
impl IntoWeb<BillSellStatusWeb> for BillSellStatus {
    fn into_web(self) -> BillSellStatusWeb {
        BillSellStatusWeb {
            time_of_last_offer_to_sell: self.time_of_last_offer_to_sell,
            sold: self.sold,
            offered_to_sell: self.offered_to_sell,
            offer_to_sell_timed_out: self.offer_to_sell_timed_out,
            rejected_offer_to_sell: self.rejected_offer_to_sell,
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct BillRecourseStatusWeb {
    pub time_of_last_request_to_recourse: Option<u64>,
    pub recoursed: bool,
    pub requested_to_recourse: bool,
    pub request_to_recourse_timed_out: bool,
    pub rejected_request_to_recourse: bool,
}

impl IntoWeb<BillRecourseStatusWeb> for BillRecourseStatus {
    fn into_web(self) -> BillRecourseStatusWeb {
        BillRecourseStatusWeb {
            time_of_last_request_to_recourse: self.time_of_last_request_to_recourse,
            recoursed: self.recoursed,
            requested_to_recourse: self.requested_to_recourse,
            request_to_recourse_timed_out: self.request_to_recourse_timed_out,
            rejected_request_to_recourse: self.rejected_request_to_recourse,
        }
    }
}

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct IdentityPublicDataWeb {
    pub t: ContactTypeWeb,
    pub node_id: String,
    pub name: String,
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct LightIdentityPublicDataWithAddressWeb {
    pub t: ContactTypeWeb,
    pub name: String,
    pub node_id: String,
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

#[derive(Tsify, Debug, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct LightIdentityPublicDataWeb {
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
