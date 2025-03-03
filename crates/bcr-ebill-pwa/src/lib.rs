#![allow(clippy::arc_with_non_send_sync)]
use bcr_ebill_api::{Config as ApiConfig, get_db_context, init};
use context::{Context, get_ctx};
use futures::{StreamExt, future::ready};
use gloo_timers::future::{IntervalStream, TimeoutFuture};
use job::run_jobs;
use log::info;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::thread_local;
use wasm_bindgen::prelude::*;

pub mod api;
mod constants;
mod context;
mod data;
mod error;
mod job;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct Config {
    #[wasm_bindgen(getter_with_clone)]
    pub bitcoin_network: String,
    #[wasm_bindgen(getter_with_clone)]
    pub nostr_relay: String,
    #[wasm_bindgen(getter_with_clone)]
    pub surreal_db_connection: String,
    #[wasm_bindgen(getter_with_clone)]
    pub data_dir: String,
    pub job_runner_initial_delay_seconds: u32,
    pub job_runner_check_interval_seconds: u32,
}

#[wasm_bindgen]
impl Config {
    #[wasm_bindgen(constructor)]
    pub fn new(
        bitcoin_network: String,
        nostr_relay: String,
        surreal_db_connection: String,
        data_dir: String,
        job_runner_initial_delay_seconds: u32,
        job_runner_check_interval_seconds: u32,
    ) -> Self {
        Self {
            bitcoin_network,
            nostr_relay,
            surreal_db_connection,
            data_dir,
            job_runner_initial_delay_seconds,
            job_runner_check_interval_seconds,
        }
    }
}

pub type Result<T> = std::result::Result<T, error::WasmError>;

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) } ;
}

#[wasm_bindgen]
pub async fn initialize_api(
    #[wasm_bindgen(unchecked_param_type = "Config")] cfg: JsValue,
) -> Result<()> {
    // init logging
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("can initialize logging");

    // init config and API
    let config: Config = serde_wasm_bindgen::from_value(cfg)?;
    let api_config = ApiConfig {
        bitcoin_network: config.bitcoin_network,
        nostr_relay: config.nostr_relay,
        surreal_db_connection: config.surreal_db_connection,
        data_dir: config.data_dir,
    };
    init(api_config.clone())?;

    // init db
    let db = get_db_context(&api_config).await?;
    let keys = db.identity_store.get_or_create_key_pair().await?;

    info!("Local node id: {:?}", keys.get_public_key());
    info!("Local npub: {:?}", keys.get_nostr_npub()?);
    info!("Local npub as hex: {:?}", keys.get_nostr_npub_as_hex());
    info!("Config: {api_config:?}");

    // init context
    let ctx = Context::new(api_config.clone(), db, &keys.get_public_key()).await?;
    CONTEXT.with(|context| {
        let mut context_ref = context.borrow_mut();
        if context_ref.is_none() {
            *context_ref = Some(ctx);
        }
    });

    // start jobs
    wasm_bindgen_futures::spawn_local(async move {
        TimeoutFuture::new(config.job_runner_initial_delay_seconds * 1000).await;
        IntervalStream::new(config.job_runner_check_interval_seconds * 1000)
            .for_each(|_| {
                info!("tick: {}", chrono::Utc::now());
                run_jobs();
                ready(())
            })
            .await;
    });

    // start nostr subscription
    wasm_bindgen_futures::spawn_local(async {
        get_ctx()
            .nostr_consumer
            .start()
            .await
            .expect("nostr consumer failed");
    });
    Ok(())
}
