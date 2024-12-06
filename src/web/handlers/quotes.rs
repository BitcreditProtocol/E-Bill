use crate::external;
use crate::external::mint::{
    check_bitcredit_quote, client_accept_bitcredit_quote, get_quote_from_map,
};
use crate::service::{bill_service::BitcreditEbillQuote, Result, ServiceContext};
use rocket::serde::json::Json;
use rocket::{get, put, State};
use std::thread;

#[get("/return/<id>")]
pub async fn return_quote(
    state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    let mut quote = get_quote_from_map(&id);
    let copy_id = id.clone();
    let local_peer_id = state.identity_service.get_peer_id().await?.to_string();
    if !quote.bill_id.is_empty() && quote.quote_id.is_empty() {
        // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
        // thread, but this logic will be replaced soon
        thread::spawn(move || check_bitcredit_quote(&copy_id, &local_peer_id))
            .join()
            .expect("Thread panicked");
    }
    quote = get_quote_from_map(&id);
    Ok(Json(quote))
}

#[put("/accept/<id>")]
pub async fn accept_quote(
    state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditEbillQuote>> {
    let mut quote = get_quote_from_map(&id);

    let public_data_endorsee = state
        .contact_service
        .get_identity_by_name(&quote.mint_node_id)
        .await?;

    if !public_data_endorsee.name.is_empty() {
        let timestamp = external::time::TimeApi::get_atomic_time()
            .await
            .unwrap()
            .timestamp;
        state
            .bill_service
            .endorse_bitcredit_bill(&quote.bill_id, public_data_endorsee.clone(), timestamp)
            .await?;
    }

    let copy_id = id.clone();
    if !quote.bill_id.is_empty() && !quote.quote_id.is_empty() {
        // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
        // thread, but this logic will be replaced soon
        thread::spawn(move || client_accept_bitcredit_quote(&copy_id))
            .join()
            .expect("Thread panicked");
    }
    quote = get_quote_from_map(&id);
    Ok(Json(quote))
}
