use bcr_ebill_api::service::Error as ServiceError;
use bcr_ebill_api::service::bill_service::Error as BillServiceError;
use bcr_ebill_api::service::notification_service::Error as NotificationServiceError;
use bcr_ebill_api::util;
use serde::Serialize;
use thiserror::Error;
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

#[derive(Serialize)]
struct JsErrorData {
    error: &'static str,
    message: String,
    code: u16,
}

impl From<WasmError> for JsValue {
    fn from(error: WasmError) -> JsValue {
        let js_error_data = match error {
            WasmError::Service(e) => JsErrorData {
                error: "service_error",
                message: e.to_string(),
                code: 500,
            },
            WasmError::BillService(e) => JsErrorData {
                error: "bill_service_error",
                message: e.to_string(),
                code: 500,
            },
            WasmError::NotificationService(e) => JsErrorData {
                error: "notification_service_error",
                message: e.to_string(),
                code: 500,
            },
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
