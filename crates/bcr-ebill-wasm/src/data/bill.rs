use bcr_ebill_api::data::{
    bill::{
        BillCombinedBitcoinKey, BillsFilterRole, BitcreditBillResult, Endorsement,
        LightBitcreditBillResult, LightSignedBy, PastEndorsee,
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
    pub file_upload_id: Option<String>,
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
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    pub drawee: IdentityPublicDataWeb,
    pub drawer: IdentityPublicDataWeb,
    pub payee: IdentityPublicDataWeb,
    pub endorsee: Option<IdentityPublicDataWeb>,
    pub currency: String,
    pub sum: String,
    pub maturity_date: String,
    pub issue_date: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub accepted: bool,
    pub endorsed: bool,
    pub requested_to_pay: bool,
    pub requested_to_accept: bool,
    pub paid: bool,
    pub waiting_for_payment: bool,
    pub buyer: Option<IdentityPublicDataWeb>,
    pub seller: Option<IdentityPublicDataWeb>,
    pub in_recourse: bool,
    pub recourser: Option<IdentityPublicDataWeb>,
    pub recoursee: Option<IdentityPublicDataWeb>,
    pub link_for_buy: String,
    pub link_to_pay: String,
    pub link_to_pay_recourse: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
    pub files: Vec<FileWeb>,
    pub active_notification: Option<NotificationWeb>,
    pub bill_participants: Vec<String>,
    pub endorsements_count: u64,
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
