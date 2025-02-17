use std::str::FromStr;

use super::{base58_decode, base58_encode};
use bip39::Mnemonic;
use bitcoin::{
    secp256k1::{self, schnorr::Signature, Keypair, Scalar, SecretKey},
    Network,
};
use libp2p::PeerId;
use nostr_sdk::ToBech32;
use secp256k1::{rand, Message, PublicKey, Secp256k1};
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

    #[error("Ecies encryption error: {0}")]
    Ecies(String),

    #[error("Signature had invalid length")]
    InvalidSignatureLength,

    #[error("Aggregated signature needs at least 2 keys")]
    TooFewKeys,

    /// Errors stemming from decoding base58
    #[error("Decode base58 error: {0}")]
    Decode(#[from] super::Error),

    /// Errors stemming from parsing the recovery id
    #[error("Parse recovery id error: {0}")]
    ParseRecoveryId(#[from] std::num::ParseIntError),

    #[error("Mnemonic seed phrase error {0}")]
    Mnemonic(#[from] bip39::Error),
}

// -------------------- Keypair --------------------------

/// A wrapper around the secp256k1 keypair that can be used for
/// Bitcoin and Nostr keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BcrKeys {
    inner: Keypair,
}

#[allow(dead_code)]
impl BcrKeys {
    /// Generates a fresh random keypair that can be used for
    /// Bitcoin and Nostr keys.
    pub fn new() -> Self {
        Self {
            inner: generate_keypair(),
        }
    }

    /// Generates a fresh random keypair including a seed phrase that
    /// can be used to recover the private_key
    pub fn new_with_seed_phrase() -> Result<(Self, String)> {
        let (seed_phrase, keypair) = generate_keypair_from_seed_phrase()?;
        let keys = Self { inner: keypair };
        Ok((keys, seed_phrase.to_string()))
    }

    /// Recovers keys from a given seedphrase
    pub fn from_seedphrase(seed: &str) -> Result<Self> {
        let recovered_keys = keypair_from_seed_phrase(seed)?;
        Ok(Self {
            inner: recovered_keys,
        })
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

    /// Returns the nostr public key as an XOnlyPublicKey hex string
    pub fn get_nostr_npub_as_hex(&self) -> String {
        self.get_nostr_keys().public_key().to_hex()
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

    /// Checks if the given libp2p PeerId was created from this key
    pub fn is_peer_id_from_this_key(&self, peer_id: &PeerId) -> bool {
        let libp2p_keypair = match self.get_libp2p_keys() {
            Err(_) => return false,
            Ok(keypair) => keypair,
        };
        peer_id
            .is_public_key(&libp2p_keypair.public())
            .unwrap_or(false)
    }
}

pub fn validate_pub_key(pub_key: &str) -> Result<()> {
    match PublicKey::from_str(pub_key) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Number of words to use when generating BIP39 seed phrases
const BIP39_WORD_COUNT: usize = 12;

/// Calculates the XOnlyPublicKey as hex from the given node_id to be used as the npub as hex for
/// nostr
pub fn get_nostr_npub_as_hex_from_node_id(node_id: &str) -> Result<String> {
    Ok(PublicKey::from_str(node_id)?
        .x_only_public_key()
        .0
        .to_string())
}

/// Checks if the given node_id and the given npub (as hex) are the same public key.
/// This converts the node_id to an XOnlyPublicKey (which is the way nostr saves it's public key)
/// and compares it to the given npub
pub fn is_node_id_nostr_hex_npub(node_id: &str, npub: &str) -> bool {
    let x_only_pub_key = match get_nostr_npub_as_hex_from_node_id(node_id) {
        Ok(pub_key) => pub_key,
        Err(_) => return false,
    };
    match nostr_sdk::PublicKey::from_hex(&x_only_pub_key) {
        Ok(npub_from_node_id) => npub == npub_from_node_id.to_hex(),
        Err(_) => false,
    }
}

pub fn is_peer_id_from_this_node_id(node_id: &str, peer_id: &PeerId) -> bool {
    let public_key = match PublicKey::from_str(node_id) {
        Err(_) => return false,

        Ok(pub_key) => pub_key,
    };
    let libp2p_secp_pub_key = match libp2p::identity::secp256k1::PublicKey::try_from_bytes(
        public_key.serialize().as_slice(),
    ) {
        Err(_) => return false,

        Ok(pub_key) => pub_key,
    };
    let libp2p_pub_key = libp2p::identity::PublicKey::from(libp2p_secp_pub_key);
    peer_id.is_public_key(&libp2p_pub_key).unwrap_or(false)
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

// -------------------- Aggregated Signatures --------------------------

/// Returns the aggregated public key for the given private keys
pub fn get_aggregated_public_key(private_keys: &[String]) -> Result<String> {
    if private_keys.len() < 2 {
        return Err(Error::TooFewKeys);
    }

    let key_pairs: Vec<Keypair> = private_keys
        .iter()
        .map(|pk| load_keypair(pk))
        .collect::<Result<Vec<Keypair>>>()?;
    let public_keys: Vec<PublicKey> = key_pairs.into_iter().map(|kp| kp.public_key()).collect();

    let first_key = public_keys.first().ok_or(Error::TooFewKeys)?;
    let mut aggregated_key: PublicKey = first_key.to_owned();
    for key in public_keys.iter().skip(1) {
        aggregated_key = aggregated_key.combine(key)?;
    }
    Ok(aggregated_key.to_string())
}

/// The keys need to be in the correct order (identity -> company -> bill) to get the same
/// signature for the same keys
/// Public keys can be aggregated regardless of order
/// Returns the aggregated signature
pub fn aggregated_signature(hash: &str, keys: &[String]) -> Result<String> {
    if keys.len() < 2 {
        return Err(Error::TooFewKeys);
    }
    let secp = Secp256k1::signing_only();
    let key_pairs: Vec<Keypair> = keys
        .iter()
        .map(|pk| load_keypair(pk))
        .collect::<Result<Vec<Keypair>>>()?;
    let secret_keys: Vec<SecretKey> = key_pairs.into_iter().map(|kp| kp.secret_key()).collect();

    let first_key = secret_keys.first().ok_or(Error::TooFewKeys)?;
    let mut aggregated_key: SecretKey = first_key.to_owned();
    for key in secret_keys.iter().skip(1) {
        aggregated_key = aggregated_key.add_tweak(&Scalar::from(*key))?;
    }

    let aggregated_key_pair = Keypair::from_secret_key(&secp, &aggregated_key);
    let msg = Message::from_digest_slice(&base58_decode(hash)?)?;
    let signature = secp.sign_schnorr(&msg, &aggregated_key_pair);

    Ok(base58_encode(&signature.serialize()))
}

// -------------------- Signatures --------------------------

pub fn signature(hash: &str, private_key: &str) -> Result<String> {
    // create a signing context
    let secp = Secp256k1::signing_only();
    let key_pair = load_keypair(private_key)?;
    let msg = Message::from_digest_slice(&base58_decode(hash)?)?;
    let signature = secp.sign_schnorr(&msg, &key_pair);
    Ok(base58_encode(&signature.serialize()))
}

pub fn verify(hash: &str, signature: &str, public_key: &str) -> Result<bool> {
    // create a verification context
    let secp = Secp256k1::verification_only();
    let (pub_key, _) = PublicKey::from_str(public_key)?.x_only_public_key();
    let msg = Message::from_digest_slice(&base58_decode(hash)?)?;
    let decoded_signature = Signature::from_slice(&base58_decode(signature)?)?;
    Ok(secp
        .verify_schnorr(&decoded_signature, &msg, &pub_key)
        .is_ok())
}

// -------------------- Encryption --------------------------

/// Encrypt the given bytes with the given Secp256k1 key via ECIES
pub fn encrypt_ecies(bytes: &[u8], public_key: &str) -> Result<Vec<u8>> {
    let pub_key_parsed = PublicKey::from_str(public_key)?;
    let pub_key_bytes = pub_key_parsed.serialize();
    let encrypted =
        ecies::encrypt(pub_key_bytes.as_slice(), bytes).map_err(|e| Error::Ecies(e.to_string()))?;
    Ok(encrypted)
}

/// Decrypt the given bytes with the given Secp256k1 key via ECIES
pub fn decrypt_ecies(bytes: &[u8], private_key: &str) -> Result<Vec<u8>> {
    let keypair = BcrKeys::from_private_key(private_key)?;
    let decrypted = ecies::decrypt(keypair.inner.secret_bytes().as_slice(), bytes)
        .map_err(|e| Error::Ecies(e.to_string()))?;
    Ok(decrypted)
}

// ------------------------ BIP39 ---------------------------

/// Generate a new secp256k1 keypair using a 12 word seed phrase.
/// Returns both the keypair and the Mnemonic with the seed phrase.
fn generate_keypair_from_seed_phrase() -> Result<(Mnemonic, Keypair)> {
    let mnemonic = Mnemonic::generate(BIP39_WORD_COUNT)?;
    let keypair = keypair_from_mnemonic(&mnemonic)?;
    Ok((mnemonic, keypair))
}

/// Recover a key pair from a BIP39 seed phrase. Word count
/// and language are detected automatically.
fn keypair_from_seed_phrase(words: &str) -> Result<Keypair> {
    let mnemonic = Mnemonic::from_str(words)?;
    keypair_from_mnemonic(&mnemonic)
}

/// Recover a key pair from a BIP39 mnemonic.
fn keypair_from_mnemonic(mnemonic: &Mnemonic) -> Result<Keypair> {
    let seed = mnemonic.to_seed("");
    let (key, _) = seed.split_at(32);
    let secp = Secp256k1::new();
    let secret = SecretKey::from_slice(key)?;
    Ok(Keypair::from_secret_key(&secp, &secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        service::company_service::tests::get_baseline_company_data,
        tests::tests::{TEST_NODE_ID_SECP, TEST_NODE_ID_SECP_AS_NPUB_HEX},
        util,
    };
    use borsh::to_vec;

    const PKEY: &str = "926a7ce0fdacad199307bcbbcda4869bca84d54b939011bafe6a83cb194130d3";

    #[test]
    fn test_generate_keypair_and_seed_phrase_round_trip() {
        let (mnemonic, keypair) = generate_keypair_from_seed_phrase()
            .expect("Could not generate keypair and seed phrase");
        let recovered_keys = keypair_from_seed_phrase(&mnemonic.to_string())
            .expect("Could not recover private key from seed phrase");
        assert_eq!(keypair.secret_key(), recovered_keys.secret_key());
    }

    #[test]
    fn test_recover_keypair_from_seed_phrase_24_words() {
        // a valid pair of 24 words to priv key
        let words = "forward paper connect economy twelve debate cart isolate accident creek bind predict captain rifle glory cradle hip whisper wealth save buddy place develop dolphin";
        let priv_key = "f31e0373f6fa9f4835d49a278cd48f47ea115af7480edf435275a3c2dbb1f982";
        let keypair =
            keypair_from_seed_phrase(words).expect("Could not create keypair from seed phrase");
        let returned_priv_key = keypair.secret_key().display_secret().to_string();
        assert_eq!(priv_key, returned_priv_key);
    }

    #[test]
    fn test_recover_keypair_from_seed_phrase_12_words() {
        // a valid pair of 12 words to priv key
        let words = "oblige repair kind park dust act name myth cheap treat hammer arrive";
        let priv_key = "92f920d8e183cab62723c3a7eee9cb0b3edb3c4aad459f4062cfb7960b570662";
        let keypair =
            keypair_from_seed_phrase(words).expect("Could not create keypair from seed phrase");
        let returned_priv_key = keypair.secret_key().display_secret().to_string();
        assert_eq!(priv_key, returned_priv_key);
    }

    #[test]
    fn test_sign_verify() {
        let keypair = BcrKeys::new();
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let signature = signature(&hash, &keypair.get_private_key_string()).unwrap();
        assert!(verify(&hash, &signature, &keypair.get_public_key()).is_ok());
        assert!(verify(&hash, &signature, &keypair.get_public_key()).unwrap());
    }

    #[test]
    fn test_sign_verify_invalid() {
        let keypair = BcrKeys::new();
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let signature = signature(&hash, &keypair.get_private_key_string()).unwrap();
        let hash2 = util::sha256_hash("Hello, Changed Changed Changed World".as_bytes());
        assert!(verify(&hash, &signature, &keypair.get_public_key()).is_ok());
        assert!(verify(&hash, &signature, &keypair.get_public_key()).is_ok());
        // it fails for a different hash
        assert!(verify(&hash2, &signature, &keypair.get_public_key()).is_ok());
        assert!(!verify(&hash2, &signature, &keypair.get_public_key())
            .as_ref()
            .unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());

        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        assert!(verify(&hash, &signature, &public_key).is_ok());
        assert!(verify(&hash, &signature, &public_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_order_dependence() {
        let hash = util::sha256_hash("Hello, World".as_bytes());

        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();

        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let keys2: Vec<String> = vec![
            keypair2.get_private_key_string(),
            keypair1.get_private_key_string(),
        ];
        let public_key2 = get_aggregated_public_key(&keys2).unwrap();
        let signature2 = aggregated_signature(&hash, &keys2).unwrap();

        assert_ne!(signature, signature2); // the signatures aren't the same
        assert_eq!(public_key, public_key2); // but the public keys are

        assert!(verify(&hash, &signature, &public_key).is_ok());
        assert!(verify(&hash, &signature, &public_key).unwrap());

        assert!(verify(&hash, &signature2, &public_key2).is_ok());
        assert!(verify(&hash, &signature2, &public_key2).unwrap());

        assert!(verify(&hash, &signature, &public_key2).is_ok());
        assert!(verify(&hash, &signature, &public_key2).unwrap());

        assert!(verify(&hash, &signature2, &public_key).is_ok());
        assert!(verify(&hash, &signature2, &public_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_invalid() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());

        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let changed_hash = util::sha256_hash("Hello Hello, World".as_bytes());
        assert!(verify(&changed_hash, &signature, &public_key).is_ok());
        assert!(!verify(&changed_hash, &signature, &public_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_invalid_only_one_pubkey() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let signature = aggregated_signature(&hash, &keys).unwrap();
        assert!(verify(&hash, &signature, &keypair2.get_public_key()).is_ok());
        assert!(!verify(&hash, &signature, &keypair2.get_public_key()).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_multiple() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keypair3 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
            keypair3.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();
        assert!(verify(&hash, &signature, &public_key).is_ok());
        assert!(verify(&hash, &signature, &public_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_multiple_manually_combined_pubkeys_for_verify() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keypair3 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
            keypair3.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let combined_pub_key = keypair1
            .inner
            .public_key()
            .combine(&keypair2.inner.public_key())
            .unwrap()
            .combine(&keypair3.inner.public_key())
            .unwrap()
            .to_string();
        assert_eq!(public_key, combined_pub_key);
        assert!(verify(&hash, &signature, &combined_pub_key).is_ok());
        assert!(verify(&hash, &signature, &combined_pub_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_manually_combined_pubkeys_for_verify() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let combined_pub_key = keypair1
            .inner
            .public_key()
            .combine(&keypair2.inner.public_key())
            .unwrap()
            .to_string();
        assert_eq!(public_key, combined_pub_key);
        assert!(verify(&hash, &signature, &combined_pub_key).is_ok());
        assert!(verify(&hash, &signature, &combined_pub_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_manually_combined_pubkeys_for_verify_different_order_also_works()
    {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let combined_pub_key = keypair2
            .inner
            .public_key()
            .combine(&keypair1.inner.public_key())
            .unwrap()
            .to_string();
        assert_eq!(public_key, combined_pub_key);
        assert!(verify(&hash, &signature, &combined_pub_key).is_ok());
        assert!(verify(&hash, &signature, &combined_pub_key).unwrap());
    }

    #[test]
    fn test_sign_verify_aggregated_manually_combined_pubkeys_for_verify_invalid_pubkey() {
        let keypair1 = BcrKeys::new();
        let keypair2 = BcrKeys::new();
        let keypair3 = BcrKeys::new();
        let keys: Vec<String> = vec![
            keypair1.get_private_key_string(),
            keypair2.get_private_key_string(),
        ];
        let hash = util::sha256_hash("Hello, World".as_bytes());
        let public_key = get_aggregated_public_key(&keys).unwrap();
        let signature = aggregated_signature(&hash, &keys).unwrap();

        let combined_pub_key = keypair1
            .inner
            .public_key()
            .combine(&keypair3.inner.public_key())
            .unwrap()
            .to_string();
        assert_ne!(public_key, combined_pub_key);
        assert!(verify(&hash, &signature, &combined_pub_key).is_ok());
        assert!(!verify(&hash, &signature, &combined_pub_key).unwrap());
    }

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

    #[test]
    fn test_check_libp2p_peer_id_against_keypair() {
        let keypair = BcrKeys::new();
        let peer_id = keypair.get_libp2p_keys().unwrap().public().to_peer_id();

        assert!(keypair.is_peer_id_from_this_key(&peer_id));
    }

    #[test]
    fn test_check_libp2p_peer_id_against_keypair_invalid() {
        let keypair = BcrKeys::new();
        let keypair_other = BcrKeys::new();
        let another_key_pair = BcrKeys::from_private_key(
            "8863c82829480536893fc49c4b30e244f97261e989433373d73c648c1a656a79",
        )
        .unwrap();
        let peer_id = keypair.get_libp2p_keys().unwrap().public().to_peer_id();
        let other_peer_id =
            PeerId::from_str("16Uiu2HAkxDBLq6DXuKA9vacbxQ9TgsP2Th9GmzkY44cA2vDyn7Eo").unwrap();
        let another_peer_id =
            PeerId::from_str("16Uiu2HAmEqJNJbkTVX9TLtcya4p5mhUL3E8rZq2VjitE7HZtuWCE").unwrap();

        assert!(!keypair_other.is_peer_id_from_this_key(&peer_id));
        assert!(!keypair.is_peer_id_from_this_key(&other_peer_id));
        assert!(keypair.is_peer_id_from_this_key(&peer_id));
        assert!(another_key_pair.is_peer_id_from_this_key(&another_peer_id));
        assert!(!another_key_pair.is_peer_id_from_this_key(&peer_id));
        assert!(!another_key_pair.is_peer_id_from_this_key(&other_peer_id));
    }

    #[test]
    fn test_check_libp2p_peer_id_against_node_id() {
        let keypair = BcrKeys::new();
        let node_id = keypair.get_public_key();
        let peer_id = keypair.get_libp2p_keys().unwrap().public().to_peer_id();

        assert!(is_peer_id_from_this_node_id(&node_id, &peer_id));
    }

    #[test]
    fn test_check_libp2p_peer_id_against_node_id_hardcoded_values() {
        assert!(is_peer_id_from_this_node_id(
            "03205b8dec12bc9e879f5b517aa32192a2550e88adcee3e54ec2c7294802568fef",
            &PeerId::from_str("16Uiu2HAmEqJNJbkTVX9TLtcya4p5mhUL3E8rZq2VjitE7HZtuWCE").unwrap()
        ));
        assert!(!is_peer_id_from_this_node_id(
            "03205b8dec12bc9e879f5b517aa32192a2550e88adcee3e54ec2c7294802568fef",
            &PeerId::from_str("16Uiu2HAkxDBLq6DXuKA9vacbxQ9TgsP2Th9GmzkY44cA2vDyn7Eo").unwrap()
        ));
    }

    #[test]
    fn test_check_libp2p_peer_id_against_node_id_invalid() {
        let keypair = BcrKeys::new();
        let keypair_other = BcrKeys::new();
        let node_id = keypair.get_public_key();
        let peer_id = keypair.get_libp2p_keys().unwrap().public().to_peer_id();
        let keypair_other_peer_id = keypair_other
            .get_libp2p_keys()
            .unwrap()
            .public()
            .to_peer_id();

        let other_peer_id =
            PeerId::from_str("16Uiu2HAkxDBLq6DXuKA9vacbxQ9TgsP2Th9GmzkY44cA2vDyn7Eo").unwrap();
        let another_peer_id =
            PeerId::from_str("16Uiu2HAmEqJNJbkTVX9TLtcya4p5mhUL3E8rZq2VjitE7HZtuWCE").unwrap();

        assert!(is_peer_id_from_this_node_id(&node_id, &peer_id));
        assert!(!is_peer_id_from_this_node_id(&node_id, &other_peer_id));
        assert!(!is_peer_id_from_this_node_id(&node_id, &another_peer_id));
        assert!(!is_peer_id_from_this_node_id(
            &node_id,
            &keypair_other_peer_id
        ));
        assert!(!is_peer_id_from_this_node_id("nonsense", &other_peer_id));
    }

    #[test]
    fn encrypt_decrypt_ecies_basic() {
        let msg = "Hello, this is a very important message!"
            .to_string()
            .into_bytes();
        let keypair = BcrKeys::new();

        let encrypted = encrypt_ecies(&msg, &keypair.get_public_key());
        assert!(encrypted.is_ok());
        let decrypted = decrypt_ecies(
            encrypted.as_ref().unwrap(),
            &keypair.get_private_key_string(),
        );
        assert!(decrypted.is_ok());

        assert_eq!(&msg, decrypted.as_ref().unwrap());
    }

    #[test]
    fn encrypt_decrypt_ecies_hardcoded_creds() {
        let msg = "Important!".to_string().into_bytes();

        let encrypted = encrypt_ecies(
            &msg,
            "03205b8dec12bc9e879f5b517aa32192a2550e88adcee3e54ec2c7294802568fef",
        );
        assert!(encrypted.is_ok());
        let decrypted = decrypt_ecies(
            encrypted.as_ref().unwrap(),
            "8863c82829480536893fc49c4b30e244f97261e989433373d73c648c1a656a79",
        );
        assert!(decrypted.is_ok());

        assert_eq!(&msg, decrypted.as_ref().unwrap());
    }

    #[test]
    fn encrypt_decrypt_ecies_big_data() {
        let company_data = get_baseline_company_data();
        let mut companies = vec![];
        for _ in 0..100 {
            companies.push(company_data.clone());
        }
        let companies_bytes = to_vec(&companies).unwrap();
        let keypair = BcrKeys::new();

        let encrypted = encrypt_ecies(&companies_bytes, &keypair.get_public_key());
        assert!(encrypted.is_ok());
        let decrypted = decrypt_ecies(
            encrypted.as_ref().unwrap(),
            &keypair.get_private_key_string(),
        );
        assert!(decrypted.is_ok());

        assert_eq!(&companies_bytes, decrypted.as_ref().unwrap());
    }

    #[test]
    fn get_nostr_npub_as_hex_from_node_id_base() {
        let node_id = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";
        let npub_as_hex = "39a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";

        assert_eq!(
            get_nostr_npub_as_hex_from_node_id(node_id).unwrap(),
            npub_as_hex.to_string()
        );
        let npub =
            nostr_sdk::PublicKey::from_hex(&get_nostr_npub_as_hex_from_node_id(node_id).unwrap())
                .unwrap();
        assert_eq!(npub.to_hex(), npub_as_hex);
        assert_eq!(
            get_nostr_npub_as_hex_from_node_id(TEST_NODE_ID_SECP).unwrap(),
            TEST_NODE_ID_SECP_AS_NPUB_HEX.to_string()
        );
        let npub = nostr_sdk::PublicKey::from_hex(
            &get_nostr_npub_as_hex_from_node_id(TEST_NODE_ID_SECP).unwrap(),
        )
        .unwrap();
        assert_eq!(npub.to_hex(), TEST_NODE_ID_SECP_AS_NPUB_HEX);
    }

    #[test]
    fn get_nostr_npub_as_hex_from_node_id_invalid() {
        let node_id = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";
        let npub_as_hex = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";

        assert_ne!(
            get_nostr_npub_as_hex_from_node_id(node_id).unwrap(),
            npub_as_hex.to_string()
        );
        assert_ne!(
            get_nostr_npub_as_hex_from_node_id(node_id).unwrap(),
            TEST_NODE_ID_SECP_AS_NPUB_HEX.to_string()
        );
    }

    #[test]
    fn is_node_id_nostr_hex_npub_base() {
        let node_id = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";
        let npub_as_hex = "39a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";

        assert!(is_node_id_nostr_hex_npub(node_id, npub_as_hex));
        assert!(is_node_id_nostr_hex_npub(
            TEST_NODE_ID_SECP,
            TEST_NODE_ID_SECP_AS_NPUB_HEX
        ));
    }

    #[test]
    fn is_node_id_nostr_hex_npub_neg() {
        let node_id = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";
        let npub_as_hex = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";

        assert!(!is_node_id_nostr_hex_npub(node_id, npub_as_hex));

        let node_id = "0239a02d7aa976f4ef69173c271926d15fbff71e5b7d9e1adbb37fac2f3a370a70";

        assert!(!is_node_id_nostr_hex_npub(
            node_id,
            TEST_NODE_ID_SECP_AS_NPUB_HEX
        ));
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
