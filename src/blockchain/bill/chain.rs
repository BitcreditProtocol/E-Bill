use super::super::Result;
use super::block::{
    BillBlock, BillIssueBlockData, BillOfferToSellBlockData, BillRequestRecourseBlockData,
};
use super::PaymentInfo;
use super::{BillOpCode, RecourseWaitingForPayment};
use super::{OfferToSellWaitingForPayment, RecoursePaymentInfo};
use crate::blockchain::{Block, Blockchain, Error};
use crate::constants::{PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS};
use crate::service::bill_service::BillKeys;
use crate::util::{self, BcrKeys};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone)]
pub struct BillBlockchain {
    blocks: Vec<BillBlock>,
}

impl Blockchain for BillBlockchain {
    type Block = BillBlock;

    fn blocks(&self) -> &Vec<Self::Block> {
        &self.blocks
    }

    fn blocks_mut(&mut self) -> &mut Vec<Self::Block> {
        &mut self.blocks
    }
}

impl BillBlockchain {
    /// Creates a new blockchain for the given bill, encrypting the metadata using the bill's public
    /// key
    pub fn new(
        bill: &BillIssueBlockData,
        drawer_key_pair: BcrKeys,
        drawer_company_key_pair: Option<BcrKeys>,
        bill_keys: BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let genesis_hash = util::base58_encode(bill.id.as_bytes());

        let first_block = BillBlock::create_block_for_issue(
            bill.id.clone(),
            genesis_hash,
            bill,
            &drawer_key_pair,
            drawer_company_key_pair.as_ref(),
            &bill_keys,
            timestamp,
        )?;

        Ok(Self {
            blocks: vec![first_block],
        })
    }

    /// Creates a bill chain from a vec of blocks
    pub fn new_from_blocks(blocks_to_add: Vec<BillBlock>) -> Result<Self> {
        match blocks_to_add.first() {
            None => Err(Error::BlockchainInvalid),
            Some(first) => {
                if !first.verify() || !first.validate_hash() {
                    return Err(Error::BlockchainInvalid);
                }

                let chain = Self {
                    blocks: blocks_to_add,
                };

                if !chain.is_chain_valid() {
                    return Err(Error::BlockchainInvalid);
                }

                Ok(chain)
            }
        }
    }

    /// Checks if the the chain has Endorse, Mint, or Sell blocks in it
    pub fn has_been_endorsed_sold_or_minted(&self) -> bool {
        self.blocks.iter().any(|block| {
            matches!(
                block.op_code,
                BillOpCode::Mint | BillOpCode::Sell | BillOpCode::Endorse
            )
        })
    }

