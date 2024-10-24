use super::OperationCode;
use crate::blockchain::calculate_hash;
use crate::blockchain::OperationCode::{
    Accept, Endorse, Issue, Mint, RequestToAccept, RequestToPay, Sell,
};
use crate::{
    bill::{contacts::IdentityPublicData, read_keys_from_bill_file, BitcreditBill},
    util::rsa::{decrypt_bytes, private_key_from_pem_u8, public_key_from_pem_u8},
};
use chrono::prelude::*;
use log::info;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::pkey::Private;
use openssl::rsa::Rsa;
use openssl::sign::Signer;
use openssl::sign::Verifier;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Block {
    pub id: u64,
    pub bill_name: String,
    pub hash: String,
    pub timestamp: i64,
    pub data: String,
    pub previous_hash: String,
    pub signature: String,
    pub public_key: String,
    pub operation_code: OperationCode,
}

impl Block {
    pub fn new(
        id: u64,
        previous_hash: String,
        data: String,
        bill_name: String,
        public_key: String,
        operation_code: OperationCode,
        private_key: String,
        timestamp: i64,
    ) -> Self {
        let hash: String = mine_block(
            &id,
            &bill_name,
            &previous_hash,
            &data,
            &timestamp,
            &public_key,
            &operation_code,
        );
        let signature = signature(hash.clone(), private_key.clone());

        Self {
            id,
            bill_name,
            hash,
            timestamp,
            previous_hash,
            signature,
            data,
            public_key,
            operation_code,
        }
    }

