use bcr_ebill_api::service::Error as ServiceError;
use bcr_ebill_api::service::bill_service::Error as BillServiceError;
use bcr_ebill_api::util;
use bcr_ebill_transport::Error as NotificationServiceError;
use serde::Serialize;
use thiserror::Error;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("service error: {0}")]
    Service(#[from] ServiceError),

    #[error("bill service error: {0}")]
    BillService(#[from] BillServiceError),

    #[error("notification service error: {0}")]
    NotificationService(#[from] NotificationServiceError),

    #[error("bill service error: {0}")]
    WasmSerialization(#[from] serde_wasm_bindgen::Error),

    #[error("crypto error: {0}")]
    Crypto(#[from] util::crypto::Error),

    #[error("persistence error: {0}")]
    Persistence(#[from] bcr_ebill_api::PersistenceError),

    #[error("api init error: {0}")]
    Init(#[from] anyhow::Error),
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
struct JsErrorData {
    error: &'static str,
    message: String,
    code: u16,
}

impl From<WasmError> for JsValue {
    fn from(error: WasmError) -> JsValue {
        let js_error_data = match error {
            WasmError::Service(e) => match e {
                ServiceError::NoFileForFileUploadId => JsErrorData {
                    error: "bad_request",
                    message: e.to_string(),
                    code: 400,
                },
                ServiceError::PreconditionFailed => JsErrorData {
                    error: "not_acceptable",
                    message: e.to_string(),
                    code: 406,
                },
                ServiceError::NotFound => JsErrorData {
                    error: "not_found",
                    message: e.to_string(),
                    code: 404,
                },
                ServiceError::NotificationService(e) => notification_service_error_data(e),
                ServiceError::BillService(e) => bill_service_error_data(e),
                ServiceError::Validation(msg) => JsErrorData {
                    error: "validation",
                    message: msg,
                    code: 400,
                },
                ServiceError::ExternalApi(e) => JsErrorData {
                    error: "external_api",
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Io(e) => JsErrorData {
                    error: "io",
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::CryptoUtil(e) => JsErrorData {
                    error: "crypto",
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Persistence(e) => JsErrorData {
                    error: "persistence",
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Blockchain(e) => JsErrorData {
                    error: "blockchain",
                    message: e.to_string(),
                    code: 500,
                },
            },
            WasmError::BillService(e) => bill_service_error_data(e),
            WasmError::NotificationService(e) => notification_service_error_data(e),
            WasmError::WasmSerialization(e) => JsErrorData {
                error: "wasm_serialization",
                message: e.to_string(),
                code: 500,
            },
            WasmError::Crypto(e) => JsErrorData {
                error: "crypto",
                message: e.to_string(),
                code: 500,
            },
            WasmError::Persistence(e) => JsErrorData {
                error: "persistence",
                message: e.to_string(),
                code: 500,
            },
            WasmError::Init(e) => JsErrorData {
                error: "init",
                message: e.to_string(),
                code: 500,
            },
        };
        serde_wasm_bindgen::to_value(&js_error_data).expect("can serialize error")
    }
}

fn notification_service_error_data(e: NotificationServiceError) -> JsErrorData {
    JsErrorData {
        error: "notification_service_error",
        message: e.to_string(),
        code: 500,
    }
}

fn bill_service_error_data(e: BillServiceError) -> JsErrorData {
    match e {
        BillServiceError::RequestAlreadyExpired
        | BillServiceError::BillAlreadyAccepted
        | BillServiceError::BillWasNotOfferedToSell
        | BillServiceError::BillWasNotRequestedToPay
        | BillServiceError::BillWasNotRequestedToAccept
        | BillServiceError::BillWasNotRequestedToRecourse
        | BillServiceError::BillIsNotOfferToSellWaitingForPayment
        | BillServiceError::BillIsOfferedToSellAndWaitingForPayment
        | BillServiceError::BillIsRequestedToPay
        | BillServiceError::BillIsInRecourseAndWaitingForPayment
        | BillServiceError::BillRequestToAcceptDidNotExpireAndWasNotRejected
        | BillServiceError::BillRequestToPayDidNotExpireAndWasNotRejected
        | BillServiceError::BillIsNotRequestedToRecourseAndWaitingForPayment
        | BillServiceError::BillSellDataInvalid
        | BillServiceError::BillAlreadyPaid
        | BillServiceError::BillNotAccepted
        | BillServiceError::BillAlreadyRequestedToAccept
        | BillServiceError::BillIsRequestedToPayAndWaitingForPayment
        | BillServiceError::BillRecourseDataInvalid
        | BillServiceError::RecourseeNotPastHolder
        | BillServiceError::CallerIsNotDrawee
        | BillServiceError::CallerIsNotBuyer
        | BillServiceError::CallerIsNotRecoursee
        | BillServiceError::RequestAlreadyRejected
        | BillServiceError::CallerIsNotHolder
        | BillServiceError::NoFileForFileUploadId
        | BillServiceError::InvalidOperation
        | BillServiceError::InvalidBillType => JsErrorData {
            error: "bad_request",
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::Validation(msg) => JsErrorData {
            error: "validation",
            message: msg,
            code: 400,
        },
        BillServiceError::NotFound => JsErrorData {
            error: "not_found",
            message: e.to_string(),
            code: 404,
        },
        BillServiceError::Io(e) => JsErrorData {
            error: "io",
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Persistence(e) => JsErrorData {
            error: "persistence",
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::ExternalApi(e) => JsErrorData {
            error: "external_api",
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Blockchain(e) => JsErrorData {
            error: "blockchain",
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Cryptography(e) => JsErrorData {
            error: "crypto",
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Notification(e) => JsErrorData {
            error: "notification",
            message: e.to_string(),
            code: 500,
        },
    }
}
