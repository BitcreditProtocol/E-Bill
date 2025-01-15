use crate::external;
use crate::service::Result;
use crate::web::data::{ChangeIdentityPayload, IdentityPayload, SwitchIdentity};
use crate::{service::identity_service::IdentityToReturn, service::ServiceContext};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, post, put, State};

#[get("/return")]
pub async fn return_identity(state: &State<ServiceContext>) -> Result<Json<IdentityToReturn>> {
    let my_identity = if !state.identity_service.identity_exists().await {
        return Err(crate::service::Error::NotFound);
    } else {
        let full_identity = state.identity_service.get_full_identity().await?;
        IdentityToReturn::from(full_identity.identity, full_identity.key_pair)?
    };
    Ok(Json(my_identity))
}

#[post("/create", format = "json", data = "<identity_payload>")]
pub async fn create_identity(
    state: &State<ServiceContext>,
    identity_payload: Json<IdentityPayload>,
) -> Result<Status> {
    let identity = identity_payload.into_inner();
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;
    state
        .identity_service
        .create_identity(
            identity.name,
            identity.date_of_birth,
            identity.city_of_birth,
            identity.country_of_birth,
            identity.email,
            identity.postal_address,
            timestamp,
        )
        .await?;
    Ok(Status::Ok)
}

#[put("/change", format = "json", data = "<identity_payload>")]
pub async fn change_identity(
    state: &State<ServiceContext>,
    identity_payload: Json<ChangeIdentityPayload>,
) -> Result<Status> {
    let identity_payload = identity_payload.into_inner();
    if identity_payload.name.is_none()
        && identity_payload.email.is_none()
        && identity_payload.postal_address.is_none()
    {
        return Ok(Status::Ok);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;
    state
        .identity_service
        .update_identity(
            identity_payload.name,
            identity_payload.email,
            identity_payload.postal_address,
            timestamp,
        )
        .await?;
    Ok(Status::Ok)
}

#[get("/active")]
pub async fn active(state: &State<ServiceContext>) -> Result<Json<SwitchIdentity>> {
    let current_identity_state = state.get_current_identity().await;
    let node_id = match current_identity_state.company {
        None => current_identity_state.personal,
        Some(company_node_id) => company_node_id,
    };
    Ok(Json(SwitchIdentity { node_id }))
}

#[put("/switch", format = "json", data = "<switch_identity_payload>")]
pub async fn switch(
    state: &State<ServiceContext>,
    switch_identity_payload: Json<SwitchIdentity>,
) -> Result<Status> {
    let node_id = switch_identity_payload.0.node_id;
    let personal_node_id = state.identity_service.get_identity().await?.node_id;

    // if it's the personal node id, set it
    if node_id == personal_node_id {
        state.set_current_personal_identity(node_id).await;
        return Ok(Status::Ok);
    }

    // if it's one of our companies, set it
    if state
        .company_service
        .get_list_of_companies()
        .await?
        .iter()
        .any(|c| c.id == node_id)
    {
        state.set_current_company_identity(node_id).await;
        return Ok(Status::Ok);
    }

    // otherwise, return an error
    Err(crate::service::Error::Validation(format!(
        "The provided node_id: {node_id} is not a valid company id, or personal node_id"
    )))
}
