use super::data::{
    BalanceResponse, CurrenciesResponse, CurrencyResponse, GeneralSearchFilterPayload,
    GeneralSearchResponse, OverviewBalanceResponse, OverviewResponse,
};
use crate::{
    constants::VALID_CURRENCIES,
    service::{Error, Result, ServiceContext},
    CONFIG,
};
use bill::get_current_identity_node_id;
use rocket::{fs::NamedFile, get, post, serde::json::Json, Shutdown, State};
use std::path::Path;

pub mod bill;
pub mod company;
pub mod contacts;
pub mod identity;
pub mod middleware;
pub mod notifications;
pub mod quotes;

#[get("/<_..>", rank = 10)]
pub async fn serve_frontend() -> Option<NamedFile> {
    NamedFile::open(Path::new(&CONFIG.frontend_serve_folder).join("index.html"))
        .await
        .ok()
}

#[get("/")]
pub async fn exit(shutdown: Shutdown, state: &State<ServiceContext>) {
    log::info!("Exit called - shutting down...");
    shutdown.notify();
    state.shutdown();
}

#[get("/")]
pub async fn currencies(_state: &State<ServiceContext>) -> Result<Json<CurrenciesResponse>> {
    Ok(Json(CurrenciesResponse {
        currencies: VALID_CURRENCIES
            .iter()
            .map(|vc| CurrencyResponse {
                code: vc.to_string(),
            })
            .collect(),
    }))
}

#[get("/?<currency>")]
pub async fn overview(
    currency: &str,
    state: &State<ServiceContext>,
) -> Result<Json<OverviewResponse>> {
    if !VALID_CURRENCIES.contains(&currency) {
        return Err(Error::Validation(format!(
            "Currency with code '{}' not found",
            currency
        )));
    }
    let result = state
        .bill_service
        .get_bill_balances(currency, &get_current_identity_node_id(state).await)
        .await?;

    Ok(Json(OverviewResponse {
        currency: currency.to_owned(),
        balances: OverviewBalanceResponse {
            payee: BalanceResponse {
                sum: result.payee.sum,
            },
            payer: BalanceResponse {
                sum: result.payer.sum,
            },
            contingent: BalanceResponse {
                sum: result.contingent.sum,
            },
        },
    }))
}

#[utoipa::path(
    tag = "General Search",
    path = "/search",
    description = "Search bills, contacts and companies",
    responses(
        (status = 200, description = "Search Result", body = GeneralSearchResponse)
    )
)]
#[post("/", format = "json", data = "<search_filter>")]
pub async fn search(
    state: &State<ServiceContext>,
    search_filter: Json<GeneralSearchFilterPayload>,
) -> Result<Json<GeneralSearchResponse>> {
    let result = state
        .search_service
        .search(
            &search_filter.filter.search_term,
            &search_filter.filter.currency,
            &search_filter.filter.item_types,
            &get_current_identity_node_id(state).await,
        )
        .await?;

    Ok(Json(result))
}
