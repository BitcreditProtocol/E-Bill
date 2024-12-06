use crate::{
    dht::{GossipsubEvent, GossipsubEventId},
    service::{
        self,
        company_service::{CompanyPublicData, CompanyToReturn},
        Error, Result, ServiceContext,
    },
    util::file::{detect_content_type_for_bytes, UploadFileHandler},
    web::data::{UploadFileForm, UploadFilesResponse},
};
use data::{AddSignatoryPayload, CreateCompanyPayload, EditCompanyPayload, RemoveSignatoryPayload};
use rocket::{
    form::Form,
    get,
    http::{ContentType, Status},
    post, put,
    serde::json::Json,
    State,
};

pub mod data;

#[get("/check_dht")]
pub async fn check_companies_in_dht(state: &State<ServiceContext>) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    state.dht_client().check_companies().await?;
    Ok(Status::Ok)
}

#[get("/list")]
pub async fn list(state: &State<ServiceContext>) -> Result<Json<Vec<CompanyToReturn>>> {
    let companies = state.company_service.get_list_of_companies().await?;
    Ok(Json(companies))
}

#[get("/file/<id>/<file_name>")]
pub async fn get_file(
    state: &State<ServiceContext>,
    id: &str,
    file_name: &str,
) -> Result<(ContentType, Vec<u8>)> {
    let private_key = state.identity_service.get_identity().await?.private_key_pem;

    let file_bytes = state
        .company_service
        .open_and_decrypt_file(id, file_name, &private_key)
        .await?;

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
    state: &State<ServiceContext>,
    file_upload_form: Form<UploadFileForm<'_>>,
) -> Result<Json<UploadFilesResponse>> {
    if !state.identity_service.identity_exists().await {
        return Err(Error::PreconditionFailed);
    }

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

#[get("/<id>")]
pub async fn detail(state: &State<ServiceContext>, id: &str) -> Result<Json<CompanyToReturn>> {
    let company = state.company_service.get_company_by_id(id).await?;
    Ok(Json(company))
}

#[post("/create", format = "json", data = "<create_company_payload>")]
pub async fn create(
    state: &State<ServiceContext>,
    create_company_payload: Json<CreateCompanyPayload>,
) -> Result<Json<CompanyToReturn>> {
    let payload = create_company_payload.0;
    let created_company = state
        .company_service
        .create_company(
            payload.legal_name,
            payload.country_of_registration,
            payload.city_of_registration,
            payload.postal_address,
            payload.legal_email,
            payload.registration_number,
            payload.registration_date,
            payload.proof_of_registration_file_upload_id,
            payload.logo_file_upload_id,
        )
        .await?;

    let id = &created_company.id;
    let peer_id = state.identity_service.get_peer_id().await?;

    let mut dht_client = state.dht_client();
    dht_client
        .add_company_to_dht_for_node(id, &peer_id.to_string())
        .await?;
    dht_client.subscribe_to_company_topic(id).await?;
    dht_client.start_providing_company(id).await?;
    dht_client
        .put_company_public_data_in_dht(CompanyPublicData::from(created_company.clone()))
        .await?;

    Ok(Json(created_company))
}

#[put("/edit", format = "json", data = "<edit_company_payload>")]
pub async fn edit(
    state: &State<ServiceContext>,
    edit_company_payload: Json<EditCompanyPayload>,
) -> Result<()> {
    let payload = edit_company_payload.0;
    state
        .company_service
        .edit_company(
            &payload.id,
            payload.legal_name,
            payload.legal_email,
            payload.postal_address,
            payload.logo_file_upload_id,
        )
        .await?;

    let updated = state.company_service.get_company_by_id(&payload.id).await?;
    let mut dht_client = state.dht_client();
    dht_client
        .put_company_public_data_in_dht(CompanyPublicData::from(updated))
        .await?;

    Ok(())
}

#[put("/add_signatory", format = "json", data = "<add_signatory_payload>")]
pub async fn add_signatory(
    state: &State<ServiceContext>,
    add_signatory_payload: Json<AddSignatoryPayload>,
) -> Result<()> {
    let payload = add_signatory_payload.0;
    state
        .company_service
        .add_signatory(&payload.id, payload.signatory_node_id.clone())
        .await?;

    let mut dht_client = state.dht_client();
    dht_client
        .add_company_to_dht_for_node(&payload.id, &payload.signatory_node_id)
        .await?;

    dht_client
        .add_message_to_company_topic(
            GossipsubEvent::new(
                GossipsubEventId::AddSignatoryFromCompany,
                payload.signatory_node_id.into_bytes(),
            )
            .to_byte_array()?,
            &payload.id,
        )
        .await?;

    Ok(())
}

#[put(
    "/remove_signatory",
    format = "json",
    data = "<remove_signatory_payload>"
)]
pub async fn remove_signatory(
    state: &State<ServiceContext>,
    remove_signatory_payload: Json<RemoveSignatoryPayload>,
) -> Result<()> {
    let payload = remove_signatory_payload.0;
    state
        .company_service
        .remove_signatory(&payload.id, payload.signatory_node_id.clone())
        .await?;

    let mut dht_client = state.dht_client();
    dht_client
        .remove_company_from_dht_for_node(&payload.id, &payload.signatory_node_id)
        .await?;
    dht_client
        .add_message_to_company_topic(
            GossipsubEvent::new(
                GossipsubEventId::RemoveSignatoryFromCompany,
                payload.signatory_node_id.clone().into_bytes(),
            )
            .to_byte_array()?,
            &payload.id,
        )
        .await?;

    // if we're removing ourselves, we need to stop subscribing and stop providing
    let peer_id = state.identity_service.get_peer_id().await?;
    if peer_id.to_string().eq(&payload.signatory_node_id) {
        dht_client.stop_providing_company(&payload.id).await?;
        dht_client
            .unsubscribe_from_company_topic(&payload.id)
            .await?;
    }

    Ok(())
}
