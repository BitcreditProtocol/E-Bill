use borsh::to_vec;
use openssl::sha::Sha256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::blockchain::OperationCode::{
    Accept, Endorse, Issue, Mint, RequestToAccept, RequestToPay, Sell,
};
use crate::constants::SIGNED_BY;
use crate::external;
use crate::service::bill_service::{BillKeys, BitcreditBill};
use crate::service::contact_service::IdentityPublicData;
use crate::util::rsa;
pub use block::Block;
pub use chain::Chain;
use std::string::FromUtf8Error;

mod block;
mod chain;

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// Errors from io handling, or binary serialization/deserialization
    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    /// If a whole chain is not valid
    #[error("Blockchain is invalid")]
    BlockchainInvalid,

    /// Errors stemming from json deserialization. Most of the time this is a
    #[error("unable to serialize/deserialize to/from JSON {0}")]
    Json(#[from] serde_json::Error),

    /// Errors stemming from cryptography, such as converting keys
    #[error("Cryptography error: {0}")]
    Cryptography(#[from] openssl::error::ErrorStack),

    /// Errors stemming from decoding
    #[error("Decode error: {0}")]
    Decode(#[from] hex::FromHexError),

    /// Errors stemming from converting from utf-8 strings
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),

    /// Errors stemming from dealing with invalid block data, e.g. if within an Endorse block,
    /// there is no endorsee
    #[error("Invalid block data error: {0}")]
    InvalidBlockdata(String),

    /// all errors originating from external APIs
    #[error("External API error: {0}")]
    ExternalApi(#[from] external::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChainToReturn {
    pub blocks: Vec<BlockToReturn>,
}

impl ChainToReturn {
    /// Creates a new Chain to return by transforming a given `Chain` into its corresponding representation.
    ///
    /// # Parameters
    /// * `chain` - The `Chain` to be transformed. It contains the list of blocks and the initial bill version
    ///   necessary for processing.
    /// * `bill_keys` - The keys for the bill
    ///
    /// # Returns
    /// A new instance containing the transformed `BlockToReturn` objects.
    ///
    pub fn new(chain: Chain, bill_keys: &BillKeys) -> Result<Self> {
        let mut blocks: Vec<BlockToReturn> = Vec::new();
        for block in chain.blocks {
            blocks.push(BlockToReturn::new(block, bill_keys)?);
        }
        Ok(Self { blocks })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OperationCode {
    Issue,
    Accept,
    Endorse,
    RequestToAccept,
    RequestToPay,
    Sell,
    Mint,
}

impl OperationCode {
    pub fn get_all_operation_codes() -> Vec<OperationCode> {
        vec![
            Issue,
            Accept,
            Endorse,
            RequestToAccept,
            RequestToPay,
            Sell,
            Mint,
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitingForPayment {
    Yes(Box<PaymentInfo>),
    No,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentInfo {
    pub buyer: IdentityPublicData,
    pub seller: IdentityPublicData,
    pub amount: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BlockToReturn {
    pub id: u64,
    pub bill_name: String,
    pub hash: String,
    pub timestamp: i64,
    pub data: String,
    pub previous_hash: String,
    pub signature: String,
    pub public_key: String,
    pub operation_code: OperationCode,
    pub label: String,
}

impl BlockToReturn {
    /// Creates a new block to return for the given bill, with an attached history label,
    /// describing what happened in this block
    pub fn new(block: Block, bill_keys: &BillKeys) -> Result<Self> {
        let label = block.get_history_label(bill_keys)?;

        Ok(Self {
            id: block.id,
            bill_name: block.bill_name,
            hash: block.hash,
            timestamp: block.timestamp,
            data: block.data,
            previous_hash: block.previous_hash,
            signature: block.signature,
            public_key: block.public_key,
            operation_code: block.operation_code,
            label,
        })
    }
}

/// Creates a new blockchain for the given bill, encrypting the metadata using the bill's public
/// key
pub fn start_blockchain_for_new_bill(
    bill: &BitcreditBill,
    operation_code: OperationCode,
    drawer: IdentityPublicData,
    drawer_public_key: String,
    drawer_private_key: String,
    bill_public_key_pem: String,
    timestamp: i64,
) -> Result<Chain> {
    let drawer_bytes = serde_json::to_vec(&drawer)?;
    let data_for_new_block = format!("{}{}", SIGNED_BY, hex::encode(drawer_bytes));

    let genesis_hash: String = hex::encode(data_for_new_block.as_bytes());

    let encrypted_and_hashed_bill_data = hex::encode(rsa::encrypt_bytes_with_public_key(
        &to_vec(bill)?,
        &bill_public_key_pem,
    ));

    let first_block = Block::new(
        1,
        genesis_hash,
        encrypted_and_hashed_bill_data,
        bill.name.clone(),
        drawer_public_key,
        operation_code,
        drawer_private_key,
        timestamp,
    )?;

    let chain = Chain::new(first_block);
    Ok(chain)
}

fn calculate_hash(
    id: &u64,
    bill_name: &str,
    previous_hash: &str,
    data: &str,
    timestamp: &i64,
    public_key: &str,
    operation_code: &OperationCode,
) -> Vec<u8> {
    let data = serde_json::json!({
        "id": id,
        "bill_name": bill_name,
        "previous_hash": previous_hash,
        "data": data,
        "timestamp": timestamp,
        "public_key": public_key,
        "operation_code": operation_code,
    });
    let mut hasher = Sha256::new();
    hasher.update(data.to_string().as_bytes());
    hasher.finish().to_vec()
}

fn extract_after_phrase(input: &str, phrase: &str) -> Option<String> {
    if let Some(start) = input.find(phrase) {
        let start_idx = start + phrase.len();
        if let Some(remaining) = input.get(start_idx..) {
            if let Some(end_idx) = remaining.find(' ') {
                return Some(remaining[..end_idx].to_string());
            } else {
                return Some(remaining.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        service::identity_service::{Identity, IdentityWithAll},
        tests::test::{TEST_PRIVATE_KEY, TEST_PUB_KEY},
        util::BcrKeys,
    };
    use libp2p::PeerId;

    pub fn get_baseline_identity() -> IdentityWithAll {
        let mut identity = Identity::new_empty();
        identity.name = "drawer".to_owned();
        identity.public_key_pem = TEST_PUB_KEY.to_owned();
        identity.private_key_pem = TEST_PRIVATE_KEY.to_owned();
        IdentityWithAll {
            identity,
            peer_id: PeerId::random(),
            key_pair: BcrKeys::new(),
        }
    }

    #[test]
    fn start_blockchain_for_new_bill_baseline() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let result = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.public_key_pem,
            identity.identity.private_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        );

        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().blocks.len(), 1);
    }

    #[test]
    fn start_blockchain_for_new_bill_baseline_fails_with_invalid_keys() {
        let bill = BitcreditBill::new_empty();
        let identity = get_baseline_identity();

        let result = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string()),
            identity.identity.private_key_pem, // swapped private and public
            identity.identity.public_key_pem,
            TEST_PUB_KEY.to_owned(),
            1731593928,
        );

        assert!(result.is_err());
    }

    #[test]
    fn extract_after_phrase_basic() {
        assert_eq!(
            extract_after_phrase(
                "Endorsed by 123 endorsed to 456 amount: 5000",
                "Endorsed by "
            ),
            Some(String::from("123"))
        );
        assert_eq!(
            extract_after_phrase(
                "Endorsed by 123 endorsed to 456 amount: 5000",
                " endorsed to "
            ),
            Some(String::from("456"))
        );
        assert_eq!(
            extract_after_phrase("Endorsed by 123 endorsed to 456 amount: 5000", " amount: "),
            Some(String::from("5000"))
        );
        assert_eq!(
            extract_after_phrase(
                "Endorsed by 123 endorsed to 456 amount: 5000",
                " weird stuff "
            ),
            None
        );
    }
}
