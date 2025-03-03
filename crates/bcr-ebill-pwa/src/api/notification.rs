use crate::context::get_ctx;
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
}

impl Default for Notification {
    fn default() -> Self {
        Notification
    }
}
