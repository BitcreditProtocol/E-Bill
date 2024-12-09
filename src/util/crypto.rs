use std::str::FromStr;

use bitcoin::{
    secp256k1::{self, Keypair, SecretKey},
    Network,
};
use nostr_sdk::ToBech32;
use secp256k1::{rand, Secp256k1};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Private key error: {0}")]
    PrivateKey(#[from] secp256k1::Error),

    #[error("Nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),

    #[error("Nostr bech32 key error: {0}")]
    NostrNip19(#[from] nostr_sdk::nips::nip19::Error),

    #[error("Libp2p key conversion error: {0}")]
    Secp256k1Conversion(#[from] libp2p::identity::DecodingError),

    #[error("Libp2p key error: {0}")]
    LibP2p(#[from] libp2p::identity::OtherVariantError),
}

/// A wrapper around the secp256k1 keypair that can be used for
/// Bitcoin and Nostr keys.
#[derive(Clone, Debug)]
pub struct BcrKeys {
    inner: Keypair,
}

#[allow(dead_code)]
impl BcrKeys {
    /// Generates a fresh random keypair that can be used for
    /// Bitocin and Nostr keys.
    pub fn new() -> Self {
        Self {
            inner: generate_keypair(),
        }
    }

    /// Loads a keypair from a given private key string
    pub fn from_private_key(private_key: &str) -> Result<Self> {
        let keypair = load_keypair(private_key)?;
        Ok(Self { inner: keypair })
    }

    /// Returns the private key as a hex encoded string
    pub fn get_private_key_string(&self) -> String {
        self.inner.secret_key().display_secret().to_string()
    }

    /// Returns the public key as a hex encoded string
    pub fn get_public_key(&self) -> String {
        self.inner.public_key().to_string()
    }

    pub fn get_bitcoin_keys(
        &self,
        used_network: Network,
    ) -> (bitcoin::PrivateKey, bitcoin::PublicKey) {
        let secp = Secp256k1::new();
        let private_key = self.get_bitcoin_private_key(used_network);
        (private_key, private_key.public_key(&secp))
    }

    /// Returns the key pair as a bitcoin private key for the given network
    pub fn get_bitcoin_private_key(&self, used_network: Network) -> bitcoin::PrivateKey {
        bitcoin::PrivateKey::new(self.inner.secret_key(), used_network)
    }

    /// Returns the key pair as a nostr key pair
    pub fn get_nostr_keys(&self) -> nostr_sdk::Keys {
        nostr_sdk::Keys::new(self.inner.secret_key().into())
    }

    /// Returns the nostr public key as a bech32 string
    pub fn get_nostr_npub(&self) -> Result<String> {
        Ok(self.get_nostr_keys().public_key().to_bech32()?)
    }

    /// Returns the nostr private key as a bech32 string
    pub fn get_nostr_npriv(&self) -> Result<String> {
        Ok(self.get_nostr_keys().secret_key().to_bech32()?)
    }

    /// Converts the keypair to a libp2p keypair
    pub fn get_libp2p_keys(&self) -> Result<libp2p::identity::Keypair> {
        as_libp2p_keypair(&self.inner)
    }
}

/// libp2p uses a different secp256k1 library than bitcoin and nostr. This
/// function converts the key via priv key bytes and returns the keypair as a
/// libp2p keypair.
fn as_libp2p_keypair(keypair: &Keypair) -> Result<libp2p::identity::Keypair> {
    let secret: libp2p::identity::secp256k1::SecretKey =
        libp2p::identity::secp256k1::SecretKey::try_from_bytes(
            keypair.secret_key().secret_bytes(),
        )?;

    Ok(libp2p::identity::Keypair::from(
        libp2p::identity::secp256k1::Keypair::from(secret),
    ))
}

/// Generates a new keypair using the secp256k1 library
fn generate_keypair() -> Keypair {
    let secp = Secp256k1::new();
    Keypair::new(&secp, &mut rand::thread_rng())
}

/// Loads a secp256k1 keypair from a private key string
fn load_keypair(private_key: &str) -> Result<Keypair> {
    let secp = Secp256k1::new();
    let pair = Keypair::from_secret_key(&secp, &SecretKey::from_str(private_key)?);
    Ok(pair)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKEY: &str = "926a7ce0fdacad199307bcbbcda4869bca84d54b939011bafe6a83cb194130d3";

    #[test]
    fn test_new_keypair() {
        let keypair = BcrKeys::new();
        assert!(!keypair.get_private_key_string().is_empty());
        assert!(!keypair.get_public_key().is_empty());
        assert!(!keypair
            .get_bitcoin_private_key(Network::Bitcoin)
            .to_string()
            .is_empty());
        assert!(keypair.get_nostr_keys().public_key().to_bech32().is_ok());
        assert!(keypair.get_nostr_npriv().is_ok());
    }

    #[test]
    fn test_load_keypair() {
        let keypair = BcrKeys::from_private_key(PKEY).unwrap();
        let keypair2 = BcrKeys::from_private_key(PKEY).unwrap();
        assert_eq!(
            keypair.get_private_key_string(),
            keypair2.get_private_key_string()
        );
        assert_eq!(keypair.get_public_key(), keypair2.get_public_key());
        assert_eq!(
            keypair.get_bitcoin_private_key(Network::Bitcoin),
            keypair2.get_bitcoin_private_key(Network::Bitcoin)
        );
        assert_eq!(keypair.get_nostr_keys(), keypair2.get_nostr_keys());
        assert_eq!(
            keypair.get_nostr_npub().unwrap(),
            keypair2.get_nostr_npub().unwrap()
        );
        assert_eq!(
            keypair.get_nostr_npriv().unwrap(),
            keypair2.get_nostr_npriv().unwrap()
        );
    }

    #[test]
    fn test_convert_keypair() {
        let keypair = BcrKeys::new();
        let lp2p_keypair = keypair
            .get_libp2p_keys()
            .expect("could not convert keypair to libp2p keypair");
        let secp256k1_keypair = as_secp256k1_keypair(&lp2p_keypair)
            .expect("could not convert keypair to secp256k1 keypair");
        assert_eq!(
            keypair.get_private_key_string(),
            secp256k1_keypair.display_secret().to_string()
        );
    }

    /// reverses the conversion of a libp2p keypair to a secp256k1 keypair for testing
    fn as_secp256k1_keypair(keypair: &libp2p::identity::Keypair) -> Result<Keypair> {
        let secret = keypair.to_owned().try_into_secp256k1()?.secret().to_bytes();
        Ok(Keypair::from_secret_key(
            &Secp256k1::new(),
            &SecretKey::from_slice(&secret)?,
        ))
    }
}
