use crate::{bill::BitcreditBill, constants::USEDNET};
use bitcoin::Network;
use serde::Deserialize;
use std::str::FromStr;

/// Fields documented at https://github.com/Blockstream/esplora/blob/master/API.md#addresses
#[derive(Deserialize, Debug)]
pub struct AddressInfo {
    pub chain_stats: Stats,
    pub mempool_stats: Stats,
}

#[derive(Deserialize, Debug)]
pub struct Stats {
    pub funded_txo_sum: u64,
    pub spent_txo_sum: u64,
}

impl AddressInfo {
    pub async fn get_address_info(address: String) -> Self {
        let request_url = match USEDNET {
            Network::Bitcoin => {
                format!(
                    "https://blockstream.info/api/address/{address}",
                    address = address
                )
            }
            _ => {
                format!(
                    "https://blockstream.info/testnet/api/address/{address}",
                    address = address
                )
            }
        };
        let address: AddressInfo = reqwest::get(&request_url)
            .await
            .expect("Failed to send request")
            .json()
            .await
            .expect("Failed to read response");

        address
    }
}

pub type Transactions = Vec<Txid>;

/// Available fields documented at https://github.com/Blockstream/esplora/blob/master/API.md#transactions
#[derive(Deserialize, Debug, Clone)]
pub struct Txid {
    pub status: Status,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Status {
    pub block_height: u64,
}

pub async fn get_transactions(address: String) -> Transactions {
    let request_url = match USEDNET {
        Network::Bitcoin => {
            format!(
                "https://blockstream.info/api/address/{address}/txs",
                address = address
            )
        }
        _ => {
            format!(
                "https://blockstream.info/testnet/api/address/{address}/txs",
                address = address
            )
        }
    };
    let transactions: Transactions = reqwest::get(&request_url)
        .await
        .expect("Failed to send request")
        .json()
        .await
        .expect("Failed to read response");

    transactions
}

impl Txid {
    pub async fn get_first_transaction(transactions: Transactions) -> Self {
        transactions.last().unwrap().clone()
    }
}

pub async fn get_last_block_height() -> u64 {
    let request_url = match USEDNET {
        Network::Bitcoin => "https://blockstream.info/api/blocks/tip/height",
        _ => "https://blockstream.info/testnet/api/blocks/tip/height",
    };
    let height: u64 = reqwest::get(request_url)
        .await
        .expect("Failed to send request")
        .json()
        .await
        .expect("Failed to read response");

    height
}

pub async fn check_if_paid(address: String, amount: u64) -> (bool, u64) {
    //todo check what net we used
    let info_about_address = AddressInfo::get_address_info(address.clone()).await;
    let received_summ = info_about_address.chain_stats.funded_txo_sum;
    let spent_summ = info_about_address.chain_stats.spent_txo_sum;
    let received_summ_mempool = info_about_address.mempool_stats.funded_txo_sum;
    let spent_summ_mempool = info_about_address.mempool_stats.spent_txo_sum;
    if amount.eq(&(received_summ + spent_summ + received_summ_mempool + spent_summ_mempool)) {
        (true, received_summ)
    } else {
        (false, 0)
    }
}

pub fn get_address_to_pay(bill: BitcreditBill) -> String {
    let public_key_bill = bitcoin::PublicKey::from_str(&bill.public_key).unwrap();

    let mut person_to_pay = bill.payee.clone();

    if !bill.endorsee.name.is_empty() {
        person_to_pay = bill.endorsee.clone();
    }

    let public_key_holder = person_to_pay.bitcoin_public_key;
    let public_key_bill_holder = bitcoin::PublicKey::from_str(&public_key_holder).unwrap();

    let public_key_bill = public_key_bill
        .inner
        .combine(&public_key_bill_holder.inner)
        .unwrap();
    let pub_key_bill = bitcoin::PublicKey::new(public_key_bill);

    bitcoin::Address::p2pkh(pub_key_bill, USEDNET).to_string()
}

pub async fn generate_link_to_pay(address: String, amount: u64, message: String) -> String {
    //todo check what net we used
    let link = format!("bitcoin:{}?amount={}&message={}", address, amount, message);
    link
}
