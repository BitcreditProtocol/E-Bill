use super::block::Block;
use super::calculate_hash;
use super::extract_after_phrase;
use super::Error;
use super::OperationCode;
use super::PaymentInfo;
use super::Result;
use super::WaitingForPayment;
use crate::blockchain::OperationCode::{Endorse, Mint, Sell};
use crate::constants::AMOUNT;
use crate::constants::ENDORSED_TO;
use crate::constants::SOLD_BY;
use crate::constants::SOLD_TO;
use crate::service::bill_service::BillKeys;
use crate::service::bill_service::BitcreditBill;
use crate::service::contact_service::IdentityPublicData;
use borsh::from_slice;
use borsh_derive::BorshDeserialize;
use borsh_derive::BorshSerialize;
use log::error;
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chain {
    pub blocks: Vec<Block>,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
pub struct BlockForHistory {
    id: u64,
    text: String,
    bill_name: String,
}

impl Chain {
    pub fn new(first_block: Block) -> Self {
        let blocks = vec![first_block];

        Self { blocks }
    }

    /// Transforms the whole chain to pretty-printed JSON
    pub fn to_pretty_printed_json(&self) -> Result<String> {
        let res = serde_json::to_string_pretty(&self)?;
        Ok(res)
    }

    /// Validates the integrity of the blockchain by checking the validity of each block in the chain.
    ///
    /// # Returns
    /// * `true` - If all blocks in the chain are valid.
    /// * `false` - If any block in the chain is found to be invalid.
    ///
    pub fn is_chain_valid(&self) -> bool {
        for i in 0..self.blocks.len() {
            if i == 0 {
                continue;
            }
            let first: &Block = &self.blocks[i - 1];
            let second: &Block = &self.blocks[i];
            if !is_block_valid(second, first) {
                return false;
            }
        }
        true
    }

    /// This function checks whether the provided `block` is valid by comparing it with the latest block
    /// in the current list of blocks. If the block is valid, it is added to the list and the function returns `true`.
    /// If the block is not valid, it logs an error and returns `false`.
    ///
    /// # Arguments
    /// * `block` - The `Block` to be added to the list.
    ///
    /// # Returns
    /// * `true` if the block is successfully added to the list.
    /// * `false` if the block is invalid and cannot be added.
    ///
    pub fn try_add_block(&mut self, block: Block) -> bool {
        let latest_block = self.blocks.last().expect("there is at least one block");
        if is_block_valid(&block, latest_block) {
            self.blocks.push(block);
            true
        } else {
            error!("could not add block - invalid");
            false
        }
    }
    /// Retrieves the latest (most recent) block in the blocks list.
    ///
    /// # Returns
    /// * A reference to the latest block in the blocks list.
    ///
    pub fn get_latest_block(&self) -> &Block {
        self.blocks.last().expect("there is at least one block")
    }

    /// Retrieves the first block in the blocks list.
    ///
    /// # Returns
    /// * A reference to the first block in the blocks list.
    ///
    pub fn get_first_block(&self) -> &Block {
        self.blocks.first().expect("there is at least one block")
    }

    /// Retrieves the last block with the specified operation code.
    /// # Arguments
    /// * `operation_code` - The `OperationCode` to search for in the blocks.
    ///
    /// # Returns
    /// * A reference to the last block with the specified operation code, or the first block if none is found.
    ///
    pub fn get_last_version_block_with_operation_code(
        &self,
        operation_code: OperationCode,
    ) -> &Block {
        self.blocks
            .iter()
            .filter(|block| block.operation_code == operation_code)
            .last()
            .unwrap_or_else(|| self.get_first_block())
    }

    /// Checks if there is any block with a given operation code in the current blocks list.
    ///
    /// # Arguments
    /// * `operation_code` - The `OperationCode` to search for within the blocks.
    ///
    /// # Returns
    /// * `true` if a block with the specified operation code exists in the blocks list, otherwise `false`.
    ///
    pub fn exist_block_with_operation_code(&self, operation_code: OperationCode) -> bool {
        self.blocks
            .iter()
            .any(|b| b.operation_code == operation_code)
    }

    /// Checks if the the chain has Endorse, Mint, or Sell blocks in it
    pub fn has_been_endorsed_sold_or_minted(&self) -> bool {
        self.blocks.iter().any(|block| {
            matches!(
                block.operation_code,
                OperationCode::Mint | OperationCode::Sell | OperationCode::Endorse
            )
        })
    }

    /// Checks if the the chain has Endorse, or Sell blocks in it
    pub fn has_been_endorsed_or_sold(&self) -> bool {
        self.blocks.iter().any(|block| {
            matches!(
                block.operation_code,
                OperationCode::Sell | OperationCode::Endorse
            )
        })
    }

    /// Retrieves the last version of the Bitcredit bill by decrypting and processing the relevant blocks.
    ///
    /// # Arguments
    /// * `bill_keys` - The keys for the bill.
    ///
    /// # Returns
    /// A `BitcreditBill` object containing the most recent version of the bill, including the payee, endorsee,
    /// and other associated information.
    ///
    pub fn get_last_version_bill(&self, bill_keys: &BillKeys) -> Result<BitcreditBill> {
        let first_block = self.get_first_block();
        let decrypted_bytes = first_block.get_decrypted_block_bytes(bill_keys)?;
        let bill_first_version: BitcreditBill = from_slice(&decrypted_bytes)?;

        let mut last_endorsee = IdentityPublicData {
            peer_id: "".to_string(),
            name: "".to_string(),
            company: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            rsa_public_key_pem: "".to_string(),
            nostr_npub: None,
            nostr_relay: None,
        };

        if self.blocks.len() > 1 && self.has_been_endorsed_sold_or_minted() {
            let last_version_block_endorse =
                self.get_last_version_block_with_operation_code(Endorse);
            let last_version_block_mint = self.get_last_version_block_with_operation_code(Mint);
            let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);
            let last_block = self.get_latest_block();

            // TODO: check if the last sell block is paid
            // in the future, this will come from the database, filled by a job that runs regularly
            // and checks for the paid status
            let last_sell_block_is_paid = true;

            if (last_version_block_endorse.id < last_version_block_sell.id)
                && (last_version_block_mint.id < last_version_block_sell.id)
                && ((last_block.id > last_version_block_sell.id) || last_sell_block_is_paid)
            {
                let block_data_decrypted =
                    last_version_block_sell.get_decrypted_block_data(bill_keys)?;
                let buyer: IdentityPublicData = serde_json::from_slice(&hex::decode(
                    &extract_after_phrase(&block_data_decrypted, SOLD_TO).ok_or(
                        Error::InvalidBlockdata(String::from("Sell: No buyer found")),
                    )?,
                )?)?;

                last_endorsee = buyer;
            } else if self.exist_block_with_operation_code(Endorse.clone())
                && (last_version_block_endorse.id > last_version_block_mint.id)
            {
                let block_data_decrypted =
                    last_version_block_endorse.get_decrypted_block_data(bill_keys)?;
                let endorsee: IdentityPublicData = serde_json::from_slice(&hex::decode(
                    &extract_after_phrase(&block_data_decrypted, ENDORSED_TO).ok_or(
                        Error::InvalidBlockdata(String::from("Endorse: No endorsee found")),
                    )?,
                )?)?;

                last_endorsee = endorsee;
            } else if self.exist_block_with_operation_code(Mint.clone())
                && (last_version_block_mint.id > last_version_block_endorse.id)
            {
                let block_data_decrypted =
                    last_version_block_mint.get_decrypted_block_data(bill_keys)?;
                let mint: IdentityPublicData = serde_json::from_slice(&hex::decode(
                    &extract_after_phrase(&block_data_decrypted, ENDORSED_TO)
                        .ok_or(Error::InvalidBlockdata(String::from("Mint: No mint found")))?,
                )?)?;

                last_endorsee = mint;
            }
        }

        let mut payee = bill_first_version.payee;

        if !last_endorsee.peer_id.is_empty() {
            payee = last_endorsee.clone();
        }

        Ok(BitcreditBill {
            name: bill_first_version.name,
            to_payee: bill_first_version.to_payee,
            bill_jurisdiction: bill_first_version.bill_jurisdiction,
            timestamp_at_drawing: bill_first_version.timestamp_at_drawing,
            drawee: bill_first_version.drawee,
            drawer: bill_first_version.drawer,
            payee: payee.clone(),
            endorsee: last_endorsee.clone(),
            place_of_drawing: bill_first_version.place_of_drawing,
            currency_code: bill_first_version.currency_code,
            amount_numbers: bill_first_version.amount_numbers,
            amounts_letters: bill_first_version.amounts_letters,
            maturity_date: bill_first_version.maturity_date,
            date_of_issue: bill_first_version.date_of_issue,
            compounding_interest_rate: bill_first_version.compounding_interest_rate,
            type_of_interest_calculation: bill_first_version.type_of_interest_calculation,
            place_of_payment: bill_first_version.place_of_payment,
            public_key: bill_first_version.public_key,
            private_key: bill_first_version.private_key,
            language: bill_first_version.language,
            files: bill_first_version.files,
        })
    }

    /// Checks if the last block is a sell block, if it's deadline is still active and if so,
    /// returns the buyer, seller and amount
    pub fn is_last_sell_block_waiting_for_payment(
        &self,
        bill_keys: &BillKeys,
        current_timestamp: i64,
    ) -> Result<WaitingForPayment> {
        let last_block = self.get_latest_block();
        let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);
        // we only wait for payment, if the last block is a Sell block
        if self.exist_block_with_operation_code(Sell.clone())
            && last_block.id == last_version_block_sell.id
        {
            // if the deadline is up, we're not waiting for payment anymore
            if self.check_if_payment_deadline_has_passed(current_timestamp) {
                return Ok(WaitingForPayment::No);
            }

            let block_data_decrypted =
                last_version_block_sell.get_decrypted_block_data(bill_keys)?;

            let buyer: IdentityPublicData = serde_json::from_slice(&hex::decode(
                &extract_after_phrase(&block_data_decrypted, SOLD_TO).ok_or(
                    Error::InvalidBlockdata(String::from("Sell: No buyer found")),
                )?,
            )?)?;
            let seller: IdentityPublicData = serde_json::from_slice(&hex::decode(
                &extract_after_phrase(&block_data_decrypted, SOLD_BY).ok_or(
                    Error::InvalidBlockdata(String::from("Sell: No seller found")),
                )?,
            )?)?;

            let amount: u64 = extract_after_phrase(&block_data_decrypted, AMOUNT)
                .ok_or(Error::InvalidBlockdata(String::from(
                    "Sell: No amount found",
                )))?
                .parse()
                .map_err(|_| {
                    Error::InvalidBlockdata(String::from("Sell: Amount was no valid number"))
                })?;

            Ok(WaitingForPayment::Yes(Box::new(PaymentInfo {
                buyer,
                seller,
                amount,
            })))
        } else {
            Ok(WaitingForPayment::No)
        }
    }

    /// This function checks if the payment deadline associated with the most recent sell block
    /// has passed.
    /// # Returns
    ///
    /// - `true` if the payment deadline for the last sell block has passed.
    /// - `false` if no sell block exists or the deadline has not passed.
    ///
    fn check_if_payment_deadline_has_passed(&self, current_timestamp: i64) -> bool {
        if self.exist_block_with_operation_code(Sell) {
            let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);
            let timestamp = last_version_block_sell.timestamp;

            let period: i64 = (86400 * 2) as i64; // 2 days deadline
            let difference = current_timestamp - timestamp;
            difference > period
        } else {
            false
        }
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
    pub fn get_first_version_bill(&self, bill_keys: &BillKeys) -> Result<BitcreditBill> {
        let first_block_data = &self.get_first_block();
        let decrypted_bytes = first_block_data.get_decrypted_block_bytes(bill_keys)?;
        let bill_first_version: BitcreditBill = from_slice(&decrypted_bytes)?;
        Ok(bill_first_version)
    }

    /// This function iterates over the list of blocks in the chain and returns the first block
    /// that matches the provided `id`. If no block is found with the given ID, the function
    /// returns a clone of the first block in the chain as a fallback.
    /// # Arguments
    ///
    /// * `id` - A `u64` representing the ID of the block to retrieve.
    ///
    /// # Returns
    ///
    /// * `Block` - The block corresponding to the given `id`, or the first block in the chain
    ///   if no match is found.
    ///
    pub fn get_block_by_id(&self, id: u64) -> Block {
        self.blocks
            .iter()
            .find(|b| b.id == id)
            .cloned()
            .unwrap_or_else(|| self.get_first_block().clone())
    }

    /// This function compares the latest block ID of the local chain with that
    /// of the `other_chain`. If the `other_chain` is ahead, it attempts to add missing
    /// blocks from the `other_chain` to the local chain. If the addition of a block
    /// fails or the resulting chain becomes invalid, the synchronization is aborted.
    ///
    /// # Parameters
    /// - `other_chain: Chain`  
    ///   The chain to compare and synchronize with.
    ///
    /// # Returns
    /// `bool` - whether the given chain needs to be persisted locally after this comparison
    ///
    pub fn compare_chain(&mut self, other_chain: &Chain) -> bool {
        let local_chain_last_id = self.get_latest_block().id;
        let other_chain_last_id = other_chain.get_latest_block().id;
        let mut needs_to_persist = false;

        // if it's not the same id, and the local chain is shorter
        if !(local_chain_last_id.eq(&other_chain_last_id)
            || local_chain_last_id > other_chain_last_id)
        {
            let difference_in_id = other_chain_last_id - local_chain_last_id;
            for block_id in 1..difference_in_id + 1 {
                let block = other_chain.get_block_by_id(local_chain_last_id + block_id);
                let try_add_block = self.try_add_block(block);
                if try_add_block && self.is_chain_valid() {
                    needs_to_persist = true;
                    continue;
                } else {
                    return false;
                }
            }
        }
        needs_to_persist
    }

    /// This function iterates over all the blocks in the blockchain, extracts the nodes
    /// from each block, and compiles a unique list of non-empty nodes. Duplicate nodes
    /// are ignored.
    ///
    /// # Returns
    /// `Vec<String>`:  
    /// - A vector containing the unique identifiers of nodes associated with the bill.
    ///
    pub fn get_all_nodes_from_bill(&self, bill_keys: &BillKeys) -> Result<Vec<String>> {
        let mut nodes: Vec<String> = Vec::new();

        for block in &self.blocks {
            let nodes_in_block = block.get_nodes_from_block(bill_keys)?;
            for node in nodes_in_block {
                if !node.is_empty() && !nodes.contains(&node) {
                    nodes.push(node);
                }
            }
        }
        Ok(nodes)
    }

    /// This function determines the drawer of the bill by evaluating the following conditions:
    /// 1. If the drawer's name is not empty, it directly returns the drawer.
    /// 2. If the bill is directed to the payee (`to_payee` is `true`), it assigns the payee as the drawer.
    /// 3. Otherwise, the drawee is assigned as the drawer.
    ///
    /// # Returns
    /// `IdentityPublicData`:  
    /// - The identity data of the drawer, payee, or drawee depending on the evaluated conditions.
    ///
    pub fn get_drawer(&self, bill_keys: &BillKeys) -> Result<IdentityPublicData> {
        let drawer: IdentityPublicData;
        let bill = self.get_first_version_bill(bill_keys)?;
        if !bill.drawer.name.is_empty() {
            drawer = bill.drawer.clone();
        } else if bill.to_payee {
            drawer = bill.payee.clone();
        } else {
            drawer = bill.drawee.clone();
        }
        Ok(drawer)
    }
}

