use borsh::{to_vec, BorshDeserialize};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use openssl::pkey::Private;
use openssl::rsa::Rsa;
use openssl::sha::Sha256;
use rocket::FromFormField;
use serde::{Deserialize, Serialize};

use crate::blockchain::OperationCode::{
    Accept, Endorse, Issue, Mint, RequestToAccept, RequestToPay, Sell,
};
use crate::constants::BILLS_FOLDER_PATH;
use crate::{
    bill::{contacts::IdentityPublicData, BitcreditBill},
    util::rsa::encrypt_bytes,
};

pub use block::Block;
pub use chain::Chain;

mod block;
mod chain;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChainToReturn {
    pub blocks: Vec<BlockToReturn>,
}

impl ChainToReturn {
    pub fn new(chain: Chain) -> Self {
        let mut blocks: Vec<BlockToReturn> = Vec::new();
        let bill = chain.get_first_version_bill();
        for block in chain.blocks {
            blocks.push(BlockToReturn::new(block, bill.clone()));
        }
        Self { blocks }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, FromFormField)]
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
    pub fn new(block: Block, bill: BitcreditBill) -> Self {
        let label = block.get_history_label(bill);

        Self {
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
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct GossipsubEvent {
    pub id: GossipsubEventId,
    pub message: Vec<u8>,
}

impl GossipsubEvent {
    pub fn new(id: GossipsubEventId, message: Vec<u8>) -> Self {
        Self { id, message }
    }

    pub fn to_byte_array(&self) -> Vec<u8> {
        to_vec(self).expect("Failed to serialize event")
    }

    pub fn from_byte_array(bytes: &[u8]) -> Self {
        Self::try_from_slice(bytes).expect("Failed to deserialize event")
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub enum GossipsubEventId {
    Block,
    Chain,
    CommandGetChain,
}

pub fn start_blockchain_for_new_bill(
    bill: &BitcreditBill,
    operation_code: OperationCode,
    drawer: IdentityPublicData,
    public_key: String,
    private_key: String,
    private_key_pem: String,
    timestamp: i64,
) {
    let data_for_new_block_in_bytes = serde_json::to_vec(&drawer).unwrap();
    let data_for_new_block = "Signed by ".to_string() + &hex::encode(data_for_new_block_in_bytes);

    let genesis_hash: String = hex::encode(data_for_new_block.as_bytes());

    let bill_data: String = encrypted_hash_data_from_bill(bill, private_key_pem);

    let first_block = Block::new(
        1,
        genesis_hash,
        bill_data,
        bill.name.clone(),
        public_key,
        operation_code,
        private_key,
        timestamp,
    );

    let chain = Chain::new(first_block);
    let output_path = BILLS_FOLDER_PATH.to_string() + "/" + bill.name.clone().as_str() + ".json";
    std::fs::write(
        output_path.clone(),
        serde_json::to_string_pretty(&chain).unwrap(),
    )
    .unwrap();
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

fn encrypted_hash_data_from_bill(bill: &BitcreditBill, private_key_pem: String) -> String {
    let bytes = to_vec(bill).unwrap();
    let key: Rsa<Private> = Rsa::private_key_from_pem(private_key_pem.as_bytes()).unwrap();
    let encrypted_bytes = encrypt_bytes(&bytes, &key);

    hex::encode(encrypted_bytes)
}
