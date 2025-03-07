use bcr_ebill_api::data::notification::{Notification, NotificationType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use super::IntoWeb;

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: Option<String>,
    pub notification_type: NotificationTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub reference_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub description: String,
    #[wasm_bindgen(getter_with_clone)]
    pub datetime: String,
    pub active: bool,
    #[serde(skip_serializing)]
    payload: Option<Value>,
}

#[wasm_bindgen]
impl NotificationWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: String,
        node_id: Option<String>,
        notification_type: NotificationTypeWeb,
        reference_id: Option<String>,
        description: String,
        datetime: String,
        active: bool,
        payload: JsValue,
    ) -> NotificationWeb {
        let payload: Option<Value> = serde_wasm_bindgen::from_value(payload).ok();
        NotificationWeb {
            id,
            node_id,
            notification_type,
            reference_id,
            description,
            datetime,
            active,
            payload,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn payload(&self) -> JsValue {
        match &self.payload {
            Some(value) => serde_wasm_bindgen::to_value(value).unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        }
    }

    #[wasm_bindgen(setter)]
    pub fn set_payload(&mut self, payload: JsValue) {
        self.payload = serde_wasm_bindgen::from_value(payload).ok();
    }
}

impl IntoWeb<NotificationWeb> for Notification {
    fn into_web(self) -> NotificationWeb {
        NotificationWeb {
            id: self.id,
            node_id: self.node_id,
            notification_type: self.notification_type.into_web(),
            reference_id: self.reference_id,
            description: self.description,
            datetime: self
                .datetime
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            active: self.active,
            payload: self.payload,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum NotificationTypeWeb {
    General,
    Bill,
}

impl IntoWeb<NotificationTypeWeb> for NotificationType {
    fn into_web(self) -> NotificationTypeWeb {
        match self {
            NotificationType::Bill => NotificationTypeWeb::Bill,
            NotificationType::General => NotificationTypeWeb::General,
        }
    }
}
