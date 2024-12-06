pub mod date;
pub mod file;
pub mod numbers_to_words;
pub mod rsa;
pub mod terminal;
use crate::CONFIG;
use crate::{service::bill_service::BitcreditBill, service::identity_service::Identity};
use bitcoin::{secp256k1::Scalar, Network, PrivateKey, PublicKey};
use openssl::sha::sha256;
use std::str::FromStr;
use uuid::Uuid;

#[cfg(not(test))]
pub fn get_uuid_v4() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
pub fn get_uuid_v4() -> Uuid {
    use uuid::uuid;
    uuid!("00000000-0000-0000-0000-000000000000")
}

pub fn create_bitcoin_keypair(used_network: Network) -> (PrivateKey, PublicKey) {
    let key_context = bitcoin::secp256k1::Secp256k1::new();
    let private_key = bitcoin::PrivateKey::new(
        key_context
            .generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
            .0,
        used_network,
    );
    let public_key = private_key.public_key(&key_context);
    (private_key, public_key)
}

pub fn sha256_hash(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes))
}

pub fn get_current_payee_private_key(identity: Identity, bill: BitcreditBill) -> String {
    let private_key_bill = bitcoin::PrivateKey::from_str(&bill.private_key).unwrap();

    let private_key_bill_holder =
        bitcoin::PrivateKey::from_str(&identity.bitcoin_private_key).unwrap();

    let privat_key_bill = private_key_bill
        .inner
        .add_tweak(&Scalar::from(private_key_bill_holder.inner))
        .unwrap();
    bitcoin::PrivateKey::new(privat_key_bill, CONFIG.bitcoin_network()).to_string()
}