/// This function performs a series of checks to ensure the integrity of the current block
/// in relation to the previous block in the blockchain. These checks include verifying
/// the hash chain, sequential IDs, hash validity, and block signature.
///
/// # Parameters
/// - `block`: A reference to the current `Block` that needs validation.
/// - `previous_block`: A reference to the previous `Block` in the chain for comparison.
///
/// # Returns
/// `bool`:
/// - `true` if the block is valid.
/// - `false` if any of the validation checks fail.
///
fn is_block_valid(block: &Block, previous_block: &Block) -> bool {
    if block.previous_hash != previous_block.hash {
        warn!("block with id: {} has wrong previous hash", block.id);
        return false;
    } else if block.id != &previous_block.id + 1 {
        warn!(
            "block with id: {} is not the next block after the latest: {}",
            block.id, previous_block.id
        );
        return false;
    } else if hex::encode(calculate_hash(
        &block.id,
        &block.bill_name,
        &block.previous_hash,
        &block.data,
        &block.timestamp,
        &block.public_key,
        &block.operation_code,
    )) != block.hash
    {
        warn!("block with id: {} has invalid hash", block.id);
        return false;
    } else if !block.verify() {
        warn!("block with id: {} has invalid signature", block.id);
        return false;
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        blockchain::{start_blockchain_for_new_bill, test::get_baseline_identity},
        tests::test::{get_bill_keys, TEST_PRIVATE_KEY, TEST_PUB_KEY},
        util::rsa,
    };
    use libp2p::PeerId;

    fn get_sell_block(peer_id: String, prevhash: String) -> Block {
        let mut buyer = IdentityPublicData::new_empty();
        buyer.peer_id = peer_id.clone();
        let mut seller = IdentityPublicData::new_empty();
        let endorser_peer_id = PeerId::random().to_string();
        seller.peer_id = endorser_peer_id.clone();
        let hashed_buyer = hex::encode(serde_json::to_vec(&buyer).unwrap());
        let hashed_seller = hex::encode(serde_json::to_vec(&seller).unwrap());

        let data = format!(
            "{}{}{}{}{}{}",
            SOLD_TO, &hashed_buyer, SOLD_BY, &hashed_seller, AMOUNT, "5000"
        );

        Block::new(
            2,
            prevhash,
            hex::encode(rsa::encrypt_bytes_with_public_key(
                data.as_bytes(),
                TEST_PUB_KEY,
            )),
            String::from("some_bill"),
            TEST_PUB_KEY.to_owned(),
            OperationCode::Sell,
            TEST_PRIVATE_KEY.to_owned(),
            1731593928,
        )
        .unwrap()
    }

    #[test]
    fn validity_check_1_block_always_valid() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();

        assert!(chain.is_chain_valid());
    }

    #[test]
    fn validity_check_2_blocks() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        assert!(chain.try_add_block(get_sell_block(
            PeerId::random().to_string(),
            chain.get_first_block().hash.clone()
        ),));
        assert!(chain.is_chain_valid());
    }

    #[test]
    fn get_last_version_bill_last_endorsee_buyer() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let keys = get_bill_keys();
        let result = chain.get_last_version_bill(&keys);
        assert!(result.is_ok());
        assert_eq!(
            result.as_ref().unwrap().endorsee.peer_id,
            peer_id_last_endorsee
        );
    }

    #[test]
    fn is_last_sell_block_waiting_for_payment_deadline_passed() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let keys = get_bill_keys();
        let result = chain.is_last_sell_block_waiting_for_payment(&keys, 1751293728); // deadline
                                                                                      // passed
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap(), &WaitingForPayment::No);
    }

    #[test]
    fn is_last_sell_block_waiting_for_payment_baseline() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let keys = get_bill_keys();
        let result = chain.is_last_sell_block_waiting_for_payment(&keys, 1731593928);

        assert!(result.is_ok());
        if let WaitingForPayment::Yes(info) = result.unwrap() {
            assert_eq!(info.amount, 5000);
            assert_eq!(info.buyer.peer_id, peer_id_last_endorsee);
        } else {
            panic!("wrong result");
        }
    }

    #[test]
    fn get_all_nodes_from_bill_baseline() {
        let mut bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();
        bill.drawer =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let keys = get_bill_keys();
        let result = chain.get_all_nodes_from_bill(&keys);

        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().len(), 3); // drawer, buyer, seller
    }

    #[test]
    fn compare_chain_no_changes() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let chain2 = chain.clone();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let result = chain.compare_chain(&chain2);

        assert!(!result);
    }

    #[test]
    fn compare_chain_changes() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let mut chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        )
        .unwrap();
        let mut chain2 = chain.clone();
        let peer_id_last_endorsee = PeerId::random().to_string();
        assert!(chain.try_add_block(get_sell_block(
            peer_id_last_endorsee.clone(),
            chain.get_first_block().hash.clone()
        ),));

        let result = chain2.compare_chain(&chain);

        assert!(result);
        assert_eq!(chain.blocks.len(), chain2.blocks.len());
    }
}
