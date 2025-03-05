use thiserror::Error;

pub mod event;
pub mod transport;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("BlockChain error: {0}")]
    BlockChain(#[from] bcr_ebill_core::blockchain::Error),

    #[error("Nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),
}

pub use event::bill_events::{BillActionEventPayload, BillChainEventPayload};
pub use event::{Event, EventEnvelope};
pub use transport::NotificationJsonTransportApi;
