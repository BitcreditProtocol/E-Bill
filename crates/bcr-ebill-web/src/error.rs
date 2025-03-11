use bcr_ebill_api::service;
use thiserror::Error;

/// Generic result type
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// all errors originating from the bcr API service layer
    #[error("Service error: {0}")]
    Service(#[from] service::Error),

    /// all errors originating from the bcr API bill service layer
    #[error("Bill Service error: {0}")]
    BillService(#[from] service::bill_service::Error),

    /// all errors originating from the bcr API notification service layer
    #[error("Bill Service error: {0}")]
    NotificationService(#[from] bcr_ebill_transport::Error),
}
