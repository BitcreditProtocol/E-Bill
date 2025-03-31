use bcr_ebill_api::service::Error as ServiceError;
use bcr_ebill_api::service::bill_service::Error as BillServiceError;
use bcr_ebill_api::util::{self, ValidationError};
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

    #[error("Validation error: {0}")]
    Validation(#[from] bcr_ebill_api::util::ValidationError),
}

#[derive(Tsify, Debug, Clone, Serialize)]
#[tsify(into_wasm_abi)]
enum JsErrorType {
    InvalidSum,
    InvalidCurrency,
    InvalidContentType,
    InvalidContactType,
    InvalidDate,
    InvalidFileUploadId,
    InvalidBillType,
    DraweeCantBePayee,
    DraweeNotInContacts,
    PayeeNotInContacts,
    MintNotInContacts,
    BuyerNotInContacts,
    EndorseeNotInContacts,
    RecourseeNotInContacts,
    NoFileForFileUploadId,
    NotFound,
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
    // general
    DrawerIsNotBillIssuer,
    SignatoryNotInContacts,
    SignatoryAlreadySignatory,
    CantRemoveLastSignatory,
    NotASignatory,
    InvalidSecp256k1Key,
    FileIsTooBig,
    InvalidFileName,
    UnknownNodeId,
    BackupNotSupported,
    CallerMustBeSignatory,
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
                ServiceError::Validation(e) => validation_error_data(e),
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
            WasmError::Validation(e) => validation_error_data(e),
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
        BillServiceError::DraweeNotInContacts => JsErrorData {
            error: JsErrorType::DraweeNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::PayeeNotInContacts => JsErrorData {
            error: JsErrorType::PayeeNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::BuyerNotInContacts => JsErrorData {
            error: JsErrorType::BuyerNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::EndorseeNotInContacts => JsErrorData {
            error: JsErrorType::EndorseeNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::MintNotInContacts => JsErrorData {
            error: JsErrorType::MintNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        BillServiceError::RecourseeNotInContacts => JsErrorData {
            error: JsErrorType::RecourseeNotInContacts,
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
        BillServiceError::Validation(e) => validation_error_data(e),
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

fn validation_error_data(e: ValidationError) -> JsErrorData {
    match e {
        ValidationError::InvalidSum => JsErrorData {
            error: JsErrorType::InvalidSum,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidCurrency => JsErrorData {
            error: JsErrorType::InvalidCurrency,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidContactType => JsErrorData {
            error: JsErrorType::InvalidContactType,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidContentType => JsErrorData {
            error: JsErrorType::InvalidContentType,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidDate => JsErrorData {
            error: JsErrorType::InvalidDate,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidFileUploadId => JsErrorData {
            error: JsErrorType::InvalidFileUploadId,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidBillType => JsErrorData {
            error: JsErrorType::InvalidBillType,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::DraweeCantBePayee => JsErrorData {
            error: JsErrorType::DraweeCantBePayee,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::RequestAlreadyExpired => JsErrorData {
            error: JsErrorType::RequestAlreadyExpired,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillAlreadyAccepted => JsErrorData {
            error: JsErrorType::BillAlreadyAccepted,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillWasNotOfferedToSell => JsErrorData {
            error: JsErrorType::BillWasNotOfferedToSell,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillWasNotRequestedToPay => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToPay,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillWasNotRequestedToAccept => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToAccept,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillWasNotRequestedToRecourse => JsErrorData {
            error: JsErrorType::BillWasNotRequestedToRecourse,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsNotOfferToSellWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsNotOfferToSellWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsOfferedToSellAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsOfferedToSellAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsRequestedToPay => JsErrorData {
            error: JsErrorType::BillIsRequestedToPay,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsInRecourseAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsInRecourseAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillRequestToAcceptDidNotExpireAndWasNotRejected => JsErrorData {
            error: JsErrorType::BillRequestToAcceptDidNotExpireAndWasNotRejected,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillRequestToPayDidNotExpireAndWasNotRejected => JsErrorData {
            error: JsErrorType::BillRequestToPayDidNotExpireAndWasNotRejected,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsNotRequestedToRecourseAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsNotRequestedToRecourseAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillSellDataInvalid => JsErrorData {
            error: JsErrorType::BillSellDataInvalid,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillAlreadyPaid => JsErrorData {
            error: JsErrorType::BillAlreadyPaid,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillNotAccepted => JsErrorData {
            error: JsErrorType::BillNotAccepted,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillAlreadyRequestedToAccept => JsErrorData {
            error: JsErrorType::BillAlreadyRequestedToAccept,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillIsRequestedToPayAndWaitingForPayment => JsErrorData {
            error: JsErrorType::BillIsRequestedToPayAndWaitingForPayment,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BillRecourseDataInvalid => JsErrorData {
            error: JsErrorType::BillRecourseDataInvalid,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::RecourseeNotPastHolder => JsErrorData {
            error: JsErrorType::RecourseeNotPastHolder,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CallerIsNotDrawee => JsErrorData {
            error: JsErrorType::CallerIsNotDrawee,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CallerIsNotBuyer => JsErrorData {
            error: JsErrorType::CallerIsNotBuyer,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CallerIsNotRecoursee => JsErrorData {
            error: JsErrorType::CallerIsNotRecoursee,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::RequestAlreadyRejected => JsErrorData {
            error: JsErrorType::RequestAlreadyRejected,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CallerIsNotHolder => JsErrorData {
            error: JsErrorType::CallerIsNotHolder,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::DrawerIsNotBillIssuer => JsErrorData {
            error: JsErrorType::DrawerIsNotBillIssuer,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CallerMustBeSignatory => JsErrorData {
            error: JsErrorType::CallerMustBeSignatory,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::SignatoryNotInContacts(_) => JsErrorData {
            error: JsErrorType::SignatoryNotInContacts,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::SignatoryAlreadySignatory(_) => JsErrorData {
            error: JsErrorType::SignatoryAlreadySignatory,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::CantRemoveLastSignatory => JsErrorData {
            error: JsErrorType::CantRemoveLastSignatory,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::NotASignatory(_) => JsErrorData {
            error: JsErrorType::NotASignatory,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidSecp256k1Key(_) => JsErrorData {
            error: JsErrorType::InvalidSecp256k1Key,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::FileIsTooBig(_) => JsErrorData {
            error: JsErrorType::FileIsTooBig,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::InvalidFileName(_) => JsErrorData {
            error: JsErrorType::InvalidFileName,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::UnknownNodeId(_) => JsErrorData {
            error: JsErrorType::UnknownNodeId,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::BackupNotSupported => JsErrorData {
            error: JsErrorType::BackupNotSupported,
            message: e.to_string(),
            code: 400,
        },
        ValidationError::Blockchain(e) => JsErrorData {
            error: JsErrorType::Blockchain,
            message: e.to_string(),
            code: 500,
        },
    }
}
