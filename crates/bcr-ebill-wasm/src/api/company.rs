use super::Result;
use bcr_ebill_api::{
    data::{OptionalPostalAddress, PostalAddress},
    external,
    service::Error,
    util::{
        ValidationError,
        file::{UploadFileHandler, detect_content_type_for_bytes},
        validate_file_upload_id,
    },
};
use wasm_bindgen::prelude::*;

use crate::{
    context::get_ctx,
    data::{
        BinaryFileResponse, FromWeb, IntoWeb, UploadFile,
        company::{
            AddSignatoryPayload, CompaniesResponse, CreateCompanyPayload, EditCompanyPayload,
            ListSignatoriesResponse, RemoveSignatoryPayload,
        },
    },
};

#[wasm_bindgen]
pub struct Company;

#[wasm_bindgen]
impl Company {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Company
    }

    #[wasm_bindgen(unchecked_return_type = "BinaryFileResponse")]
    pub async fn file(&self, id: &str, file_name: &str) -> Result<JsValue> {
        get_ctx().company_service.get_company_by_id(id).await?; // check if company exists
        let private_key = get_ctx()
            .identity_service
            .get_full_identity()
            .await?
            .key_pair
            .get_private_key_string();

        let file_bytes = get_ctx()
            .company_service
            .open_and_decrypt_file(id, file_name, &private_key)
            .await?;
        get_ctx().contact_service.get_contact(id).await?; // check if contact exists

        let content_type = detect_content_type_for_bytes(&file_bytes)
            .ok_or(Error::Validation(ValidationError::InvalidContentType))?;

        let res = serde_wasm_bindgen::to_value(&BinaryFileResponse {
            data: file_bytes,
            name: file_name.to_owned(),
            content_type,
        })?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "UploadFileResponse")]
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
            .upload_file(upload_file_handler)
            .await?;

        let res = serde_wasm_bindgen::to_value(&file_upload_response.into_web())?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "CompaniesResponse")]
    pub async fn list(&self) -> Result<JsValue> {
        let companies = get_ctx()
            .company_service
            .get_list_of_companies()
            .await?
            .into_iter()
            .map(|c| c.into_web())
            .collect();
        let res = serde_wasm_bindgen::to_value(&CompaniesResponse { companies })?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "ListSignatoriesResponse")]
    pub async fn list_signatories(&self, id: &str) -> Result<JsValue> {
        let signatories = get_ctx().company_service.list_signatories(id).await?;
        let res = serde_wasm_bindgen::to_value(&ListSignatoriesResponse {
            signatories: signatories.into_iter().map(|c| c.into()).collect(),
        })?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "CompanyWeb")]
    pub async fn detail(&self, id: &str) -> Result<JsValue> {
        let company = get_ctx().company_service.get_company_by_id(id).await?;
        let res = serde_wasm_bindgen::to_value(&company.into_web())?;
        Ok(res)
    }

    #[wasm_bindgen(unchecked_return_type = "CompanyWeb")]
    pub async fn create(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CreateCompanyPayload")] payload: JsValue,
    ) -> Result<JsValue> {
        let company_payload: CreateCompanyPayload = serde_wasm_bindgen::from_value(payload)?;
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

        validate_file_upload_id(company_payload.logo_file_upload_id.as_deref())?;
        validate_file_upload_id(
            company_payload
                .proof_of_registration_file_upload_id
                .as_deref(),
        )?;

        let created_company = get_ctx()
            .company_service
            .create_company(
                company_payload.name,
                company_payload.country_of_registration,
                company_payload.city_of_registration,
                PostalAddress::from_web(company_payload.postal_address),
                company_payload.email,
                company_payload.registration_number,
                company_payload.registration_date,
                company_payload.proof_of_registration_file_upload_id,
                company_payload.logo_file_upload_id,
                timestamp,
            )
            .await?;

        let res = serde_wasm_bindgen::to_value(&created_company.into_web())?;
        Ok(res)
    }

    #[wasm_bindgen]
    pub async fn edit(
        &self,
        #[wasm_bindgen(unchecked_param_type = "EditCompanyPayload")] payload: JsValue,
    ) -> Result<()> {
        let company_payload: EditCompanyPayload = serde_wasm_bindgen::from_value(payload)?;
        validate_file_upload_id(company_payload.logo_file_upload_id.as_deref())?;
        validate_file_upload_id(
            company_payload
                .proof_of_registration_file_upload_id
                .as_deref(),
        )?;

        if company_payload.name.is_none()
            && company_payload.email.is_none()
            && company_payload.postal_address.is_none()
            && company_payload.logo_file_upload_id.is_none()
        {
            return Ok(());
        }
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        get_ctx()
            .company_service
            .edit_company(
                &company_payload.id,
                company_payload.name,
                company_payload.email,
                OptionalPostalAddress::from_web(company_payload.postal_address),
                company_payload.country_of_registration,
                company_payload.city_of_registration,
                company_payload.registration_number,
                company_payload.registration_date,
                company_payload.logo_file_upload_id,
                company_payload.proof_of_registration_file_upload_id,
                timestamp,
            )
            .await?;
        Ok(())
    }

    #[wasm_bindgen]
    pub async fn add_signatory(
        &self,
        #[wasm_bindgen(unchecked_param_type = "AddSignatoryPayload")] payload: JsValue,
    ) -> Result<()> {
        let company_payload: AddSignatoryPayload = serde_wasm_bindgen::from_value(payload)?;
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        get_ctx()
            .company_service
            .add_signatory(
                &company_payload.id,
                company_payload.signatory_node_id.clone(),
                timestamp,
            )
            .await?;
        Ok(())
    }

    #[wasm_bindgen]
    pub async fn remove_signatory(
        &self,
        #[wasm_bindgen(unchecked_param_type = "RemoveSignatoryPayload")] payload: JsValue,
    ) -> Result<()> {
        let company_payload: RemoveSignatoryPayload = serde_wasm_bindgen::from_value(payload)?;
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        get_ctx()
            .company_service
            .remove_signatory(
                &company_payload.id,
                company_payload.signatory_node_id.clone(),
                timestamp,
            )
            .await?;
        Ok(())
    }
}

impl Default for Company {
    fn default() -> Self {
        Company
    }
}
