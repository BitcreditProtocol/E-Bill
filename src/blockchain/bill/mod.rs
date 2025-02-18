use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub mod block;
pub mod chain;

pub use block::BillBlock;
use block::BillIdentityBlockData;
pub use chain::BillBlockchain;

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash,
)]
pub enum BillOpCode {
    Issue,
    Accept,
    Endorse,
    RequestToAccept,
    RequestToPay,
    OfferToSell,
    Sell,
    Mint,
    RejectToAccept,
    RejectToPay,
    RejectToBuy,
    RejectToPayRecourse,
    RequestRecourse,
    Recourse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfferToSellWaitingForPayment {
    Yes(Box<PaymentInfo>),
    No,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecourseWaitingForPayment {
    Yes(Box<RecoursePaymentInfo>),
    No,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentInfo {
    pub buyer: BillIdentityBlockData,
    pub seller: BillIdentityBlockData,
    pub sum: u64,
    pub currency: String,
    pub payment_address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoursePaymentInfo {
    pub recourser: BillIdentityBlockData,
    pub recoursee: BillIdentityBlockData,
    pub sum: u64,
    pub currency: String,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        blockchain::Blockchain,
        data::{
            bill::BitcreditBill,
            identity::{Identity, IdentityWithAll},
        },
        tests::tests::TEST_PRIVATE_KEY_SECP,
        util::BcrKeys,
    };
    use block::BillIssueBlockData;

    pub fn get_baseline_identity() -> IdentityWithAll {
        let keys = BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap();
        let mut identity = Identity::new_empty();
        identity.node_id = keys.get_public_key();
        identity.name = "drawer".to_owned();
        identity.postal_address.country = Some("AT".to_owned());
        identity.postal_address.city = Some("Vienna".to_owned());
        identity.postal_address.address = Some("Hayekweg 5".to_owned());
        IdentityWithAll {
            identity,
            key_pair: keys,
        }
    }

    #[test]
    fn start_blockchain_for_new_bill_baseline() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let result = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        );

        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().blocks().len(), 1);
    }
}
