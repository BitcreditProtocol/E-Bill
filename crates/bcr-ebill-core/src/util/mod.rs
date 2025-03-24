pub mod crypto;
pub mod date;

pub use crypto::BcrKeys;

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub fn get_uuid_v4() -> Uuid {
    Uuid::new_v4()
}

pub fn sha256_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    base58_encode(&hash)
}

#[derive(Debug, Error)]
pub enum Error {
    /// Errors stemming base58 decoding
    #[error("Decode base58 error: {0}")]
    Base58(bitcoin::base58::InvalidCharacterError),
}

pub fn base58_encode(bytes: &[u8]) -> String {
    bitcoin::base58::encode(bytes)
}

pub fn base58_decode(input: &str) -> std::result::Result<Vec<u8>, Error> {
    bitcoin::base58::decode(input).map_err(Error::Base58)
}
