use super::Result;
use crate::context::get_ctx;
use identity::get_current_identity;
use log::info;
use wasm_bindgen::prelude::*;

pub mod contact;
pub mod identity;
pub mod notification;

#[wasm_bindgen]
pub struct Api;

#[wasm_bindgen]
impl Api {
    #[wasm_bindgen]
    pub fn contact() -> contact::Contact {
        contact::Contact::new()
    }

    #[wasm_bindgen]
    pub fn identity() -> identity::Identity {
        identity::Identity::new()
    }

    #[wasm_bindgen]
    pub fn notification() -> notification::Notification {
        notification::Notification::new()
    }

    #[wasm_bindgen]
    pub async fn get_bills() -> Result<()> {
        let current_identity = get_current_identity();
        let bills = get_ctx()
            .bill_service
            .get_bills(&current_identity.personal)
            .await?;
        info!("{bills:?}");
        Ok(())
    }
}
