use bitcoin::Network;
use std::net::Ipv4Addr;

pub const IDENTITY_FOLDER_PATH: &str = "identity";
pub const BILLS_FOLDER_PATH: &str = "bills";
pub const BILLS_KEYS_FOLDER_PATH: &str = "bills_keys";
pub const QUOTES_MAP_FOLDER_PATH: &str = "quotes";
pub const BOOTSTRAP_FOLDER_PATH: &str = "bootstrap";
pub const IDENTITY_FILE_PATH: &str = "identity/identity";
pub const IDENTITY_PEER_ID_FILE_PATH: &str = "identity/peer_id";
pub const IDENTITY_ED_25529_KEYS_FILE_PATH: &str = "identity/ed25519_keys";
pub const QUOTE_MAP_FILE_PATH: &str = "quotes/quotes";
pub const BOOTSTRAP_NODES_FILE_PATH: &str = "bootstrap/bootstrap_nodes.json";
pub const BTC: &str = "BTC";
pub const SATOSHI: &str = "sat";
pub const BILLS_PREFIX: &str = "BILLS";
pub const TESTNET: Network = Network::Testnet;
pub const MAINNET: Network = Network::Bitcoin;
pub const USEDNET: Network = TESTNET;
pub const BILL_VALIDITY_PERIOD: u64 = 90;
pub const NUMBER_SATOSHI_IN_BTC: u64 = 100000000;
pub const COMPOUNDING_INTEREST_RATE_ZERO: u64 = 0;
//NODE ONE /ip4/45.147.248.87/tcp/1908/p2p/12D3KooWFvRxAazxdKVB7SsTtcLTnvmF8brtW2kQRhceohtgcJv2
pub const RELAY_BOOTSTRAP_NODE_ONE_IP: Ipv4Addr = Ipv4Addr::new(45, 147, 248, 87);
pub const RELAY_BOOTSTRAP_NODE_ONE_TCP: u16 = 1908;
pub const RELAY_BOOTSTRAP_NODE_ONE_PEER_ID: &str =
    "12D3KooWFvRxAazxdKVB7SsTtcLTnvmF8brtW2kQRhceohtgcJv2";
