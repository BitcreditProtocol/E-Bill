use super::{
    data::{
        BalanceResponse, CurrenciesResponse, CurrencyResponse, GeneralSearchFilterPayload,
        GeneralSearchResponse, OverviewBalanceResponse, OverviewResponse, SuccessResponse,
    },
    ErrorResponse,
};
use crate::{
    constants::VALID_CURRENCIES,
    service::{Error, Result, ServiceContext},
    util::file::detect_content_type_for_bytes,
    CONFIG,
};
use bill::get_current_identity_node_id;
use rocket::{fs::NamedFile, get, http::ContentType, post, serde::json::Json, Shutdown, State};
use std::path::{Path, PathBuf};

pub mod bill;
pub mod company;
pub mod contacts;
pub mod identity;
pub mod middleware;
pub mod notifications;
pub mod quotes;

// Lowest prio, fall back to index.html if nothing matches
#[get("/<_..>", rank = 10)]
pub async fn serve_frontend() -> Option<NamedFile> {
    NamedFile::open(Path::new(&CONFIG.frontend_serve_folder).join("index.html"))
        .await
        .ok()
}

// Higher prio than file server and index.html fallback
#[get("/<path..>", rank = 3)]
pub async fn default_api_error_catcher(path: PathBuf) -> Json<ErrorResponse> {
    Json(ErrorResponse::new(
        "not_found",
        format!("We couldn't find the requested path '{}'", path.display()),
        404,
    ))
}

#[get("/")]
pub async fn exit(
    shutdown: Shutdown,
    state: &State<ServiceContext>,
) -> Result<Json<SuccessResponse>> {
    log::info!("Exit called - shutting down...");
    shutdown.notify();
    state.shutdown();
    Ok(Json(SuccessResponse::new()))
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

#[get("/<file_upload_id>")]
pub async fn get_temp_file(
    state: &State<ServiceContext>,
    file_upload_id: &str,
) -> Result<(ContentType, Vec<u8>)> {
    if file_upload_id.is_empty() {
        return Err(Error::Validation(format!(
            "Invalid file upload id: {}",
            file_upload_id
        )));
    }
    match state
        .file_upload_service
        .get_temp_file(file_upload_id)
        .await
    {
        Ok(Some((_file_name, file_bytes))) => {
            let content_type = match detect_content_type_for_bytes(&file_bytes) {
                None => None,
                Some(t) => ContentType::parse_flexible(&t),
            }
            .ok_or(Error::Validation(String::from(
                "Content Type of the requested file could not be determined",
            )))?;
            Ok((content_type, file_bytes))
        }
        _ => Err(Error::NotFound),
    }
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
