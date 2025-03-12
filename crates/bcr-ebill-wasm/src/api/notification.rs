use super::Result;
use crate::{
    context::get_ctx,
    data::{IntoWeb, notification::NotificationWeb},
};
use bcr_ebill_api::{NotificationFilter, data::contact::IdentityPublicData};
use log::{error, info};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Notification;

#[wasm_bindgen]
impl Notification {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Notification
    }

    #[wasm_bindgen]
    pub async fn subscribe(&self, callback: js_sys::Function) {
        wasm_bindgen_futures::spawn_local(async move {
            info!("Subscribed to notifications");
            let mut receiver = get_ctx().push_service.subscribe().await;
            while let Ok(msg) = receiver.recv().await {
                match serde_wasm_bindgen::to_value(&msg) {
                    Ok(event) => {
                        if let Err(e) = callback.call1(&JsValue::NULL, &event) {
                            error!("Error while sending notification: {e:?}");
                        }
                    }
                    Err(e) => {
                        error!("Error while serializing notification: {e}");
                    }
                }
            }
        });
    }

    #[wasm_bindgen(unchecked_return_type = "NotificationWeb[]")]
    pub async fn list(
        &self,
        active: Option<bool>,
        reference_id: Option<String>,
        notification_type: Option<String>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<JsValue> {
        let notifications = get_ctx()
            .notification_service
            .get_client_notifications(NotificationFilter {
                active,
                reference_id,
                notification_type,
                limit,
                offset,
            })
            .await?;

        let web: Vec<NotificationWeb> = notifications.into_iter().map(|n| n.into_web()).collect();
        let res = serde_wasm_bindgen::to_value(&web)?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn mark_as_done(&self, notification_id: &str) -> Result<()> {
        get_ctx()
            .notification_service
            .mark_notification_as_done(notification_id)
            .await?;
        Ok(())
    }

    #[wasm_bindgen]
    pub async fn trigger_test_msg(&self, payload: JsValue) -> Result<()> {
        let msg: serde_json::Value = serde_wasm_bindgen::from_value(payload)?;
        get_ctx()
            .push_service
            .send(serde_json::to_value(msg).unwrap())
            .await;
        Ok(())
    }

    #[wasm_bindgen]
    pub async fn trigger_test_notification(&self, node_id: &str) -> Result<()> {
        get_ctx()
            .notification_service
            .send_offer_to_sell_event(
                "some_id",
                Some(10),
                &IdentityPublicData {
                    t: bcr_ebill_api::data::contact::ContactType::Person,
                    node_id: node_id.to_owned(),
                    name: "some name".to_string(),
                    postal_address: bcr_ebill_api::data::PostalAddress {
                        country: "AT".to_string(),
                        city: "AT".to_string(),
                        zip: Some("1020".to_string()),
                        address: "street".to_string(),
                    },
                    email: None,
                    nostr_relay: Some(get_ctx().cfg.nostr_relay),
                },
            )
            .await
            .unwrap();
        Ok(())
    }
}

impl Default for Notification {
    fn default() -> Self {
        Notification
    }
}
