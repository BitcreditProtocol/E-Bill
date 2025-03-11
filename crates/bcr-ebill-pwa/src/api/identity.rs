use crate::{
    CONTEXT, Result,
    context::get_ctx,
    data::{
        BinaryFileResponse, FromWeb, IntoWeb, UploadFile,
        identity::{
            ChangeIdentityPayload, IdentityWeb, NewIdentityPayload, SeedPhrase, SwitchIdentity,
        },
    },
};
use bcr_ebill_api::{
    data::{OptionalPostalAddress, identity::IdentityType},
    external,
    service::Error,
    util::{
        self,
        date::{format_date_string, now},
        file::{UploadFileHandler, detect_content_type_for_bytes},
    },
};
use wasm_bindgen::prelude::*;

/// A structure describing the currently selected identity between the personal and multiple
/// possible company identities
#[derive(Clone, Debug)]
pub struct SwitchIdentityState {
    pub personal: String,
    pub company: Option<String>,
}

#[wasm_bindgen]
pub struct Identity;

#[wasm_bindgen]
impl Identity {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Identity
    }

    #[wasm_bindgen(unchecked_return_type = "BinaryFileResponse")]
    pub async fn file(&self, file_name: &str) -> Result<JsValue> {
        let identity = get_ctx().identity_service.get_full_identity().await?;
        let private_key = identity.key_pair.get_private_key_string();
        let id = identity.identity.node_id;

        let file_bytes = get_ctx()
            .identity_service
            .open_and_decrypt_file(&id, file_name, &private_key)
            .await
            .map_err(|_| Error::NotFound)?;

        let content_type = detect_content_type_for_bytes(&file_bytes).ok_or(Error::Validation(
            String::from("Content Type of the requested file could not be determined"),
        ))?;

        let res = serde_wasm_bindgen::to_value(&BinaryFileResponse {
            data: file_bytes,
            name: file_name.to_owned(),
            content_type,
        })?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "UploadFilesResponse")]
    pub async fn upload(
        &self,
        #[wasm_bindgen(unchecked_param_type = "UploadFile")] payload: JsValue,
    ) -> Result<JsValue> {
        let upload_file: UploadFile = serde_wasm_bindgen::from_value(payload)?;
        let upload_file_handler: &dyn UploadFileHandler = &upload_file as &dyn UploadFileHandler;

        get_ctx()
            .file_upload_service
            .validate_attached_file(upload_file_handler)
            .await?;

        let file_upload_response = get_ctx()
            .file_upload_service
            .upload_files(vec![upload_file_handler])
            .await?;

        let res = serde_wasm_bindgen::to_value(&file_upload_response.into_web())?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn detail(&self) -> Result<JsValue> {
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
    pub async fn create(
        &self,
        #[wasm_bindgen(unchecked_param_type = "NewIdentityPayload")] payload: JsValue,
    ) -> Result<()> {
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
    pub async fn change(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ChangeIdentityPayload")] payload: JsValue,
    ) -> Result<()> {
        let identity_payload: ChangeIdentityPayload = serde_wasm_bindgen::from_value(payload)?;

        util::file::validate_file_upload_id(&identity_payload.profile_picture_file_upload_id)?;
        util::file::validate_file_upload_id(&identity_payload.identity_document_file_upload_id)?;

        if identity_payload.name.is_none()
            && identity_payload.email.is_none()
            && identity_payload.postal_address.is_none()
            && identity_payload.date_of_birth.is_none()
            && identity_payload.country_of_birth.is_none()
            && identity_payload.city_of_birth.is_none()
            && identity_payload.identification_number.is_none()
            && identity_payload.profile_picture_file_upload_id.is_none()
            && identity_payload.identity_document_file_upload_id.is_none()
        {
            return Ok(());
        }
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        get_ctx()
            .identity_service
            .update_identity(
                identity_payload.name,
                identity_payload.email,
                OptionalPostalAddress::from_web(identity_payload.postal_address),
                identity_payload.date_of_birth,
                identity_payload.country_of_birth,
                identity_payload.city_of_birth,
                identity_payload.identification_number,
                identity_payload.profile_picture_file_upload_id,
                identity_payload.identity_document_file_upload_id,
                timestamp,
            )
            .await?;
        Ok(())
    }

    #[wasm_bindgen(unchecked_return_type = "SwitchIdentity")]
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
    pub async fn switch(
        &self,
        #[wasm_bindgen(unchecked_param_type = "SwitchIdentity")] switch_identity_payload: JsValue,
    ) -> Result<()> {
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

    #[wasm_bindgen(unchecked_return_type = "SeedPhrase")]
    pub async fn seed_backup(&self) -> Result<JsValue> {
        let seed_phrase = get_ctx().identity_service.get_seedphrase().await?;
        let res = serde_wasm_bindgen::to_value(&SeedPhrase { seed_phrase })?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn seed_recover(
        &self,
        #[wasm_bindgen(unchecked_param_type = "SeedPhrase")] payload: JsValue,
    ) -> Result<()> {
        let seed_phrase_payload: SeedPhrase = serde_wasm_bindgen::from_value(payload)?;
        get_ctx()
            .identity_service
            .recover_from_seedphrase(&seed_phrase_payload.seed_phrase)
            .await?;
        Ok(())
    }

    #[wasm_bindgen(unchecked_return_type = "BinaryFileResponse")]
    pub async fn backup(&self) -> Result<JsValue> {
        // TODO: db export not supported on WASM by surrealdb
        let file_name = format!("bitcredit_backup_{}.ecies", format_date_string(now()));
        let res = serde_wasm_bindgen::to_value(&BinaryFileResponse {
            data: vec![],
            name: file_name.to_string(),
            content_type: "application/pdf".to_string(),
        })?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn restore(&self, _payload: Vec<u8>) -> Result<()> {
        // TODO: db import not supported on WASM by surrealdb
        Ok(())
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

pub fn get_current_identity_node_id() -> String {
    let current_identity = get_current_identity();
    match current_identity.company {
        None => current_identity.personal,
        Some(company_node_id) => company_node_id,
    }
}
