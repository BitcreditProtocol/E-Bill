use std::net::Ipv4Addr;

use anyhow::Result;
use clap::Parser;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;

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
    #[arg(default_value_t = String::from("./"), long, env = "DATA_DIR")]
    pub data_dir: String,
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
}
