use bcr_ebill_api::service::Error as ServiceError;
use bcr_ebill_api::service::bill_service::Error as BillServiceError;
use bcr_ebill_api::util;
use bcr_ebill_transport::Error as NotificationServiceError;
use log::error;
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
enum JsErrorType {
    NoFileForFileUploadId,
    NotFound,
    Validation,
    ExternalApi,
    Io,
    Crypto,
    Persistence,
    Blockchain,
    Serialization,
    Init,
    // notification
    NotificationNetwork,
    NotificationMessage,
    //bill
    InvalidBillType,
    InvalidOperation,
    BillAlreadyAccepted,
    BillAlreadyRequestedToAccept,
    BillNotAccepted,
    CallerIsNotDrawee,
    CallerIsNotHolder,
    CallerIsNotRecoursee,
    CallerIsNotBuyer,
    RequestAlreadyExpired,
    RequestAlreadyRejected,
    BillAlreadyPaid,
    BillWasNotRequestedToAccept,
    BillWasNotRequestedToPay,
    BillWasNotOfferedToSell,
    BillRequestToAcceptDidNotExpireAndWasNotRejected,
    BillRequestToPayDidNotExpireAndWasNotRejected,
    RecourseeNotPastHolder,
    BillWasNotRequestedToRecourse,
    BillIsNotRequestedToRecourseAndWaitingForPayment,
    BillIsNotOfferToSellWaitingForPayment,
    BillSellDataInvalid,
    BillRecourseDataInvalid,
    BillIsRequestedToPayAndWaitingForPayment,
    BillIsOfferedToSellAndWaitingForPayment,
    BillIsInRecourseAndWaitingForPayment,
    BillIsRequestedToPay,
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
struct JsErrorData {
    error: JsErrorType,
    message: String,
    code: u16,
}

impl From<WasmError> for JsValue {
    fn from(error: WasmError) -> JsValue {
        error!("{error}");
        let js_error_data = match error {
            WasmError::Service(e) => match e {
                ServiceError::NoFileForFileUploadId => JsErrorData {
                    error: JsErrorType::NoFileForFileUploadId,
                    message: e.to_string(),
                    code: 400,
                },
                ServiceError::NotFound => JsErrorData {
                    error: JsErrorType::NotFound,
                    message: e.to_string(),
                    code: 404,
                },
                ServiceError::NotificationService(e) => notification_service_error_data(e),
                ServiceError::BillService(e) => bill_service_error_data(e),
                ServiceError::Validation(msg) => JsErrorData {
                    error: JsErrorType::Validation,
                    message: msg,
                    code: 400,
                },
                ServiceError::ExternalApi(e) => JsErrorData {
                    error: JsErrorType::ExternalApi,
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Io(e) => JsErrorData {
                    error: JsErrorType::Io,
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::CryptoUtil(e) => JsErrorData {
                    error: JsErrorType::Crypto,
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Persistence(e) => JsErrorData {
                    error: JsErrorType::Persistence,
                    message: e.to_string(),
                    code: 500,
                },
                ServiceError::Blockchain(e) => JsErrorData {
                    error: JsErrorType::Blockchain,
                    message: e.to_string(),
                    code: 500,
                },
            },
            WasmError::BillService(e) => bill_service_error_data(e),
            WasmError::NotificationService(e) => notification_service_error_data(e),
            WasmError::WasmSerialization(e) => JsErrorData {
                error: JsErrorType::Serialization,
                message: e.to_string(),
                code: 500,
            },
            WasmError::Crypto(e) => JsErrorData {
                error: JsErrorType::Crypto,
                message: e.to_string(),
                code: 500,
            },
            WasmError::Persistence(e) => JsErrorData {
                error: JsErrorType::Persistence,
                message: e.to_string(),
                code: 500,
            },
            WasmError::Init(e) => JsErrorData {
                error: JsErrorType::Init,
                message: e.to_string(),
                code: 500,
            },
        };
        serde_wasm_bindgen::to_value(&js_error_data).expect("can serialize error")
    }
}

fn notification_service_error_data(e: NotificationServiceError) -> JsErrorData {
    match e {
        NotificationServiceError::Network(e) => JsErrorData {
            error: JsErrorType::NotificationNetwork,
            message: e.to_string(),
            code: 500,
        },
        NotificationServiceError::Message(e) => JsErrorData {
            error: JsErrorType::NotificationMessage,
            message: e.to_string(),
            code: 500,
        },
        NotificationServiceError::Persistence(e) => JsErrorData {
            error: JsErrorType::Persistence,
            message: e.to_string(),
            code: 500,
        },
        NotificationServiceError::Crypto(e) => JsErrorData {
            error: JsErrorType::Crypto,
            message: e.to_string(),
            code: 500,
        },
        NotificationServiceError::Blockchain(e) => JsErrorData {
            error: JsErrorType::Blockchain,
            message: e.to_string(),
            code: 500,
        },
    }
}

fn bill_service_error_data(e: BillServiceError) -> JsErrorData {
    match e {
        BillServiceError::RequestAlreadyExpired => JsErrorData {
            error: JsErrorType::RequestAlreadyExpired,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillAlreadyAccepted => JsErrorData {
            error: JsErrorType::BillAlreadyAccepted,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillWasNotOfferedToSell => JsErrorData {
            error: JsErrorType::BillWasNotOfferedToSell,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillWasNotRequestedToPay => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToPay,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillWasNotRequestedToAccept => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToAccept,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillWasNotRequestedToRecourse => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToRecourse,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsNotOfferToSellWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsNotOfferToSellWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsOfferedToSellAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsOfferedToSellAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsRequestedToPay => JsErrorData {
            error: JsErrorType::BillIsRequestedToPay,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsInRecourseAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsInRecourseAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillRequestToAcceptDidNotExpireAndWasNotRejected => JsErrorData {
            error: JsErrorType::BillRequestToAcceptDidNotExpireAndWasNotRejected,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillRequestToPayDidNotExpireAndWasNotRejected => JsErrorData {
            error: JsErrorType::BillRequestToPayDidNotExpireAndWasNotRejected,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsNotRequestedToRecourseAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsNotRequestedToRecourseAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillSellDataInvalid => JsErrorData {
            error: JsErrorType::BillSellDataInvalid,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillAlreadyPaid => JsErrorData {
            error: JsErrorType::BillAlreadyPaid,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillNotAccepted => JsErrorData {
            error: JsErrorType::BillNotAccepted,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillAlreadyRequestedToAccept => JsErrorData {
            error: JsErrorType::BillAlreadyRequestedToAccept,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillIsRequestedToPayAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsRequestedToPayAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BillRecourseDataInvalid => JsErrorData {
            error: JsErrorType::BillRecourseDataInvalid,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::RecourseeNotPastHolder => JsErrorData {
            error: JsErrorType::RecourseeNotPastHolder,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::CallerIsNotDrawee => JsErrorData {
            error: JsErrorType::CallerIsNotDrawee,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::CallerIsNotBuyer => JsErrorData {
            error: JsErrorType::CallerIsNotBuyer,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::CallerIsNotRecoursee => JsErrorData {
            error: JsErrorType::CallerIsNotRecoursee,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::RequestAlreadyRejected => JsErrorData {
            error: JsErrorType::RequestAlreadyRejected,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::CallerIsNotHolder => JsErrorData {
            error: JsErrorType::CallerIsNotHolder,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::NoFileForFileUploadId => JsErrorData {
            error: JsErrorType::NoFileForFileUploadId,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::InvalidOperation => JsErrorData {
            error: JsErrorType::InvalidOperation,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::InvalidBillType => JsErrorData {
            error: JsErrorType::InvalidBillType,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::Validation(msg) => JsErrorData {
            error: JsErrorType::Validation,
            message: msg,
            code: 400,
        },
        BillServiceError::NotFound => JsErrorData {
            error: JsErrorType::NotFound,
            message: e.to_string(),
            code: 404,
        },
        BillServiceError::Io(e) => JsErrorData {
            error: JsErrorType::Io,
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Persistence(e) => JsErrorData {
            error: JsErrorType::Persistence,
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::ExternalApi(e) => JsErrorData {
            error: JsErrorType::ExternalApi,
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Blockchain(e) => JsErrorData {
            error: JsErrorType::Blockchain,
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Cryptography(e) => JsErrorData {
            error: JsErrorType::Crypto,
            message: e.to_string(),
            code: 500,
        },
        BillServiceError::Notification(e) => notification_service_error_data(e),
    }
}