    /// Counts the number of endorsement blocks (mint, sell, endorse, recourse)
    pub fn get_endorsements_count(&self) -> u64 {
        self.blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.op_code,
                    BillOpCode::Mint
                        | BillOpCode::Sell
                        | BillOpCode::Endorse
                        | BillOpCode::Recourse
                )
            })
            .count() as u64
    }

    /// Checks if the the chain has Endorse, or Sell blocks in it
    pub fn has_been_endorsed_or_sold(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block.op_code, BillOpCode::Sell | BillOpCode::Endorse))
    }

    /// Checks if the last block is a request to recourse block, if it's deadline is still active and if so,
    /// returns the recoursee, recourser and sum
    pub fn is_last_request_to_recourse_block_waiting_for_payment(
        &self,
        bill_keys: &BillKeys,
        current_timestamp: u64,
    ) -> Result<RecourseWaitingForPayment> {
        let last_block = self.get_latest_block();
        if let Some(last_version_block) =
            self.get_last_version_block_with_op_code(BillOpCode::RequestRecourse)
        {
            // we only wait for payment, if the last block is a Request to Recourse block
            if last_block.id == last_version_block.id {
                // if the deadline is up, we're not waiting for payment anymore
                if self.check_if_payment_deadline_has_passed(
                    last_version_block.timestamp,
                    current_timestamp,
                    RECOURSE_DEADLINE_SECONDS,
                ) {
                    return Ok(RecourseWaitingForPayment::No);
                }

                let block_data_decrypted: BillRequestRecourseBlockData =
                    last_version_block.get_decrypted_block_bytes(bill_keys)?;
                return Ok(RecourseWaitingForPayment::Yes(Box::new(
                    RecoursePaymentInfo {
                        recoursee: block_data_decrypted.recoursee,
                        recourser: block_data_decrypted.recourser,
                        sum: block_data_decrypted.sum,
                        currency: block_data_decrypted.currency,
                    },
                )));
            }
        }
        Ok(RecourseWaitingForPayment::No)
    }

    /// Checks if the last block is an offer to sell block, if it's deadline is still active and if so,
    /// returns the buyer, seller and sum
    pub fn is_last_offer_to_sell_block_waiting_for_payment(
        &self,
        bill_keys: &BillKeys,
        current_timestamp: u64,
    ) -> Result<OfferToSellWaitingForPayment> {
        let last_block = self.get_latest_block();
        if let Some(last_version_block_offer_to_sell) =
            self.get_last_version_block_with_op_code(BillOpCode::OfferToSell)
        {
            // we only wait for payment, if the last block is an Offer to Sell block
            if last_block.id == last_version_block_offer_to_sell.id {
                // if the deadline is up, we're not waiting for payment anymore
                if self.check_if_payment_deadline_has_passed(
                    last_version_block_offer_to_sell.timestamp,
                    current_timestamp,
                    PAYMENT_DEADLINE_SECONDS,
                ) {
                    return Ok(OfferToSellWaitingForPayment::No);
                }

                let block_data_decrypted: BillOfferToSellBlockData =
                    last_version_block_offer_to_sell.get_decrypted_block_bytes(bill_keys)?;
                return Ok(OfferToSellWaitingForPayment::Yes(Box::new(PaymentInfo {
                    buyer: block_data_decrypted.buyer,
                    seller: block_data_decrypted.seller,
                    sum: block_data_decrypted.sum,
                    currency: block_data_decrypted.currency,
                    payment_address: block_data_decrypted.payment_address,
                })));
            }
        }
        Ok(OfferToSellWaitingForPayment::No)
    }

    /// This function checks if the payment deadline associated with the most recent sell block
    /// has passed.
    ///
    /// # Returns
    ///
    /// - `true` if the payment deadline for the last sell block has passed.
    /// - `false` if the deadline has not passed.
    ///
    fn check_if_payment_deadline_has_passed(
        &self,
        block_timestamp: u64,
        current_timestamp: u64,
        deadline_seconds: u64,
    ) -> bool {
        // We check this to avoid a u64 underflow, if the block timestamp is in the future, the
        // deadline can't be expired
        if block_timestamp > current_timestamp {
            return false;
        }
        let difference = current_timestamp - block_timestamp;
        difference > deadline_seconds
    }

    /// This function extracts the first block's data, decrypts it using the private key
    /// associated with the bill, and then deserializes the decrypted data into a `BitcreditBill`
    /// object.
    ///
    /// # Arguments
    /// * `bill_keys` - The keys for the bill.
    ///
    /// # Returns
    ///
    /// * `BitcreditBill` - The first version of the bill
    ///
    pub fn get_first_version_bill(&self, bill_keys: &BillKeys) -> Result<BillIssueBlockData> {
        let first_block_data = &self.get_first_block();
        let bill_first_version: BillIssueBlockData =
            first_block_data.get_decrypted_block_bytes(bill_keys)?;
        Ok(bill_first_version)
    }

    /// This function iterates over all the blocks in the blockchain, extracts the nodes
    /// from each block, and compiles a unique list of nodes.
    ///
    /// # Returns
    /// `Vec<String>`:
    /// - A vector containing the unique identifiers of nodes associated with the bill.
    ///
    pub fn get_all_nodes_from_bill(&self, bill_keys: &BillKeys) -> Result<Vec<String>> {
        let mut nodes: HashSet<String> = HashSet::new();

        for block in &self.blocks {
            let nodes_in_block = block.get_nodes_from_block(bill_keys)?;
            for node in nodes_in_block {
                nodes.insert(node);
            }
        }
        Ok(nodes.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::bill::{block::BillOfferToSellBlockData, tests::get_baseline_identity},
        service::{bill_service::BitcreditBill, contact_service::IdentityPublicData},
        tests::tests::{get_bill_keys, TEST_PRIVATE_KEY_SECP},
    };

    fn get_offer_to_sell_block(
        buyer_node_id: String,
        seller_node_id: String,
        previous_block: &BillBlock,
    ) -> BillBlock {
        let buyer = IdentityPublicData::new_only_node_id(buyer_node_id);
        let seller = IdentityPublicData::new_only_node_id(seller_node_id);

        BillBlock::create_block_for_offer_to_sell(
            "some id".to_string(),
            previous_block,
            &BillOfferToSellBlockData {
                buyer: buyer.clone().into(),
                seller: seller.clone().into(),
                sum: 5000,
                currency: "sat".to_string(),
                payment_address: "1234".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: seller.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap()
    }

    #[test]
    fn validity_check_1_block_always_valid() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();

        assert!(chain.is_chain_valid());
    }

    #[test]
    fn validity_check_2_blocks() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            BcrKeys::new().get_public_key(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));
        assert!(chain.is_chain_valid());
    }

    #[test]
    fn is_last_sell_block_waiting_for_payment_deadline_passed() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let node_id_last_endorsee = BcrKeys::new().get_public_key();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            node_id_last_endorsee.clone(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));

        let keys = get_bill_keys();
        let result = chain.is_last_offer_to_sell_block_waiting_for_payment(&keys, 1751293728); // deadline
                                                                                               // passed
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap(), &OfferToSellWaitingForPayment::No);
    }

    #[test]
    fn is_last_sell_block_waiting_for_payment_baseline() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let node_id_last_endorsee = BcrKeys::new().get_public_key();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            node_id_last_endorsee.clone(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));

        let keys = get_bill_keys();
        let result = chain.is_last_offer_to_sell_block_waiting_for_payment(&keys, 1731593928);

        assert!(result.is_ok());
        if let OfferToSellWaitingForPayment::Yes(info) = result.unwrap() {
            assert_eq!(info.sum, 5000);
            assert_eq!(info.buyer.node_id, node_id_last_endorsee);
        } else {
            panic!("wrong result");
        }
    }

    #[test]
    fn get_all_nodes_from_bill_baseline() {
        let mut bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();
        bill.drawer = IdentityPublicData::new(identity.identity.clone()).unwrap();
        bill.drawee = IdentityPublicData::new(identity.identity.clone()).unwrap();
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let node_id_last_endorsee = BcrKeys::new().get_public_key();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            node_id_last_endorsee.clone(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));

        let keys = get_bill_keys();
        let result = chain.get_all_nodes_from_bill(&keys);

        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().len(), 3); // drawer, buyer, seller
    }

    #[test]
    fn get_blocks_to_add_from_other_chain_no_changes() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let chain2 = chain.clone();
        let node_id_last_endorsee = BcrKeys::new().get_public_key();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            node_id_last_endorsee.clone(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));

        let result = chain.get_blocks_to_add_from_other_chain(&chain2);

        assert!(result.is_empty());
    }

    #[test]
    fn get_blocks_to_add_from_other_chain_changes() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            identity.key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let mut chain2 = chain.clone();
        let node_id_last_endorsee = BcrKeys::new().get_public_key();
        assert!(chain.try_add_block(get_offer_to_sell_block(
            node_id_last_endorsee.clone(),
            identity.identity.node_id,
            chain.get_first_block()
        ),));

        let result = chain2.get_blocks_to_add_from_other_chain(&chain);

        assert!(!result.is_empty());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 2);
    }
}
