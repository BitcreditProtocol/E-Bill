use crate::{blockchain, external, persistence, util};
use thiserror::Error;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// errors that currently return early http status code Status::NotFound
    #[error("not found")]
    NotFound,

    /// errors stemming from trying to do invalid operations
    #[error("invalid operation")]
    InvalidOperation,

    /// error returned if a bill was already accepted and is attempted to be accepted again
    #[error("Bill was already accepted")]
    BillAlreadyAccepted,

    /// error returned if a bill was already requested to accept
    #[error("Bill was already requested to accept")]
    BillAlreadyRequestedToAccept,

    /// error returned if a bill was not accepted yet
    #[error("Bill was not yet accepted")]
    BillNotAccepted,

    /// error returned if the caller of an operation is not the drawee, but would have to be for it
    /// to be valid, e.g. accepting a  bill
    #[error("Caller is not drawee")]
    CallerIsNotDrawee,

    /// error returned if the caller of an operation is not the holder, but would have to be for it
    /// to be valid, e.g. requesting payment
    #[error("Caller is not holder")]
    CallerIsNotHolder,

    /// error returned if the caller of a reject operation is not the recoursee
    #[error("Caller is not the recoursee and can't reject")]
    CallerIsNotRecoursee,

    /// error returned if the caller of a reject buy operation is not the buyer
    #[error("Caller is not the buyer and can't reject to buy")]
    CallerIsNotBuyer,

    /// error returned if the caller of a reject operation trys to reject a request that is already
    /// expired
    #[error("The request already expired")]
    RequestAlreadyExpired,

    /// error returned if the operation was already rejected
    #[error("The request was already rejected")]
    RequestAlreadyRejected,

    /// error returned if the bill was already paid and hence can't be rejected to be paid
    #[error("The bill was already paid")]
    BillAlreadyPaid,

    /// error returned if the bill was not requested to accept, e.g. when rejecting to accept
    #[error("Bill was not requested to accept")]
    BillWasNotRequestedToAccept,

    /// error returned if the bill was not requested to pay, e.g. when rejecting to pay
    #[error("Bill was not requested to pay")]
    BillWasNotRequestedToPay,

    /// error returned if the bill was not offered to sell, e.g. when rejecting to buy
    #[error("Bill was not offered to sell")]
    BillWasNotOfferedToSell,

    /// error returned someone wants to request acceptance recourse, but the request to accept did
    /// not expire and was not rejected
    #[error("Bill request to accept did not expire and was not rejected")]
    BillRequestToAcceptDidNotExpireAndWasNotRejected,

    /// error returned someone wants to request payment recourse, but the request to pay did
    /// not expire and was not rejected
    #[error("Bill request to pay did not expire and was not rejected")]
    BillRequestToPayDidNotExpireAndWasNotRejected,

    /// error returned if the given recoursee is not a past holder of the bill
    #[error("The given recoursee is not a past holder of the bill")]
    RecourseeNotPastHolder,

    /// error returned if the bill was not requester to recourse, e.g. when rejecting to pay for
    /// recourse
    #[error("Bill was not requested to recourse")]
    BillWasNotRequestedToRecourse,

    /// error returned if the bill is not requested to recourse and is waiting for payment
    #[error("Bill is not waiting for recourse payment")]
    BillIsNotRequestedToRecourseAndWaitingForPayment,

    /// error returned if the bill is not currently an offer to sell waiting for payment
    #[error("Bill is not offer to sell waiting for payment")]
    BillIsNotOfferToSellWaitingForPayment,

    /// error returned if the selling data of selling a bill does not match the waited for offer to
    /// sell
    #[error("Sell data does not match offer to sell")]
    BillSellDataInvalid,

    /// error returned if the selling data of recoursing a bill does not match the request to
    /// recourse
    #[error("Recourse data does not match request to recourse")]
    BillRecourseDataInvalid,

    /// error returned if the bill is requested to pay and waiting for payment
    #[error("Bill is requested to pay and waiting for payment")]
    BillIsRequestedToPayAndWaitingForPayment,

    /// error returned if the bill is offered to sell and waiting for payment
    #[error("Bill is offered to sell and waiting for payment")]
    BillIsOfferedToSellAndWaitingForPayment,

    /// error returned if the bill is in recourse and waiting for payment
    #[error("Bill is in recourse and waiting for payment")]
    BillIsInRecourseAndWaitingForPayment,

    /// error returned if the bill is requested to pay
    #[error("Bill is requested to pay")]
    BillIsRequestedToPay,

    /// error returned if the given file upload id is not a temp file we have
    #[error("No file found for file upload id")]
    NoFileForFileUploadId,

    /// errors that stem from interacting with a blockchain
    #[error("Blockchain error: {0}")]
    Blockchain(#[from] blockchain::Error),

    /// all errors originating from the persistence layer
    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),

    /// all errors originating from external APIs
    #[error("External API error: {0}")]
    ExternalApi(#[from] external::Error),

    /// Errors stemming from cryptography, such as converting keys, encryption and decryption
    #[error("Cryptography error: {0}")]
    Cryptography(#[from] util::crypto::Error),

    #[error("Notification error: {0}")]
    Notification(#[from] bcr_ebill_transport::Error),

    #[error("io error {0}")]
    Io(#[from] std::io::Error),
}
