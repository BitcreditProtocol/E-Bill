use super::{
    data::{
        BalanceResponse, CurrenciesResponse, CurrencyResponse, FromWeb, GeneralSearchFilterPayload,
        GeneralSearchResponse, IntoWeb, OverviewBalanceResponse, OverviewResponse, StatusResponse,
        SuccessResponse,
    },
    ErrorResponse,
};
use crate::{
    constants::VALID_CURRENCIES,
    data::GeneralSearchFilterItemType,
    service::{bill_service, Error, Result, ServiceContext},
    util::file::detect_content_type_for_bytes,
    CONFIG,
};
use bill::get_current_identity_node_id;
use log::error;
use rocket::Response;
use rocket::{fs::NamedFile, get, http::ContentType, post, serde::json::Json, Shutdown, State};
use rocket::{http::Status, response::Responder};
use std::io::Cursor;
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
pub async fn status() -> Result<Json<StatusResponse>> {
    Ok(Json(StatusResponse {
        bitcoin_network: CONFIG.bitcoin_network().to_string(),
        app_version: std::env::var("CARGO_PKG_VERSION").unwrap_or(String::from("unknown")),
    }))
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
    let filters: Vec<GeneralSearchFilterItemType> = search_filter
        .filter
        .clone()
        .item_types
        .into_iter()
        .map(GeneralSearchFilterItemType::from_web)
        .collect();
    let result = state
        .search_service
        .search(
            &search_filter.filter.search_term,
            &search_filter.filter.currency,
            &filters,
            &get_current_identity_node_id(state).await,
        )
        .await?;

    Ok(Json(result.into_web()))
}

/// Map from service errors directly to rocket status codes. This allows us to
/// write handlers that return `Result<T, service::Error>` and still return the correct
/// status code.
impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            Error::NoFileForFileUploadId => {
                let body =
                    ErrorResponse::new("bad_request", self.to_string(), 400).to_json_string();
                Response::build()
                    .status(Status::BadRequest)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            Error::PreconditionFailed => Status::NotAcceptable.respond_to(req),
            Error::NotFound => {
                let body =
                    ErrorResponse::new("not_found", "not found".to_string(), 404).to_json_string();
                Response::build()
                    .status(Status::NotFound)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            Error::NotificationService(_) => Status::InternalServerError.respond_to(req),
            Error::BillService(e) => e.respond_to(req),
            Error::Validation(msg) => build_validation_response(msg),
            // If an external API errors, we can only tell the caller that something went wrong on
            // our end
            Error::ExternalApi(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Io(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            // for now, DHT errors are InternalServerError
            Error::Dht(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::CryptoUtil(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            // for now handle all persistence errors as InternalServerError, there
            // will be cases where we want to handle them differently (eg. 409 Conflict)
            Error::Persistence(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Blockchain(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
        }
    }
}

impl<'r, 'o: 'r> Responder<'r, 'o> for bill_service::Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            bill_service::Error::RequestAlreadyExpired
            | bill_service::Error::BillAlreadyAccepted
            | bill_service::Error::BillWasNotOfferedToSell
            | bill_service::Error::BillWasNotRequestedToPay
            | bill_service::Error::BillWasNotRequestedToAccept
            | bill_service::Error::BillWasNotRequestedToRecourse
            | bill_service::Error::BillIsNotOfferToSellWaitingForPayment
            | bill_service::Error::BillIsOfferedToSellAndWaitingForPayment
            | bill_service::Error::BillIsInRecourseAndWaitingForPayment
            | bill_service::Error::BillRequestToAcceptDidNotExpireAndWasNotRejected
            | bill_service::Error::BillRequestToPayDidNotExpireAndWasNotRejected
            | bill_service::Error::BillIsNotRequestedToRecourseAndWaitingForPayment
            | bill_service::Error::BillSellDataInvalid
            | bill_service::Error::BillAlreadyPaid
            | bill_service::Error::BillRecourseDataInvalid
            | bill_service::Error::RecourseeNotPastHolder
            | bill_service::Error::CallerIsNotDrawee
            | bill_service::Error::CallerIsNotBuyer
            | bill_service::Error::CallerIsNotRecoursee
            | bill_service::Error::RequestAlreadyRejected
            | bill_service::Error::CallerIsNotHolder
            | bill_service::Error::NoFileForFileUploadId
            | bill_service::Error::InvalidOperation => {
                let body =
                    ErrorResponse::new("bad_request", self.to_string(), 400).to_json_string();
                Response::build()
                    .status(Status::BadRequest)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            bill_service::Error::NotFound => {
                let body =
                    ErrorResponse::new("not_found", "not found".to_string(), 404).to_json_string();
                Response::build()
                    .status(Status::NotFound)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            bill_service::Error::Io(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::Persistence(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::ExternalApi(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::Blockchain(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::Dht(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::Cryptography(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            bill_service::Error::Notification(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
        }
    }
}

fn build_validation_response<'o>(msg: String) -> rocket::response::Result<'o> {
    let err_resp = ErrorResponse::new("validation_error", msg, 400);
    let body = err_resp.to_json_string();
    Response::build()
        .status(Status::BadRequest)
        .header(ContentType::JSON)
        .sized_body(body.len(), Cursor::new(body))
        .ok()
}
