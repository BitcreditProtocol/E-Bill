use super::super::data::{EditContactPayload, NewContactPayload};
use super::middleware::IdentityCheck;
use crate::service::contact_service::Contact;
use crate::service::{self, Result, ServiceContext};
use crate::util;
use crate::util::file::{detect_content_type_for_bytes, UploadFileHandler};
use crate::web::data::{ContactsResponse, SuccessResponse, UploadFileForm, UploadFilesResponse};
use rocket::form::Form;
use rocket::http::ContentType;
use rocket::serde::json::Json;
use rocket::{delete, get, post, put, State};

#[get("/file/<id>/<file_name>")]
pub async fn get_file(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
    file_name: &str,
) -> Result<(ContentType, Vec<u8>)> {
    state.contact_service.get_contact(id).await?; // check if contact exists

    let private_key = state
        .identity_service
        .get_full_identity()
        .await?
        .key_pair
        .get_private_key_string();

    let file_bytes = state
        .contact_service
        .open_and_decrypt_file(id, file_name, &private_key)
        .await
        .map_err(|_| service::Error::NotFound)?;

    let content_type = match detect_content_type_for_bytes(&file_bytes) {
        None => None,
        Some(t) => ContentType::parse_flexible(&t),
    }
    .ok_or(service::Error::Validation(String::from(
        "Content Type of the requested file could not be determined",
    )))?;

    Ok((content_type, file_bytes))
}

#[post("/upload_file", data = "<file_upload_form>")]
pub async fn upload_file(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    file_upload_form: Form<UploadFileForm<'_>>,
) -> Result<Json<UploadFilesResponse>> {
    let file = &file_upload_form.file;
    let upload_file_handler: &dyn UploadFileHandler = file as &dyn UploadFileHandler;

    state
        .file_upload_service
        .validate_attached_file(upload_file_handler)
        .await?;

    let file_upload_response = state
        .file_upload_service
        .upload_files(vec![upload_file_handler])
        .await?;

    Ok(Json(file_upload_response))
}

#[get("/list")]
pub async fn return_contacts(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Json<ContactsResponse<Contact>>> {
    let contacts: Vec<Contact> = state.contact_service.get_contacts().await?;
    Ok(Json(ContactsResponse { contacts }))
}

#[get("/detail/<node_id>")]
pub async fn return_contact(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    node_id: &str,
) -> Result<Json<Contact>> {
    let contact: Contact = state.contact_service.get_contact(node_id).await?;
    Ok(Json(contact))
}

#[delete("/remove/<node_id>")]
pub async fn remove_contact(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    node_id: &str,
) -> Result<Json<SuccessResponse>> {
    state.contact_service.delete(node_id).await?;
    Ok(Json(SuccessResponse::new()))
}

#[post("/create", format = "json", data = "<new_contact_payload>")]
pub async fn new_contact(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    new_contact_payload: Json<NewContactPayload>,
) -> Result<Json<Contact>> {
    let payload = new_contact_payload.0;

    util::file::validate_file_upload_id(&payload.avatar_file_upload_id)?;
    util::file::validate_file_upload_id(&payload.proof_document_file_upload_id)?;

    let contact = state
        .contact_service
        .add_contact(
            &payload.node_id,
            payload.t,
            payload.name,
            payload.email,
            payload.postal_address,
            payload.date_of_birth_or_registration,
            payload.country_of_birth_or_registration,
            payload.city_of_birth_or_registration,
            payload.identification_number,
            payload.avatar_file_upload_id,
            payload.proof_document_file_upload_id,
        )
        .await?;
    Ok(Json(contact))
}

#[put("/edit", format = "json", data = "<edit_contact_payload>")]
pub async fn edit_contact(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    edit_contact_payload: Json<EditContactPayload>,
) -> Result<Json<SuccessResponse>> {
    let payload = edit_contact_payload.0;
    state
        .contact_service
        .update_contact(
            &payload.node_id,
            payload.name,
            payload.email,
            payload.postal_address,
            payload.avatar_file_upload_id,
        )
        .await?;
    Ok(Json(SuccessResponse::new()))
}
