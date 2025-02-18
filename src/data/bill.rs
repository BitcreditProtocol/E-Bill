use super::{
    contact::{IdentityPublicData, LightIdentityPublicData, LightIdentityPublicDataWithAddress},
    notification::Notification,
    File, PostalAddress,
};
use crate::util::date::date_string_to_i64_timestamp;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
pub struct BitcreditBill {
    pub id: String,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    // The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    // The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
    pub currency: String,
    pub sum: u64,
    pub maturity_date: String,
    pub issue_date: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub files: Vec<File>,
}

#[cfg(test)]
impl BitcreditBill {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            id: "".to_string(),
            country_of_issuing: "".to_string(),
            city_of_issuing: "".to_string(),
            drawee: IdentityPublicData::new_empty(),
            drawer: IdentityPublicData::new_empty(),
            payee: IdentityPublicData::new_empty(),
            endorsee: None,
            currency: "".to_string(),
            sum: 0,
            maturity_date: "".to_string(),
            issue_date: "".to_string(),
            city_of_payment: "".to_string(),
            country_of_payment: "".to_string(),
            language: "".to_string(),
            files: vec![],
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone)]
pub struct BillKeys {
    pub private_key: String,
    pub public_key: String,
}

#[derive(Debug, Clone)]
pub enum RecourseReason {
    Accept,
    Pay(u64, String), // sum and currency
}

#[derive(Debug, Clone)]
pub struct BitcreditBillResult {
    pub id: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    /// The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    /// The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    /// The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
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
    pub buyer: Option<IdentityPublicData>,
    pub seller: Option<IdentityPublicData>,
    pub in_recourse: bool,
    pub recourser: Option<IdentityPublicData>,
    pub recoursee: Option<IdentityPublicData>,
    pub link_for_buy: String,
    pub link_to_pay: String,
    pub link_to_pay_recourse: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
    pub files: Vec<File>,
    /// The currently active notification for this bill if any
    pub active_notification: Option<Notification>,
    pub bill_participants: Vec<String>,
    pub endorsements_count: u64,
}

impl BitcreditBillResult {
    /// Returns the role of the given node_id in the bill, or None if the node_id is not a
    /// participant in the bill
    pub fn get_bill_role_for_node_id(&self, node_id: &str) -> Option<BillRole> {
        // Node id is not part of the bill
        if !self.bill_participants.iter().any(|bp| bp == node_id) {
            return None;
        }

        // Node id is the payer
        if self.drawee.node_id == *node_id {
            return Some(BillRole::Payer);
        }

        // Node id is payee / endorsee
        if self.payee.node_id == *node_id
            || self.endorsee.as_ref().map(|e| e.node_id.as_str()) == Some(node_id)
        {
            return Some(BillRole::Payee);
        }

        // Node id is part of the bill, but neither payer, nor payee - they are part of the risk
        // chain
        Some(BillRole::Contingent)
    }

    // Search in the participants for the search term
    pub fn search_bill_for_search_term(&self, search_term: &str) -> bool {
        let search_term_lc = search_term.to_lowercase();
        if self.payee.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if self.drawer.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if self.drawee.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if let Some(ref endorsee) = self.endorsee {
            if endorsee.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        if let Some(ref buyer) = self.buyer {
            if buyer.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        if let Some(ref seller) = self.seller {
            if seller.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
pub struct LightBitcreditBillResult {
    pub id: String,
    pub drawee: LightIdentityPublicData,
    pub drawer: LightIdentityPublicData,
    pub payee: LightIdentityPublicData,
    pub endorsee: Option<LightIdentityPublicData>,
    pub active_notification: Option<Notification>,
    pub sum: String,
    pub currency: String,
    pub issue_date: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
}

impl From<BitcreditBillResult> for LightBitcreditBillResult {
    fn from(value: BitcreditBillResult) -> Self {
        Self {
            id: value.id,
            drawee: value.drawee.into(),
            drawer: value.drawer.into(),
            payee: value.payee.into(),
            endorsee: value.endorsee.map(|v| v.into()),
            active_notification: value.active_notification,
            sum: value.sum,
            currency: value.currency,
            issue_date: value.issue_date,
            time_of_drawing: value.time_of_drawing,
            time_of_maturity: date_string_to_i64_timestamp(&value.maturity_date, None).unwrap_or(0)
                as u64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BillsBalanceOverview {
    pub payee: BillsBalance,
    pub payer: BillsBalance,
    pub contingent: BillsBalance,
}

#[derive(Debug, Clone)]
pub struct BillsBalance {
    pub sum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillRole {
    Payee,
    Payer,
    Contingent,
}

#[derive(Debug)]
pub struct BillCombinedBitcoinKey {
    pub private_key: String,
}

#[derive(Debug)]
pub enum BillsFilterRole {
    All,
    Payer,
    Payee,
    Contingent,
}

#[derive(Debug)]
pub struct PastEndorsee {
    pub pay_to_the_order_of: LightIdentityPublicData,
    pub signed: LightSignedBy,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress,
}

#[derive(Debug)]
pub struct Endorsement {
    pub pay_to_the_order_of: LightIdentityPublicDataWithAddress,
    pub signed: LightSignedBy,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress,
}

#[derive(Debug)]
pub struct LightSignedBy {
    pub data: LightIdentityPublicData,
    pub signatory: Option<LightIdentityPublicData>,
}
