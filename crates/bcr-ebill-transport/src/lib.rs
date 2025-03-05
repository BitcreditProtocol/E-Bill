use thiserror::Error;

pub mod event;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("BlockChain error: {0}")]
    BlockChainError(#[from] bcr_ebill_core::blockchain::Error),
}
