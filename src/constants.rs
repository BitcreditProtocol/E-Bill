use std::net::Ipv4Addr;

// General
pub const BILLS_PREFIX: &str = "BILLS";
pub const BILL_PREFIX: &str = "BILL";
pub const BILL_ATTACHMENT_PREFIX: &str = "BILLATT";
pub const KEY_PREFIX: &str = "KEY";
pub const COMPANIES_PREFIX: &str = "COMPANIES";
pub const COMPANY_PREFIX: &str = "COMPANY";
pub const COMPANY_KEY_PREFIX: &str = "COMPANYKEY";
pub const COMPANY_LOGO_PREFIX: &str = "COMPANYLOGO";
pub const COMPANY_PROOF_PREFIX: &str = "COMPANYPROOF";
pub const IDENTITY_PREFIX: &str = "IDENTITY";
pub const SHUTDOWN_GRACE_PERIOD_MS: u64 = 1500;

// Validation
pub const MAX_FILE_SIZE_BYTES: usize = 1_000_000; // ~1 MB
pub const MAX_FILE_NAME_CHARACTERS: usize = 50;
pub const VALID_FILE_MIME_TYPES: [&str; 3] = ["image/jpeg", "image/png", "application/pdf"];

// Paths
pub const QUOTES_MAP_FOLDER_PATH: &str = "quotes";
pub const BOOTSTRAP_FOLDER_PATH: &str = "bootstrap";
pub const QUOTE_MAP_FILE_PATH: &str = "quotes/quotes";
pub const BOOTSTRAP_NODES_FILE_PATH: &str = "bootstrap/bootstrap_nodes.json";

pub const COMPOUNDING_INTEREST_RATE_ZERO: u64 = 0;
// pub const BILL_VALIDITY_PERIOD: u64 = 90;

// Relay
pub const RELAY_BOOTSTRAP_NODE_ONE_IP: Ipv4Addr = Ipv4Addr::new(45, 147, 248, 87);
pub const RELAY_BOOTSTRAP_NODE_ONE_TCP: u16 = 1908;
pub const RELAY_BOOTSTRAP_NODE_ONE_PEER_ID: &str =
    "12D3KooWFvRxAazxdKVB7SsTtcLTnvmF8brtW2kQRhceohtgcJv2";

// Bill Data constants
pub const SIGNED_BY: &str = "Signed by ";
pub const ENDORSED_TO: &str = "Endorsed to ";
pub const ENDORSED_BY: &str = " endorsed by ";
pub const REQ_TO_ACCEPT_BY: &str = "Requested to accept by ";
pub const REQ_TO_PAY_BY: &str = "Requested to pay by ";
pub const ACCEPTED_BY: &str = "Accepted by ";
pub const SOLD_TO: &str = "Sold to ";
pub const SOLD_BY: &str = " sold by ";
pub const AMOUNT: &str = " amount: ";
