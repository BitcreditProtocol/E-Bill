use bcr_ebill_core::util;
use thiserror::Error;

pub mod email;
pub mod event;
pub mod handler;
pub mod notification_service;
pub mod push_notification;
pub mod transport;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("BlockChain error: {0}")]
    BlockChain(#[from] bcr_ebill_core::blockchain::Error),

    #[error("Persistence error: {0}")]
    Persistence(#[from] bcr_ebill_persistence::Error),

    #[error("Nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),

    #[error("Invalid node id error: {0}")]
    InvalidNodeId(String),

    /// some transports require a http client where we use reqwest
    #[error("http client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    #[error("nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),

    #[error("crypto util error: {0}")]
    CryptoUtil(#[from] util::crypto::Error),

    #[error("notification service contact error: {0}")]
    ContactError(String),
}

pub use event::bill_events::{BillActionEventPayload, BillChainEventPayload};
pub use event::chain_event::BillChainEvent;
pub use event::{Event, EventEnvelope};
pub use notification_service::NotificationServiceApi;
pub use push_notification::{PushApi, PushService};
pub use transport::NotificationJsonTransportApi;
