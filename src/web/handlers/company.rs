use super::middleware::IdentityCheck;
use crate::data::{OptionalPostalAddress, PostalAddress};
use crate::util;
use crate::web::data::{
    AddSignatoryPayload, CompanyWeb, CreateCompanyPayload, EditCompanyPayload, FromWeb, IntoWeb,
    ListSignatoriesResponse, RemoveSignatoryPayload,
};
use crate::{
    dht::{GossipsubEvent, GossipsubEventId},
    external,
    service::{self, Result, ServiceContext},
    util::file::{detect_content_type_for_bytes, UploadFileHandler},
    web::data::{CompaniesResponse, SuccessResponse, UploadFileForm, UploadFilesResponse},
};
use log::error;
use rocket::{form::Form, get, http::ContentType, post, put, serde::json::Json, State};

#[get("/check_dht")]
pub async fn check_companies_in_dht(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Json<SuccessResponse>> {
    state.dht_client().check_companies().await?;
    Ok(Json(SuccessResponse::new()))
}

#[get("/list")]
pub async fn list(state: &State<ServiceContext>) -> Result<Json<CompaniesResponse<CompanyWeb>>> {
    let companies = state
        .company_service
        .get_list_of_companies()
        .await?
        .into_iter()
        .map(|c| c.into_web())
        .collect();
    Ok(Json(CompaniesResponse { companies }))
}

#[get("/signatories/<id>")]
pub async fn list_signatories(
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<ListSignatoriesResponse>> {
    let signatories = state.company_service.list_signatories(id).await?;
    Ok(Json(ListSignatoriesResponse {
        signatories: signatories.into_iter().map(|c| c.into()).collect(),
    }))
}

#[get("/file/<id>/<file_name>")]
pub async fn get_file(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
    file_name: &str,
) -> Result<(ContentType, Vec<u8>)> {
    state.company_service.get_company_by_id(id).await?; // check if company exists
    let private_key = state
        .identity_service
        .get_full_identity()
        .await?
        .key_pair
        .get_private_key_string();

    let file_bytes = state
        .company_service
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

    Ok(Json(file_upload_response.into_web()))
}

#[get("/<id>")]
pub async fn detail(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<CompanyWeb>> {
    let company = state.company_service.get_company_by_id(id).await?;
    Ok(Json(company.into_web()))
}

#[post("/create", format = "json", data = "<create_company_payload>")]
pub async fn create(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    create_company_payload: Json<CreateCompanyPayload>,
) -> Result<Json<CompanyWeb>> {
    let payload = create_company_payload.0;
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

    util::file::validate_file_upload_id(&payload.logo_file_upload_id)?;
    util::file::validate_file_upload_id(&payload.proof_of_registration_file_upload_id)?;

    let created_company = state
        .company_service
        .create_company(
            payload.name,
            payload.country_of_registration,
            payload.city_of_registration,
            PostalAddress::from_web(payload.postal_address),
            payload.email,
            payload.registration_number,
            payload.registration_date,
            payload.proof_of_registration_file_upload_id,
            payload.logo_file_upload_id,
            timestamp,
        )
        .await?;

    let id = &created_company.id;
    let node_id = state.identity_service.get_identity().await?.node_id;

    // asynchronously update the DHT
    let mut dht_client = state.dht_client();
    let id_clone = id.clone();
    tokio::spawn(async move {
        if let Err(e) = dht_client
            .add_company_to_dht_for_node(&id_clone, &node_id.to_string())
            .await
        {
            error!("Error while adding company {id_clone} to dht for node: {e}");
        }

        if let Err(e) = dht_client.subscribe_to_company_topic(&id_clone).await {
            error!("Error while subscribing to company topic {id_clone}: {e}");
        }

        if let Err(e) = dht_client.start_providing_company(&id_clone).await {
            error!("Error while starting to provide company {id_clone}: {e}");
        }
    });

    Ok(Json(created_company.into_web()))
}

#[put("/edit", format = "json", data = "<edit_company_payload>")]
pub async fn edit(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    edit_company_payload: Json<EditCompanyPayload>,
) -> Result<Json<SuccessResponse>> {
    let payload = edit_company_payload.0;
    if payload.name.is_none()
        && payload.email.is_none()
        && payload.postal_address.is_none()
        && payload.logo_file_upload_id.is_none()
    {
        return Ok(Json(SuccessResponse::new()));
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    state
        .company_service
        .edit_company(
            &payload.id,
            payload.name,
            payload.email,
            OptionalPostalAddress::from_web(payload.postal_address),
            payload.country_of_registration,
            payload.city_of_registration,
            payload.registration_number,
            payload.registration_date,
            payload.logo_file_upload_id,
            payload.proof_of_registration_file_upload_id,
            timestamp,
        )
        .await?;

    Ok(Json(SuccessResponse::new()))
}

#[put("/add_signatory", format = "json", data = "<add_signatory_payload>")]
pub async fn add_signatory(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    add_signatory_payload: Json<AddSignatoryPayload>,
) -> Result<Json<SuccessResponse>> {
    let payload = add_signatory_payload.0;
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    state
        .company_service
        .add_signatory(&payload.id, payload.signatory_node_id.clone(), timestamp)
        .await?;

    // asynchronously update the DHT
    let mut dht_client = state.dht_client();
    tokio::spawn(async move {
        if let Err(e) = dht_client
            .add_company_to_dht_for_node(&payload.id, &payload.signatory_node_id)
            .await
        {
            error!(
                "Error while adding company {} to dht for node: {e}",
                &payload.id
            );
        }

        match GossipsubEvent::new(
            GossipsubEventId::AddSignatoryFromCompany,
            payload.signatory_node_id.into_bytes(),
        )
        .to_byte_array()
        {
            Ok(event) => {
                if let Err(e) = dht_client
                    .add_message_to_company_topic(event, &payload.id)
                    .await
                {
                    error!("Error while adding signatory to company topic: {e}");
                }
            }
            Err(e) => error!("Error while creating gossipsub event: {e}"),
        };
    });

    Ok(Json(SuccessResponse::new()))
}

#[put(
    "/remove_signatory",
    format = "json",
    data = "<remove_signatory_payload>"
)]
pub async fn remove_signatory(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    remove_signatory_payload: Json<RemoveSignatoryPayload>,
) -> Result<Json<SuccessResponse>> {
    let payload = remove_signatory_payload.0;
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    state
        .company_service
        .remove_signatory(&payload.id, payload.signatory_node_id.clone(), timestamp)
        .await?;

    // asynchronously update the DHT
    let mut dht_client = state.dht_client();
    let node_id = state.identity_service.get_identity().await?.node_id;
    tokio::spawn(async move {
        if let Err(e) = dht_client
            .remove_company_from_dht_for_node(&payload.id, &payload.signatory_node_id)
            .await
        {
            error!("Error while removing company from dht for node: {e}");
        }

        match GossipsubEvent::new(
            GossipsubEventId::RemoveSignatoryFromCompany,
            payload.signatory_node_id.clone().into_bytes(),
        )
        .to_byte_array()
        {
            Ok(event) => {
                if let Err(e) = dht_client
                    .add_message_to_company_topic(event, &payload.id)
                    .await
                {
                    error!("Error while removing signatory to company topic: {e}");
                }
            }
            Err(e) => error!("Error while creating gossipsub event: {e}"),
        };

        // if we're removing ourselves, we need to stop subscribing and stop providing
        if node_id.to_string().eq(&payload.signatory_node_id) {
            if let Err(e) = dht_client.stop_providing_company(&payload.id).await {
                error!("Error while stopping to provide company: {e}");
            }

            if let Err(e) = dht_client.unsubscribe_from_company_topic(&payload.id).await {
                error!("Error while unsubscribing from company topic: {e}");
            }
        }
    });

    Ok(Json(SuccessResponse::new()))
}
