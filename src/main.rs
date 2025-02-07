use crate::constants::QUOTES_MAP_FOLDER_PATH;
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
mod job;
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

    loop {
        let (reboot_sender, mut reboot_receiver) = tokio::sync::watch::channel(false);
        if let Err(e) = start(conf.clone(), reboot_sender).await {
            error!("Error starting the application: {e}");
            break;
        }

        let reboot = *reboot_receiver.borrow_and_update();
        if reboot {
            // we need to give the os time to finish its disk operations
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
            info!("Restarting application...");
        } else {
            break;
        }
    }

    Ok(())
}

async fn start(conf: Config, reboot_sender: tokio::sync::watch::Sender<bool>) -> Result<()> {
    external::mint::init_wallet().await;

    // Initialize the database context
    let db = get_db_context(&conf).await?;

    let dht = dht::dht_main(
        &conf,
        db.bill_store.clone(),
        db.bill_blockchain_store.clone(),
        db.company_store.clone(),
        db.company_chain_store.clone(),
        db.identity_store.clone(),
        db.file_upload_store.clone(),
    )
    .await
    .expect("DHT failed to start");
    let dht_client = dht.client;

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

    let local_node_id = db.identity_store.get_key_pair().await?.get_public_key();
    let mut dht_client_clone = dht_client.clone();
    let identity_store_clone = db.identity_store.clone();
    spawn(async move {
        // These actions only make sense, if we already have created an identity
        // We do them asynchronously, in a non-failing way
        if identity_store_clone.exists().await {
            if let Err(e) = dht_client_clone.check_new_bills().await {
                error!("Error while checking for new bills: {e}");
            }

            if let Err(e) = dht_client_clone.subscribe_to_all_bills_topics().await {
                error!("Error while subscribing to bills: {e}");
            }

            if let Err(e) = dht_client_clone.put_bills_for_parties().await {
                error!("Error while putting bills for parties: {e}");
            }

            if let Err(e) = dht_client_clone.start_providing_bills().await {
                error!("Error while starting to provide bills: {e}");
            }

            if let Err(e) = dht_client_clone
                .receive_updates_for_all_bills_topics()
                .await
            {
                error!("Error while starting receive updates for bill topics: {e}");
            }

            if let Err(e) = dht_client_clone.check_companies().await {
                error!("Error while checking for new companies: {e}");
            }

            if let Err(e) = dht_client_clone.put_companies_for_signatories().await {
                error!("Error while putting companies for signatories: {e}");
            }

            if let Err(e) = dht_client_clone.start_providing_companies().await {
                error!("Error while starting to provide companies: {e}");
            }

            if let Err(e) = dht_client_clone.subscribe_to_all_companies_topics().await {
                error!("Error while subscribing to all companies: {e}");
            }
        }
    });

    let job_shutdown_receiver = dht.shutdown_sender.clone().subscribe();
    let web_server_error_shutdown_sender = dht.shutdown_sender.clone();
    let service_context = create_service_context(
        &local_node_id,
        conf.clone(),
        dht_client.clone(),
        dht.shutdown_sender,
        db,
        reboot_sender,
    )
    .await?;

    let service_context_clone = service_context.clone();
    spawn(async move { job::run(service_context_clone, job_shutdown_receiver).await });

    let nostr_handle = service_context.nostr_consumer.start().await?;

    if let Err(e) = web::rocket_main(service_context).launch().await {
        error!("Web server stopped with error: {e}, shutting down the rest of the application...");
        if let Err(e) = web_server_error_shutdown_sender.send(true) {
            error!("Error triggering shutdown signal: {e}");
        }
    }

    info!("Stopping nostr consumer...");
    nostr_handle.abort();

    info!("Waiting for application to exit...");
    // If the web server exits fast, we wait for a grace period so libp2p can finish as well
    tokio::time::sleep(std::time::Duration::from_millis(SHUTDOWN_GRACE_PERIOD_MS)).await;

    Ok(())
}

fn init_folders() {
    if !Path::new(QUOTES_MAP_FOLDER_PATH).exists() {
        fs::create_dir(QUOTES_MAP_FOLDER_PATH).expect("Can't create folder quotes.");
    }
}
