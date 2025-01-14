use crate::external;
use crate::service::Result;
use crate::web::data::{ChangeIdentityPayload, IdentityPayload};
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
