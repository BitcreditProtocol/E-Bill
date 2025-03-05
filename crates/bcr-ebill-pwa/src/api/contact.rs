use crate::{Result, context::get_ctx};
use log::info;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Contact;

#[wasm_bindgen]
impl Contact {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Contact
    }
    #[wasm_bindgen]
    pub async fn get_contact_for_node_id(&self) -> Result<()> {
        let contact = get_ctx()
            .contact_service
            .get_identity_by_node_id(
                "039180c169e5f6d7c579cf1cefa37bffd47a2b389c8125601f4068c87bea795943",
            )
            .await?;
        info!("{contact:?}");
        Ok(())
    }
}

impl Default for Contact {
    fn default() -> Self {
        Contact
    }
}
