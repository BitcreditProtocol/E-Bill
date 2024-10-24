use log::error;
use openssl::pkey::Private;
use openssl::rsa::Rsa;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::thread;

use super::block::Block;
use super::calculate_hash;
use super::OperationCode;
use crate::blockchain::OperationCode::{
    Accept, Endorse, Issue, Mint, RequestToAccept, RequestToPay, Sell,
};
use crate::constants::{BILLS_FOLDER_PATH, USEDNET};
use crate::external;
use crate::{
    bill::{
        bill_from_byte_array, contacts::IdentityPublicData, read_keys_from_bill_file, BitcreditBill,
    },
    util::rsa::decrypt_bytes,
};
use borsh_derive::BorshDeserialize;
use borsh_derive::BorshSerialize;
use log::warn;
use rocket::FromForm;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chain {
    pub blocks: Vec<Block>,
}

#[derive(BorshSerialize, BorshDeserialize, FromForm, Debug, Serialize, Deserialize, Clone)]
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

    pub fn read_chain_from_file(bill_name: &str) -> Self {
        let input_path = BILLS_FOLDER_PATH.to_string() + "/" + bill_name + ".json";
        let blockchain_from_file = std::fs::read(input_path.clone()).expect("file not found");
        serde_json::from_slice(blockchain_from_file.as_slice()).unwrap()
    }

    pub fn write_chain_to_file(&self, bill_name: &str) {
        let output_path = BILLS_FOLDER_PATH.to_string() + "/" + bill_name + ".json";
        std::fs::write(
            output_path.clone(),
            serde_json::to_string_pretty(&self).unwrap(),
        )
        .unwrap();
    }

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

    pub fn get_latest_block(&self) -> &Block {
        self.blocks.last().expect("there is at least one block")
    }

    pub fn get_first_block(&self) -> &Block {
        self.blocks.first().expect("there is at least one block")
    }

    pub fn get_last_version_block_with_operation_code(
        &self,
        operation_code: OperationCode,
    ) -> &Block {
        let mut last_version_block: &Block = self.get_first_block();
        for block in &self.blocks {
            if block.operation_code == operation_code {
                last_version_block = block;
            }
        }
        last_version_block
    }

    pub fn exist_block_with_operation_code(&self, operation_code: OperationCode) -> bool {
        let mut exist_block_with_operation_code = false;
        for block in &self.blocks {
            if block.operation_code == operation_code {
                exist_block_with_operation_code = true;
            }
        }
        exist_block_with_operation_code
    }

    pub fn get_last_version_bill(&self) -> BitcreditBill {
        let first_block = self.get_first_block();

        let bill_keys = read_keys_from_bill_file(&first_block.bill_name);
        let key: Rsa<Private> =
            Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
        let bytes = hex::decode(first_block.data.clone()).unwrap();
        let decrypted_bytes = decrypt_bytes(&bytes, &key);
        let bill_first_version: BitcreditBill = bill_from_byte_array(&decrypted_bytes);

        let mut last_endorsee = IdentityPublicData {
            peer_id: "".to_string(),
            name: "".to_string(),
            company: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            email: "".to_string(),
            rsa_public_key_pem: "".to_string(),
        };

        if self.blocks.len() > 1
            && (self.exist_block_with_operation_code(Endorse.clone())
                || self.exist_block_with_operation_code(Sell.clone())
                || self.exist_block_with_operation_code(Mint.clone()))
        {
            let last_version_block_endorse =
                self.get_last_version_block_with_operation_code(Endorse);
            let last_version_block_mint = self.get_last_version_block_with_operation_code(Mint);
            let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);
            let last_block = self.get_latest_block();

            let paid = Self::check_if_last_sell_block_is_paid(self);

            if (last_version_block_endorse.id < last_version_block_sell.id)
                && (last_version_block_mint.id < last_version_block_sell.id)
                && ((last_block.id > last_version_block_sell.id) || paid)
            {
                let bill_keys = read_keys_from_bill_file(&last_version_block_sell.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(last_version_block_sell.data.clone()).unwrap();
                let decrypted_bytes = decrypt_bytes(&bytes, &key);
                let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                let part_without_sold_to = block_data_decrypted
                    .split("Sold to ")
                    .collect::<Vec<&str>>()
                    .get(1)
                    .unwrap()
                    .to_string();

                let part_with_buyer = part_without_sold_to
                    .split(" sold by ")
                    .collect::<Vec<&str>>()
                    .first()
                    .unwrap()
                    .to_string();

                let buyer_bill_u8 = hex::decode(part_with_buyer).unwrap();
                let buyer_bill: IdentityPublicData =
                    serde_json::from_slice(&buyer_bill_u8).unwrap();

                last_endorsee = buyer_bill.clone();
            } else if self.exist_block_with_operation_code(Endorse.clone())
                && (last_version_block_endorse.id > last_version_block_mint.id)
            {
                let bill_keys = read_keys_from_bill_file(&last_version_block_endorse.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(last_version_block_endorse.data.clone()).unwrap();
                let decrypted_bytes = decrypt_bytes(&bytes, &key);
                let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                let mut part_with_endorsee = block_data_decrypted
                    .split("Endorsed to ")
                    .collect::<Vec<&str>>()
                    .get(1)
                    .unwrap()
                    .to_string();

                part_with_endorsee = part_with_endorsee
                    .split(" endorsed by ")
                    .collect::<Vec<&str>>()
                    .first()
                    .unwrap()
                    .to_string();

                let endorsee = hex::decode(part_with_endorsee).unwrap();
                last_endorsee = serde_json::from_slice(&endorsee).unwrap();
            } else if self.exist_block_with_operation_code(Mint.clone())
                && (last_version_block_mint.id > last_version_block_endorse.id)
            {
                let bill_keys = read_keys_from_bill_file(&last_version_block_mint.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(last_version_block_mint.data.clone()).unwrap();
                let decrypted_bytes = decrypt_bytes(&bytes, &key);
                let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                let mut part_with_mint = block_data_decrypted
                    .split("Endorsed to ")
                    .collect::<Vec<&str>>()
                    .get(1)
                    .unwrap()
                    .to_string();

                part_with_mint = part_with_mint
                    .split(" endorsed by ")
                    .collect::<Vec<&str>>()
                    .first()
                    .unwrap()
                    .to_string();

                let mint = hex::decode(part_with_mint).unwrap();
                last_endorsee = serde_json::from_slice(&mint).unwrap();
            }
        }

        let mut payee = bill_first_version.payee.clone();

        if !last_endorsee.peer_id.is_empty() {
            payee = last_endorsee.clone();
        }

        BitcreditBill {
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
        }
    }

    pub fn waiting_for_payment(
        &self,
    ) -> (bool, IdentityPublicData, IdentityPublicData, String, u64) {
        let last_block = self.get_latest_block();
        let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);
        let identity_buyer = IdentityPublicData::new_empty();
        let identity_seller = IdentityPublicData::new_empty();

        if self.exist_block_with_operation_code(Sell.clone())
            && last_block.id == last_version_block_sell.id
        {
            let bill_keys = read_keys_from_bill_file(&last_version_block_sell.bill_name);
            let key: Rsa<Private> =
                Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
            let bytes = hex::decode(last_version_block_sell.data.clone()).unwrap();
            let decrypted_bytes = decrypt_bytes(&bytes, &key);
            let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

            let part_without_sold_to = block_data_decrypted
                .split("Sold to ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string();

            let part_with_buyer = part_without_sold_to
                .split(" sold by ")
                .collect::<Vec<&str>>()
                .first()
                .unwrap()
                .to_string();

            let part_with_seller_and_amount = part_without_sold_to
                .clone()
                .split(" sold by ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string();

            let amount: u64 = part_with_seller_and_amount
                .clone()
                .split(" amount: ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string()
                .parse()
                .unwrap();

            let part_with_seller = part_with_seller_and_amount
                .clone()
                .split(" amount: ")
                .collect::<Vec<&str>>()
                .first()
                .unwrap()
                .to_string();

            let buyer_bill_u8 = hex::decode(part_with_buyer).unwrap();
            let buyer_bill: IdentityPublicData = serde_json::from_slice(&buyer_bill_u8).unwrap();
            let identity_buyer = buyer_bill;

            let seller_bill_u8 = hex::decode(part_with_seller).unwrap();
            let seller_bill: IdentityPublicData = serde_json::from_slice(&seller_bill_u8).unwrap();
            let identity_seller = seller_bill;

            let bill = self.get_first_version_bill();

            let address_to_pay =
                Self::get_address_to_pay_for_block_sell(last_version_block_sell.clone(), bill);

            let address_to_pay_for_async = address_to_pay.clone();

            let paid = thread::spawn(move || Self::check_if_paid(address_to_pay_for_async, amount))
                .join()
                .expect("Thread panicked");

            (
                !paid,
                identity_buyer,
                identity_seller,
                address_to_pay,
                amount,
            )
        } else {
            (false, identity_buyer, identity_seller, String::new(), 0)
        }
    }

    pub async fn check_if_payment_deadline_has_passed(&self) -> bool {
        if self.exist_block_with_operation_code(Sell) {
            let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);

            let timestamp = last_version_block_sell.timestamp;

            Self::payment_deadline_has_passed(timestamp, 2).await
        } else {
            false
        }
    }

    async fn payment_deadline_has_passed(timestamp: i64, day: i32) -> bool {
        let period: i64 = (86400 * day) as i64;
        let current_timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        let diference = current_timestamp - timestamp;
        diference > period
    }

    fn check_if_last_sell_block_is_paid(&self) -> bool {
        if self.exist_block_with_operation_code(Sell) {
            let last_version_block_sell = self.get_last_version_block_with_operation_code(Sell);

            let bill_keys = read_keys_from_bill_file(&last_version_block_sell.bill_name);
            let key: Rsa<Private> =
                Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
            let bytes = hex::decode(last_version_block_sell.data.clone()).unwrap();
            let decrypted_bytes = decrypt_bytes(&bytes, &key);
            let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

            let part_without_sold_to = block_data_decrypted
                .split("Sold to ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string();

            let part_with_seller_and_amount = part_without_sold_to
                .clone()
                .split(" sold by ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string();

            let amount: u64 = part_with_seller_and_amount
                .clone()
                .split(" amount: ")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string()
                .parse()
                .unwrap();

            let bill = self.get_first_version_bill();

            let address_to_pay =
                Self::get_address_to_pay_for_block_sell(last_version_block_sell.clone(), bill);

            thread::spawn(move || Self::check_if_paid(address_to_pay, amount))
                .join()
                .expect("Thread panicked")
        } else {
            false
        }
    }

    fn get_address_to_pay_for_block_sell(
        last_version_block_sell: Block,
        bill: BitcreditBill,
    ) -> String {
        let public_key_bill = bitcoin::PublicKey::from_str(&bill.public_key).unwrap();

        let bill_keys = read_keys_from_bill_file(&last_version_block_sell.bill_name);
        let key: Rsa<Private> =
            Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
        let bytes = hex::decode(last_version_block_sell.data.clone()).unwrap();
        let decrypted_bytes = decrypt_bytes(&bytes, &key);
        let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

        let part_without_sold_to = block_data_decrypted
            .split("Sold to ")
            .collect::<Vec<&str>>()
            .get(1)
            .unwrap()
            .to_string();

        let part_with_seller_and_amount = part_without_sold_to
            .clone()
            .split(" sold by ")
            .collect::<Vec<&str>>()
            .get(1)
            .unwrap()
            .to_string();

        let part_with_seller = part_with_seller_and_amount
            .clone()
            .split(" amount: ")
            .collect::<Vec<&str>>()
            .first()
            .unwrap()
            .to_string();

        let seller_bill_u8 = hex::decode(part_with_seller).unwrap();
        let seller_bill: IdentityPublicData = serde_json::from_slice(&seller_bill_u8).unwrap();

        let public_key_seller = seller_bill.bitcoin_public_key;
        let public_key_bill_seller = bitcoin::PublicKey::from_str(&public_key_seller).unwrap();

        let public_key_bill = public_key_bill
            .inner
            .combine(&public_key_bill_seller.inner)
            .unwrap();
        let pub_key_bill = bitcoin::PublicKey::new(public_key_bill);

        bitcoin::Address::p2pkh(pub_key_bill, USEDNET).to_string()
    }

    #[tokio::main]
    async fn check_if_paid(address: String, amount: u64) -> bool {
        let info_about_address =
            external::bitcoin::AddressInfo::get_address_info(address.clone()).await;
        let received_summ = info_about_address.chain_stats.funded_txo_sum;
        let spent_summ = info_about_address.chain_stats.spent_txo_sum;
        // let received_summ_mempool = info_about_address.mempool_stats.funded_txo_sum;
        let spent_summ_mempool = info_about_address.mempool_stats.spent_txo_sum;
        if amount.eq(&(received_summ + spent_summ
                // + received_summ_mempool
                + spent_summ_mempool))
        {
            true
        } else {
            false
        }
    }

    pub(super) fn get_first_version_bill(&self) -> BitcreditBill {
        let first_block_data = &self.get_first_block();
        let bill_keys = read_keys_from_bill_file(&first_block_data.bill_name);
        let key: Rsa<Private> =
            Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
        let bytes = hex::decode(first_block_data.data.clone()).unwrap();
        let decrypted_bytes = decrypt_bytes(&bytes, &key);
        let bill_first_version: BitcreditBill = bill_from_byte_array(&decrypted_bytes);
        bill_first_version
    }

    pub fn get_block_by_id(&self, id: u64) -> Block {
        let mut block = self.get_first_block().clone();
        for b in &self.blocks {
            if b.id == id {
                block = b.clone();
            }
        }
        block
    }

    pub fn compare_chain(&mut self, other_chain: Chain, bill_name: &str) {
        let local_chain_last_id = self.get_latest_block().id;
        let other_chain_last_id = other_chain.get_latest_block().id;
        if local_chain_last_id.eq(&other_chain_last_id) {
        } else if local_chain_last_id > other_chain_last_id {
            return;
        } else {
            let difference_in_id = other_chain_last_id - local_chain_last_id;
            for block_id in 1..difference_in_id + 1 {
                let block = other_chain.get_block_by_id(local_chain_last_id + block_id);
                let try_add_block = self.try_add_block(block);
                if try_add_block && self.is_chain_valid() {
                    self.write_chain_to_file(bill_name);
                } else {
                    return;
                }
            }
        }
    }

    pub fn get_all_nodes_from_bill(&self) -> Vec<String> {
        let mut nodes: Vec<String> = Vec::new();

        for block in &self.blocks {
            let bill = self.get_first_version_bill();
            let nodes_in_block = block.get_nodes_from_block(bill);
            for node in nodes_in_block {
                if !node.is_empty() && !nodes.contains(&node) {
                    nodes.push(node);
                }
            }
        }
        nodes
    }

    pub fn get_drawer(&self) -> IdentityPublicData {
        let drawer: IdentityPublicData;
        let bill = self.get_first_version_bill();
        if !bill.drawer.name.is_empty() {
            drawer = bill.drawer.clone();
        } else if bill.to_payee {
            drawer = bill.payee.clone();
        } else {
            drawer = bill.drawee.clone();
        }
        drawer
    }

    pub fn bill_contain_node(&self, request_node_id: String) -> bool {
        for block in &self.blocks {
            match block.operation_code {
                Issue => {
                    let bill = self.get_first_version_bill();
                    if bill.drawer.peer_id.eq(&request_node_id)
                        || bill.drawee.peer_id.eq(&request_node_id)
                        || bill.payee.peer_id.eq(&request_node_id)
                    {
                        return true;
                    }
                }
                Endorse => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let mut part_with_endorsee = block_data_decrypted
                        .split("Endorsed to ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    let part_with_endorsed_by = part_with_endorsee
                        .clone()
                        .split(" endorsed by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    part_with_endorsee = part_with_endorsee
                        .split(" endorsed by ")
                        .collect::<Vec<&str>>()
                        .first()
                        .unwrap()
                        .to_string();

                    let endorsee_bill_u8 = hex::decode(part_with_endorsee).unwrap();
                    let endorsee_bill: IdentityPublicData =
                        serde_json::from_slice(&endorsee_bill_u8).unwrap();

                    let endorser_bill_u8 = hex::decode(part_with_endorsed_by).unwrap();
                    let endorser_bill: IdentityPublicData =
                        serde_json::from_slice(&endorser_bill_u8).unwrap();

                    if endorsee_bill.peer_id.eq(&request_node_id)
                        || endorser_bill.peer_id.eq(&request_node_id)
                    {
                        return true;
                    }
                }
                Mint => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let mut part_with_mint = block_data_decrypted
                        .split("Endorsed to ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    let part_with_minter = part_with_mint
                        .clone()
                        .split(" endorsed by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    part_with_mint = part_with_mint
                        .split(" endorsed by ")
                        .collect::<Vec<&str>>()
                        .first()
                        .unwrap()
                        .to_string();

                    let minter_bill_u8 = hex::decode(part_with_minter).unwrap();
                    let minter_bill: IdentityPublicData =
                        serde_json::from_slice(&minter_bill_u8).unwrap();

                    let mint_bill_u8 = hex::decode(part_with_mint).unwrap();
                    let mint_bill: IdentityPublicData =
                        serde_json::from_slice(&mint_bill_u8).unwrap();

                    if minter_bill.peer_id.eq(&request_node_id)
                        || mint_bill.peer_id.eq(&request_node_id)
                    {
                        return true;
                    }
                }
                RequestToAccept => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let part_with_identity = block_data_decrypted
                        .split("Requested to accept by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();
                    let requester_to_accept_bill_u8 = hex::decode(part_with_identity).unwrap();
                    let requester_to_accept_bill: IdentityPublicData =
                        serde_json::from_slice(&requester_to_accept_bill_u8).unwrap();

                    if requester_to_accept_bill.peer_id.eq(&request_node_id) {
                        return true;
                    }
                }
                Accept => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let part_with_identity = block_data_decrypted
                        .split("Accepted by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();
                    let accepter_bill_u8 = hex::decode(part_with_identity).unwrap();
                    let accepter_bill: IdentityPublicData =
                        serde_json::from_slice(&accepter_bill_u8).unwrap();

                    if accepter_bill.peer_id.eq(&request_node_id) {
                        return true;
                    }
                }
                RequestToPay => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let part_with_identity = block_data_decrypted
                        .split("Requested to pay by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();
                    let requester_to_pay_bill_u8 = hex::decode(part_with_identity).unwrap();
                    let requester_to_pay_bill: IdentityPublicData =
                        serde_json::from_slice(&requester_to_pay_bill_u8).unwrap();

                    if requester_to_pay_bill.peer_id.eq(&request_node_id) {
                        return true;
                    }
                }
                Sell => {
                    let block = self.get_block_by_id(block.id);

                    let bill_keys = read_keys_from_bill_file(&block.bill_name);
                    let key: Rsa<Private> =
                        Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                    let bytes = hex::decode(block.data.clone()).unwrap();
                    let decrypted_bytes = decrypt_bytes(&bytes, &key);
                    let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                    let part_without_sold_to = block_data_decrypted
                        .split("Sold to ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    let part_with_buyer = part_without_sold_to
                        .split(" sold by ")
                        .collect::<Vec<&str>>()
                        .first()
                        .unwrap()
                        .to_string();

                    let part_with_seller_and_amount = part_without_sold_to
                        .clone()
                        .split(" sold by ")
                        .collect::<Vec<&str>>()
                        .get(1)
                        .unwrap()
                        .to_string();

                    let part_with_seller = part_with_seller_and_amount
                        .clone()
                        .split(" amount: ")
                        .collect::<Vec<&str>>()
                        .first()
                        .unwrap()
                        .to_string();

                    let buyer_bill_u8 = hex::decode(part_with_buyer).unwrap();
                    let buyer_bill: IdentityPublicData =
                        serde_json::from_slice(&buyer_bill_u8).unwrap();

                    let seller_bill_u8 = hex::decode(part_with_seller).unwrap();
                    let seller_bill: IdentityPublicData =
                        serde_json::from_slice(&seller_bill_u8).unwrap();

                    if buyer_bill.peer_id.eq(&request_node_id)
                        || seller_bill.peer_id.eq(&request_node_id)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

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
    } else if !block.verifier() {
        warn!("block with id: {} has invalid signature", block.id);
        return false;
    }
    true
}
