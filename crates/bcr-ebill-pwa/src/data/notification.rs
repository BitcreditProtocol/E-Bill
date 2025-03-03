use bcr_ebill_api::data::notification::{Notification, NotificationType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use super::IntoWeb;

#[derive(Tsify, Debug, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct NotificationWeb {
    pub id: String,
    pub node_id: Option<String>,
    pub notification_type: NotificationTypeWeb,
    pub reference_id: Option<String>,
    pub description: String,
    pub datetime: String,
    pub active: bool,
    #[tsify(type = "any | undefined")]
    pub payload: Option<Value>,
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

#[derive(Tsify, Debug, Copy, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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
