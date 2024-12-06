pub mod bitcoin;
pub mod mint;
pub mod time;

use thiserror::Error;

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// all errors originating from the external time API
    #[error("External Time API error: {0}")]
    ExternalTimeApi(#[from] reqwest::Error),

    /// all errors originating from the external bitcoin API
    #[error("External Bitcoin API error: {0}")]
    ExternalBitcoinApi(#[from] bitcoin::Error),
}
