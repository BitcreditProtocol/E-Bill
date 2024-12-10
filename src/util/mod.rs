pub mod date;
pub mod file;
pub mod numbers_to_words;
pub mod rsa;
pub mod terminal;
use bitcoin::{Network, PrivateKey, PublicKey};
use openssl::sha::sha256;
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
