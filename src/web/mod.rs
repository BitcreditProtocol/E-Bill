use crate::service::ServiceContext;
use api_docs::ApiDocs;
use log::info;
use rocket::http::Method;
use rocket::{catch, catchers, routes, Build, Config, Request, Rocket};
use rocket_cors::{AllowedHeaders, AllowedOrigins, CorsOptions};
use serde::Serialize;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod api_docs;
pub mod data;
mod handlers;

use crate::constants::MAX_FILE_SIZE_BYTES;
use crate::CONFIG;
use rocket::data::ByteUnit;
use rocket::figment::Figment;
use rocket::fs::FileServer;
use rocket::serde::json::Json;
use serde_json::json;

#[derive(Serialize, Debug, Clone)]
pub struct ErrorResponse {
    error: &'static str,
    message: String,
    code: u16,
}

impl ErrorResponse {
    pub fn new(error: &'static str, message: String, code: u16) -> Self {
        Self {
            error,
            message,
            code,
        }
    }

    pub fn to_json_string(&self) -> String {
        json!({ "error": self.error, "message": self.message }).to_string()
    }
}

pub fn rocket_main(context: ServiceContext) -> Rocket<Build> {
    let conf = context.config.clone();
    let config = Figment::from(Config::default())
        .merge(("limits.forms", ByteUnit::Byte(MAX_FILE_SIZE_BYTES as u64)))
        .merge(("limits.file", ByteUnit::Byte(MAX_FILE_SIZE_BYTES as u64)))
        .merge((
            "limits.data-form",
            ByteUnit::Byte(MAX_FILE_SIZE_BYTES as u64),
        ))
        .merge(("port", conf.http_port))
        .merge(("address", conf.http_address.to_owned()));

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_headers(AllowedHeaders::all())
        .allowed_methods(
            vec![
                Method::Get,
                Method::Post,
                Method::Patch,
                Method::Put,
                Method::Delete,
                Method::Options,
            ]
            .into_iter()
            .map(From::from)
            .collect(),
        )
        .allow_credentials(true)
        .to_cors()
        .expect("Cors setup failed");

    let rocket = rocket::custom(config)
        .attach(cors.clone())
        // catchers for CORS and API errors
        .mount("/api/", rocket_cors::catch_all_options_routes())
        .mount("/api/", routes![handlers::default_api_error_catcher])
        .register("/api/", catchers![not_found])
        .manage(context)
        .manage(cors)
        .mount("/api/exit", routes![handlers::exit])
        .mount("/api/currencies", routes![handlers::currencies])
        .mount("/api/overview", routes![handlers::overview])
        .mount("/api/search", routes![handlers::search])
        .mount("/api/temp_file", routes![handlers::get_temp_file])
        .mount(
            "/api/identity",
            routes![
                handlers::identity::create_identity,
                handlers::identity::change_identity,
                handlers::identity::return_identity,
                handlers::identity::active,
                handlers::identity::switch,
                handlers::identity::get_seed_phrase,
                handlers::identity::recover_from_seed_phrase,
                handlers::identity::get_file,
                handlers::identity::upload_file,
                handlers::identity::backup_identity,
                handlers::identity::restore_identity,
            ],
        )
        .mount(
            "/api/contacts",
            routes![
                handlers::contacts::new_contact,
                handlers::contacts::edit_contact,
                handlers::contacts::remove_contact,
                handlers::contacts::return_contacts,
                handlers::contacts::return_contact,
                handlers::contacts::get_file,
                handlers::contacts::upload_file,
            ],
        )
        .mount(
            "/api/company",
            routes![
                handlers::company::check_companies_in_dht,
                handlers::company::list,
                handlers::company::detail,
                handlers::company::get_file,
                handlers::company::upload_file,
                handlers::company::create,
                handlers::company::edit,
                handlers::company::add_signatory,
                handlers::company::remove_signatory,
                handlers::company::list_signatories,
            ],
        )
        .mount(
            "/api/bill",
            routes![
                handlers::bill::all_bills_from_all_identities,
                handlers::bill::issue_bill,
                handlers::bill::bill_detail,
                handlers::bill::list,
                handlers::bill::list_light,
                handlers::bill::attachment,
                handlers::bill::upload_files,
                handlers::bill::endorse_bill,
                handlers::bill::request_to_accept_bill,
                handlers::bill::accept_bill,
                handlers::bill::request_to_pay_bill,
                handlers::bill::offer_to_sell_bill,
                handlers::bill::mint_bill,
                handlers::bill::accept_mint_bill,
                handlers::bill::request_to_mint_bill,
                handlers::bill::check_payment,
                handlers::bill::bitcoin_key,
                handlers::bill::numbers_to_words_for_sum,
                handlers::bill::find_and_sync_with_bill_in_dht,
                handlers::bill::check_dht_for_bills,
                handlers::bill::holder,
                handlers::bill::search,
                handlers::bill::get_past_endorsees_for_bill,
                handlers::bill::get_endorsements_for_bill,
                handlers::bill::reject_to_accept_bill,
                handlers::bill::reject_to_pay_bill,
                handlers::bill::reject_to_buy_bill,
                handlers::bill::reject_to_pay_recourse_bill,
                handlers::bill::request_to_recourse_bill_payment,
                handlers::bill::request_to_recourse_bill_acceptance,
            ],
        )
        .mount(
            "/api/quote",
            routes![
                handlers::quotes::return_quote,
                handlers::quotes::accept_quote
            ],
        )
        .mount(
            "/api/",
            routes![
                handlers::notifications::list_notifications,
                handlers::notifications::mark_notification_done,
                handlers::notifications::websocket,
                handlers::notifications::sse,
                handlers::notifications::trigger_msg,
            ],
        )
        .mount(
            "/",
            SwaggerUi::new("/api/swagger-ui/<_..>")
                .url("/api/api-docs/openapi.json", ApiDocs::openapi()),
        )
        // Routes for the frontend - lower rank means higher prio
        .mount(
            &CONFIG.frontend_url_path,
            FileServer::from(&CONFIG.frontend_serve_folder).rank(5), // lower rank than index.html
                                                                     // fallback, means it's higher
                                                                     // prio
        )
        .mount(&CONFIG.frontend_url_path, routes![handlers::serve_frontend]);

    info!("HTTP Server Listening on {}", conf.http_listen_url());

    if CONFIG.launch_frontend_at_startup {
        match open::that(
            format!("{}{}", conf.http_listen_url(), &CONFIG.frontend_url_path).as_str(),
        ) {
            Ok(_) => {}
            Err(_) => {
                info!("Can't open browser.")
            }
        }
    }

    rocket
}

#[catch(404)]
fn not_found(req: &Request) -> Json<ErrorResponse> {
    Json(ErrorResponse::new(
        "not_found",
        format!("We couldn't find the requested path '{}'", req.uri()),
        404,
    ))
}
