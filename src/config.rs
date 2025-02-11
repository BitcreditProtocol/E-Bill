use anyhow::Result;
use bitcoin::Network;
use clap::Parser;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use std::net::Ipv4Addr;

/// Configuration for the bitcredit application
/// Allows to set the ports and addresses for the http and p2p connections
/// either via command line or environment variables
#[derive(Parser, Clone)]
#[command(version, about, long_about = None)]
pub struct Config {
    #[arg(default_value_t = 1908, long, env = "P2P_PORT")]
    pub p2p_port: u16,
    #[arg(default_value_t = String::from("0.0.0.0"), long, env = "P2P_ADDRESS")]
    pub p2p_address: String,
    #[arg(default_value_t = 8000, long, env = "HTTP_PORT")]
    pub http_port: u16,
    #[arg(default_value_t = String::from("127.0.0.1"), long, env = "HTTP_ADDRESS")]
    pub http_address: String,
    #[arg(default_value_t = String::from("."), long, env = "DATA_DIR")]
    pub data_dir: String,
    #[arg(default_value_t = String::from("ws://localhost:8800"), long, env = "SURREAL_DB_CONNECTION")]
    pub surreal_db_connection: String,
    #[arg(default_value_t = false, long, env = "TERMINAL_CLIENT")]
    pub terminal_client: bool,
    #[arg(default_value_t = String::from("bitcoin_network"),  env = "BITCOIN_NETWORK")]
    pub bitcoin_network: String,
    #[arg(default_value_t = String::from("ws://localhost:8080"), long, env = "NOSTR_RELAY")]
    pub nostr_relay: String,
    #[arg(default_value_t = String::from("https://moksha.minibill.tech"), long, env = "MINT_URL")]
    pub mint_url: String,
    #[arg(default_value_t = 1, long, env = "JOB_RUNNER_INITIAL_DELAY_SECONDS")]
    pub job_runner_initial_delay_seconds: u64,
    #[arg(default_value_t = 600, long, env = "JOB_RUNNER_CHECK_INTERVAL_SECONDS")]
    pub job_runner_check_interval_seconds: u64,
    #[arg(default_value_t = String::from("frontend"), long, env = "FRONTEND_SERVE_FOLDER")]
    pub frontend_serve_folder: String,
    #[arg(default_value_t = String::from("/"), long, env = "FRONTEND_URL_PATH")]
    pub frontend_url_path: String,
    #[arg(default_value_t = false, long, env = "LAUNCH_FRONTEND_AT_STARTUP")]
    pub launch_frontend_at_startup: bool,
    #[arg(default_value_t = String::from("/ip4/45.147.248.87/tcp/1908"), long, env = "RELAY_BOOTSTRAP_ADDRESS")]
    pub relay_bootstrap_address: String,
    #[arg(default_value_t = String::from("12D3KooWL5y2jyVFtk541g9ySSoKGjNf61GEPG1XbPhop5MRfyA8"), long, env = "RELAY_BOOTSTRAP_PEER_ID")]
    pub relay_bootstrap_peer_id: String,
}

impl Config {
    pub fn http_listen_url(&self) -> String {
        format!("http://{}:{}", self.http_address, self.http_port)
    }

    pub fn p2p_listen_url(&self) -> Result<Multiaddr> {
        let res = Multiaddr::empty()
            .with(self.p2p_address.parse::<Ipv4Addr>()?.into())
            .with(Protocol::Tcp(self.p2p_port));
        Ok(res)
    }

    pub fn bitcoin_network(&self) -> Network {
        match self.bitcoin_network.as_str() {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "regtest" => Network::Regtest,
            _ => Network::Testnet,
        }
    }
}
