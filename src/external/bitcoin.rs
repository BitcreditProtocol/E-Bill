use crate::CONFIG;
use async_trait::async_trait;
use bitcoin::{secp256k1::Scalar, Network};
use serde::Deserialize;
use std::str::FromStr;
use thiserror::Error;

/// Generic result type
pub type Result<T> = std::result::Result<T, super::Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// all errors originating from interacting with the web api
    #[error("External Bitcoin Web API error: {0}")]
    Api(#[from] reqwest::Error),

    /// all errors originating from dealing with secp256k1 keys
    #[error("External Bitcoin Key error: {0}")]
    Key(#[from] bitcoin::secp256k1::Error),

    /// all errors originating from dealing with public secp256k1 keys
    #[error("External Bitcoin Public Key error: {0}")]
    PublicKey(#[from] bitcoin::key::ParsePublicKeyError),

    /// all errors originating from dealing with private secp256k1 keys
    #[error("External Bitcoin Private Key error: {0}")]
    PrivateKey(#[from] bitcoin::key::FromWifError),
}

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BitcoinClientApi: Send + Sync {
    async fn get_address_info(&self, address: &str) -> Result<AddressInfo>;

    async fn get_transactions(&self, address: &str) -> Result<Transactions>;

    async fn get_last_block_height(&self) -> Result<u64>;

    fn get_first_transaction(&self, transactions: &Transactions) -> Option<Txid>;

    async fn check_if_paid(&self, address: &str, amount: u64) -> Result<(bool, u64)>;

    fn get_address_to_pay(&self, bill_public_key: &str, holder_public_key: &str) -> Result<String>;

    fn generate_link_to_pay(&self, address: &str, amount: u64, message: &str) -> String;

    fn get_combined_private_key(&self, pkey: &str, pkey_to_combine: &str) -> Result<String>;
}

#[derive(Clone)]
pub struct BitcoinClient;

impl BitcoinClient {
    pub fn new() -> Self {
        Self {}
    }

    pub fn request_url(&self, path: &str) -> String {
        match CONFIG.bitcoin_network() {
            Network::Bitcoin => {
                format!("https://blockstream.info/api{path}")
            }
            _ => {
                format!("https://blockstream.info/testnet/api{path}")
            }
        }
    }
}

#[async_trait]
impl BitcoinClientApi for BitcoinClient {
    async fn get_address_info(&self, address: &str) -> Result<AddressInfo> {
        let address: AddressInfo = reqwest::get(&self.request_url(&format!("/address/{address}")))
            .await
            .map_err(Error::from)?
            .json()
            .await
            .map_err(Error::from)?;

        Ok(address)
    }

    async fn get_transactions(&self, address: &str) -> Result<Transactions> {
        let transactions: Transactions =
            reqwest::get(&self.request_url(&format!("/address/{address}/txs")))
                .await
                .map_err(Error::from)?
                .json()
                .await
                .map_err(Error::from)?;

        Ok(transactions)
    }

    async fn get_last_block_height(&self) -> Result<u64> {
        let height: u64 = reqwest::get(&self.request_url("/blocks/tip/height"))
            .await?
            .json()
            .await?;

        Ok(height)
    }

    fn get_first_transaction(&self, transactions: &Transactions) -> Option<Txid> {
        transactions.last().cloned()
    }

    async fn check_if_paid(&self, address: &str, amount: u64) -> Result<(bool, u64)> {
        //todo check what net we used
        let info_about_address = self.get_address_info(address).await?;
        let received_summ = info_about_address.chain_stats.funded_txo_sum;
        let spent_summ = info_about_address.chain_stats.spent_txo_sum;
        let received_summ_mempool = info_about_address.mempool_stats.funded_txo_sum;
        let spent_summ_mempool = info_about_address.mempool_stats.spent_txo_sum;
        if amount.eq(&(received_summ + spent_summ + received_summ_mempool + spent_summ_mempool)) {
            Ok((true, received_summ))
        } else {
            Ok((false, 0))
        }
    }

    fn get_address_to_pay(&self, bill_public_key: &str, holder_public_key: &str) -> Result<String> {
        let public_key_bill = bitcoin::PublicKey::from_str(bill_public_key).map_err(Error::from)?;
        let public_key_bill_holder =
            bitcoin::PublicKey::from_str(holder_public_key).map_err(Error::from)?;

        let public_key_bill = public_key_bill
            .inner
            .combine(&public_key_bill_holder.inner)
            .map_err(Error::from)?;
        let pub_key_bill = bitcoin::PublicKey::new(public_key_bill);

        Ok(bitcoin::Address::p2pkh(pub_key_bill, CONFIG.bitcoin_network()).to_string())
    }

    fn generate_link_to_pay(&self, address: &str, amount: u64, message: &str) -> String {
        //todo check what net we used
        let link = format!("bitcoin:{}?amount={}&message={}", address, amount, message);
        link
    }

    fn get_combined_private_key(&self, pkey: &str, pkey_to_combine: &str) -> Result<String> {
        let private_key_bill = bitcoin::PrivateKey::from_str(pkey).map_err(Error::from)?;

        let private_key_bill_holder =
            bitcoin::PrivateKey::from_str(pkey_to_combine).map_err(Error::from)?;

        let private_key_bill = private_key_bill
            .inner
            .add_tweak(&Scalar::from(private_key_bill_holder.inner))
            .map_err(Error::from)?;
        Ok(bitcoin::PrivateKey::new(private_key_bill, CONFIG.bitcoin_network()).to_string())
    }
}

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
