use crate::{
    CONTEXT, Result,
    context::get_ctx,
    data::{
        FromWeb, IdentityWeb, IntoWeb, NewIdentityPayload, SwitchIdentity, SwitchIdentityState,
    },
};
use bcr_ebill_api::{
    data::{OptionalPostalAddress, identity::IdentityType},
    external,
    service::Error,
    util,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Identity;

#[wasm_bindgen]
impl Identity {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Identity
    }

    #[wasm_bindgen]
    pub async fn return_identity(&self) -> Result<JsValue> {
        let my_identity = if !get_ctx().identity_service.identity_exists().await {
            return Err(Error::NotFound.into());
        } else {
            let full_identity = get_ctx().identity_service.get_full_identity().await?;
            IdentityWeb::from(full_identity.identity, full_identity.key_pair)?
        };
        let res = serde_wasm_bindgen::to_value(&my_identity)?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn create_identity(&self, payload: JsValue) -> Result<()> {
        let identity: NewIdentityPayload = serde_wasm_bindgen::from_value(payload)?;

        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

        util::file::validate_file_upload_id(&identity.profile_picture_file_upload_id)?;
        util::file::validate_file_upload_id(&identity.identity_document_file_upload_id)?;

        get_ctx()
            .identity_service
            .create_identity(
                identity.name,
                identity.email,
                OptionalPostalAddress::from_web(identity.postal_address),
                identity.date_of_birth,
                identity.country_of_birth,
                identity.city_of_birth,
                identity.identification_number,
                identity.profile_picture_file_upload_id,
                identity.identity_document_file_upload_id,
                timestamp,
            )
            .await?;

        Ok(())
    }

    #[wasm_bindgen]
    pub async fn active(&self) -> Result<JsValue> {
        let current_identity = get_current_identity();
        let (node_id, t) = match current_identity.company {
            None => (current_identity.personal, IdentityType::Person),
            Some(company_node_id) => (company_node_id, IdentityType::Company),
        };
        let switch_identity = SwitchIdentity {
            t: Some(t.into_web()),
            node_id,
        };
        let res = serde_wasm_bindgen::to_value(&switch_identity)?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn switch(&self, switch_identity_payload: JsValue) -> Result<()> {
        let payload: SwitchIdentity = serde_wasm_bindgen::from_value(switch_identity_payload)?;
        let node_id = payload.node_id;
        let personal_node_id = get_ctx().identity_service.get_identity().await?.node_id;

        // if it's the personal node id, set it
        if node_id == personal_node_id {
            set_current_personal_identity(node_id);
            return Ok(());
        }

        // if it's one of our companies, set it
        if get_ctx()
            .company_service
            .get_list_of_companies()
            .await?
            .iter()
            .any(|c| c.id == node_id)
        {
            set_current_company_identity(node_id);
            return Ok(());
        }

        // otherwise, return an error
        Err(Error::Validation(format!(
            "The provided node_id: {node_id} is not a valid company id, or personal node_id"
        ))
        .into())
    }
}

impl Default for Identity {
    fn default() -> Self {
        Identity
    }
}

pub fn get_current_identity() -> SwitchIdentityState {
    get_ctx().current_identity.clone()
}

pub fn set_current_personal_identity(node_id: String) {
    CONTEXT.with(|ctx| {
        if let Some(ref mut ctx) = *ctx.borrow_mut() {
            ctx.current_identity.personal = node_id;
            ctx.current_identity.company = None;
        }
    });
}

pub fn set_current_company_identity(node_id: String) {
    CONTEXT.with(|ctx| {
        if let Some(ref mut ctx) = *ctx.borrow_mut() {
            ctx.current_identity.company = Some(node_id);
        }
    });
}
