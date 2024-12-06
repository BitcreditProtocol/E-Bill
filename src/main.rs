use crate::constants::{BOOTSTRAP_FOLDER_PATH, QUOTES_MAP_FOLDER_PATH};
use anyhow::Result;
use clap::Parser;
use config::Config;
use constants::SHUTDOWN_GRACE_PERIOD_MS;
use log::{error, info};
use persistence::get_db_context;
use service::create_service_context;
use std::path::Path;
use std::{env, fs};
use tokio::spawn;
mod blockchain;
mod config;
mod constants;
mod dht;
mod error;
mod external;
mod persistence;
mod service;
#[cfg(test)]
mod tests;
mod util;
mod web;

// MAIN
#[macro_use]
extern crate lazy_static;
lazy_static! {
    pub static ref CONFIG: Config = Config::parse();
}

#[tokio::main]
async fn main() -> Result<()> {
    env::set_var("RUST_BACKTRACE", "full");

    env_logger::init();

    info!("Chosen Network: {:?}", CONFIG.bitcoin_network());

    let conf = CONFIG.clone();

    init_folders();

    external::mint::init_wallet().await;

    // Initialize the database context
    let db = get_db_context(&conf).await?;

    let dht = dht::dht_main(
        &conf,
        db.bill_store.clone(),
        db.company_store.clone(),
        db.identity_store.clone(),
    )
    .await
    .expect("DHT failed to start");
    let mut dht_client = dht.client;

    let ctrl_c_sender = dht.shutdown_sender.clone();
    spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("can't register ctrl-c handler");
        info!("Received SIGINT. Shutting down...");

        if let Err(e) = ctrl_c_sender.send(true) {
            error!("Error triggering shutdown signal: {e}");
        }
    });

    let local_peer_id = db.identity_store.get_peer_id().await?;
    dht_client
        .check_new_bills(local_peer_id.to_string())
        .await?;
    dht_client
        .update_bills_table(local_peer_id.to_string())
        .await?;
    dht_client.subscribe_to_all_bills_topics().await?;
    dht_client.put_bills_for_parties().await?;
    dht_client.start_providing_bills().await?;
    dht_client.receive_updates_for_all_bills_topics().await?;

    dht_client.put_identity_public_data_in_dht().await?;

    dht_client.check_companies().await?;
    dht_client.put_companies_for_signatories().await?;
    dht_client.put_companies_public_data_in_dht().await?;
    dht_client.start_providing_companies().await?;
    dht_client.subscribe_to_all_companies_topics().await?;

    let web_server_error_shutdown_sender = dht.shutdown_sender.clone();
    let service_context =
        create_service_context(conf.clone(), dht_client.clone(), dht.shutdown_sender, db).await?;

    if let Err(e) = web::rocket_main(service_context).launch().await {
        error!("Web server stopped with error: {e}, shutting down the rest of the application...");
        if let Err(e) = web_server_error_shutdown_sender.send(true) {
            error!("Error triggering shutdown signal: {e}");
        }
    }

    info!("Waiting for application to exit...");
    // If the web server exits fast, we wait for a grace period so libp2p can finish as well
    tokio::time::sleep(std::time::Duration::from_millis(SHUTDOWN_GRACE_PERIOD_MS)).await;

    Ok(())
}

fn init_folders() {
    if !Path::new(QUOTES_MAP_FOLDER_PATH).exists() {
        fs::create_dir(QUOTES_MAP_FOLDER_PATH).expect("Can't create folder quotes.");
    }
    if !Path::new(BOOTSTRAP_FOLDER_PATH).exists() {
        fs::create_dir(BOOTSTRAP_FOLDER_PATH).expect("Can't create folder bootstrap.");
    }
}