    pub fn get_nodes_from_block(&self, bill: BitcreditBill) -> Vec<String> {
        let mut nodes = Vec::new();
        match self.operation_code {
            Issue => {
                let drawer_name = bill.drawer.peer_id.clone();
                if !drawer_name.is_empty() && !nodes.contains(&drawer_name) {
                    nodes.push(drawer_name);
                }

                let payee_name = bill.payee.peer_id.clone();
                if !payee_name.is_empty() && !nodes.contains(&payee_name) {
                    nodes.push(payee_name);
                }

                let drawee_name = bill.drawee.peer_id.clone();
                if !drawee_name.is_empty() && !nodes.contains(&drawee_name) {
                    nodes.push(drawee_name);
                }
            }
            Endorse => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let endorsee_bill_name = endorsee_bill.peer_id.clone();
                if !endorsee_bill_name.is_empty() && !nodes.contains(&endorsee_bill_name) {
                    nodes.push(endorsee_bill_name);
                }

                let endorser_bill_u8 = hex::decode(part_with_endorsed_by).unwrap();
                let endorser_bill: IdentityPublicData =
                    serde_json::from_slice(&endorser_bill_u8).unwrap();
                let endorser_bill_name = endorser_bill.peer_id.clone();
                if !endorser_bill_name.is_empty() && !nodes.contains(&endorser_bill_name) {
                    nodes.push(endorser_bill_name);
                }
            }
            Mint => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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

                let mint_bill_u8 = hex::decode(part_with_mint).unwrap();
                let mint_bill: IdentityPublicData = serde_json::from_slice(&mint_bill_u8).unwrap();
                let mint_bill_name = mint_bill.peer_id.clone();
                if !mint_bill_name.is_empty() && !nodes.contains(&mint_bill_name) {
                    nodes.push(mint_bill_name);
                }

                let minter_bill_u8 = hex::decode(part_with_minter).unwrap();
                let minter_bill: IdentityPublicData =
                    serde_json::from_slice(&minter_bill_u8).unwrap();
                let minter_bill_name = minter_bill.peer_id.clone();
                if !minter_bill_name.is_empty() && !nodes.contains(&minter_bill_name) {
                    nodes.push(minter_bill_name);
                }
            }
            RequestToAccept => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let requester_to_accept_bill_name = requester_to_accept_bill.peer_id.clone();
                if !requester_to_accept_bill_name.is_empty()
                    && !nodes.contains(&requester_to_accept_bill_name)
                {
                    nodes.push(requester_to_accept_bill_name);
                }
            }
            Accept => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let accepter_bill_name = accepter_bill.peer_id.clone();
                if !accepter_bill_name.is_empty() && !nodes.contains(&accepter_bill_name) {
                    nodes.push(accepter_bill_name);
                }
            }
            RequestToPay => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let requester_to_pay_bill_name = requester_to_pay_bill.peer_id.clone();
                if !requester_to_pay_bill_name.is_empty()
                    && !nodes.contains(&requester_to_pay_bill_name)
                {
                    nodes.push(requester_to_pay_bill_name);
                }
            }
            Sell => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let buyer_peer_id = buyer_bill.peer_id.clone();
                if !buyer_peer_id.is_empty() && !nodes.contains(&buyer_peer_id) {
                    nodes.push(buyer_peer_id);
                }

                let seller_bill_u8 = hex::decode(part_with_seller).unwrap();
                let seller_bill: IdentityPublicData =
                    serde_json::from_slice(&seller_bill_u8).unwrap();
                let seller_bill_peer_id = seller_bill.peer_id.clone();
                if !seller_bill_peer_id.is_empty() && !nodes.contains(&seller_bill_peer_id) {
                    nodes.push(seller_bill_peer_id);
                }
            }
        }
        nodes
    }

    pub fn get_history_label(&self, bill: BitcreditBill) -> String {
        match self.operation_code {
            Issue => {
                let time_of_issue = Utc.timestamp_opt(self.timestamp, 0).unwrap();
                if !bill.drawer.name.is_empty() {
                    format!(
                        "Bill issued by {} at {} in {}",
                        bill.drawer.name, time_of_issue, bill.place_of_drawing
                    )
                } else if bill.to_payee {
                    format!(
                        "Bill issued by {} at {} in {}",
                        bill.payee.name, time_of_issue, bill.place_of_drawing
                    )
                } else {
                    format!(
                        "Bill issued by {} at {} in {}",
                        bill.drawee.name, time_of_issue, bill.place_of_drawing
                    )
                }
            }
            Endorse => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
                let decrypted_bytes = decrypt_bytes(&bytes, &key);
                let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                let part_with_endorsee = block_data_decrypted
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

                let endorser_bill_u8 = hex::decode(part_with_endorsed_by).unwrap();
                let endorser_bill: IdentityPublicData =
                    serde_json::from_slice(&endorser_bill_u8).unwrap();

                endorser_bill.name + ", " + &endorser_bill.postal_address
            }
            Mint => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
                let decrypted_bytes = decrypt_bytes(&bytes, &key);
                let block_data_decrypted = String::from_utf8(decrypted_bytes).unwrap();

                let part_with_mint = block_data_decrypted
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

                let minter_bill_u8 = hex::decode(part_with_minter).unwrap();
                let minter_bill: IdentityPublicData =
                    serde_json::from_slice(&minter_bill_u8).unwrap();

                minter_bill.name + ", " + &minter_bill.postal_address
            }
            RequestToAccept => {
                let time_of_request_to_accept = Utc.timestamp_opt(self.timestamp, 0).unwrap();

                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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

                format!(
                    "Bill requested to accept by {} at {} in {}",
                    requester_to_accept_bill.name,
                    time_of_request_to_accept,
                    requester_to_accept_bill.postal_address
                )
            }
            Accept => {
                let time_of_accept = Utc.timestamp_opt(self.timestamp, 0).unwrap();

                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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

                format!(
                    "Bill accepted by {} at {} in {}",
                    accepter_bill.name, time_of_accept, accepter_bill.postal_address
                )
            }
            RequestToPay => {
                let time_of_request_to_pay = Utc.timestamp_opt(self.timestamp, 0).unwrap();

                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                format!(
                    "Bill requested to pay by {} at {} in {}",
                    requester_to_pay_bill.name,
                    time_of_request_to_pay,
                    requester_to_pay_bill.postal_address
                )
            }
            Sell => {
                let bill_keys = read_keys_from_bill_file(&self.bill_name);
                let key: Rsa<Private> =
                    Rsa::private_key_from_pem(bill_keys.private_key_pem.as_bytes()).unwrap();
                let bytes = hex::decode(self.data.clone()).unwrap();
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
                let seller_bill: IdentityPublicData =
                    serde_json::from_slice(&seller_bill_u8).unwrap();

                seller_bill.name + ", " + &seller_bill.postal_address
            }
        }
    }

    pub fn verifier(&self) -> bool {
        let public_key_bytes = self.public_key.as_bytes();
        let public_key_rsa = public_key_from_pem_u8(public_key_bytes);
        let verifier_key = PKey::from_rsa(public_key_rsa).unwrap();

        let mut verifier = Verifier::new(MessageDigest::sha256(), verifier_key.as_ref()).unwrap();

        let data_to_check = self.hash.as_bytes();
        verifier.update(data_to_check).unwrap();

        let signature_bytes = hex::decode(&self.signature).unwrap();
        verifier.verify(signature_bytes.as_slice()).unwrap()
    }
}

fn mine_block(
    id: &u64,
    bill_name: &str,
    previous_hash: &str,
    data: &str,
    timestamp: &i64,
    public_key: &str,
    operation_code: &OperationCode,
) -> String {
    let hash = calculate_hash(
        id,
        bill_name,
        previous_hash,
        data,
        timestamp,
        public_key,
        operation_code,
    );
    let binary_hash = hex::encode(&hash);
    info!(
        "mined! hash: {}, binary hash: {}",
        hex::encode(&hash),
        binary_hash
    );
    hex::encode(hash)
}

fn signature(hash: String, private_key_pem: String) -> String {
    let private_key_bytes = private_key_pem.as_bytes();
    let private_key_rsa = private_key_from_pem_u8(private_key_bytes);
    let signer_key = PKey::from_rsa(private_key_rsa).unwrap();

    let mut signer: Signer = Signer::new(MessageDigest::sha256(), signer_key.as_ref()).unwrap();

    let data_to_sign = hash.as_bytes();
    signer.update(data_to_sign).unwrap();

    let signature: Vec<u8> = signer.sign_to_vec().unwrap();
    let signature_readable = hex::encode(signature.as_slice());

    signature_readable
}
