use super::Result;
use super::middleware::IdentityCheck;
use crate::data::BitcreditEbillQuote;
use crate::service_context::ServiceContext;
use bcr_ebill_api::service::Error;
use log::info;
use rocket::serde::json::Json;
use rocket::{State, get, put};

#[get("/return/<id>")]
pub async fn return_quote(
    _identity: IdentityCheck,
    _state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    info!("return quote called with {id} - not implemented");
    Err(Error::PreconditionFailed.into())
}

#[put("/accept/<id>")]
pub async fn accept_quote(
    _identity: IdentityCheck,
    _state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    info!("accept quote called with {id} - not implemented");
    Err(Error::PreconditionFailed.into())
}
