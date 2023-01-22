use std::string::ToString;

pub const IDENTITY_FOLDER_PATH: &'static str = "identity";
pub const IDENTITY_FILE_PATH: &'static str = "identity/identity";
pub const IDENTITY_PEER_ID_FILE_PATH: &'static str = "identity/peer_id";
pub const IDENTITY_ED_25529_KEYS_FILE_PATH: &'static str = "identity/ed25519_keys";
pub const BILLS_FOLDER_PATH: &'static str = "bills";
pub const BILL_VALIDITY_PERIOD: u64 = 90;
pub const BTC: &'static str = "BTC";
pub const COMPOUNDING_INTEREST_RATE_ZERO: u64 = 0;