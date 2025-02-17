use super::middleware::IdentityCheck;
use crate::service::{bill_service::BitcreditEbillQuote, Error, Result, ServiceContext};
use log::info;
use rocket::serde::json::Json;
use rocket::{get, put, State};

#[get("/return/<id>")]
pub async fn return_quote(
    _identity: IdentityCheck,
    _state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    info!("return quote called with {id} - not implemented");
    Err(Error::PreconditionFailed)
}

#[put("/accept/<id>")]
pub async fn accept_quote(
    _identity: IdentityCheck,
    _state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    info!("accept quote called with {id} - not implemented");
    Err(Error::PreconditionFailed)
}
