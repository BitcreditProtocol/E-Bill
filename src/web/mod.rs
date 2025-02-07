use crate::service::ServiceContext;
use api_docs::ApiDocs;
use log::info;
use rocket::fs::FileServer;
use rocket::http::{Method, Status};
use rocket::{catch, catchers, routes, Build, Config, Request, Rocket};
use rocket_cors::{AllowedOrigins, CorsOptions};
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
        .allow_credentials(true);

    let rocket = rocket::custom(config)
        .register("/", catchers![default_catcher, not_found])
        .manage(context)
        .mount("/exit", routes![handlers::exit])
        .mount("/currencies", routes![handlers::currencies])
        .mount("/overview", routes![handlers::overview])
        .mount("/search", routes![handlers::search])
        .mount(
            "/identity",
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
            &CONFIG.frontend_url_path,
            FileServer::from(&CONFIG.frontend_serve_folder),
        )
        .mount(
            "/contacts",
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
            "/company",
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
            "/bill",
            routes![
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
                handlers::bill::find_bill_in_dht,
                handlers::bill::check_dht_for_bills,
                handlers::bill::holder,
                handlers::bill::search,
                handlers::bill::get_past_endorsees_for_bill,
                handlers::bill::reject_to_accept_bill,
                handlers::bill::reject_to_pay_bill,
                handlers::bill::reject_to_buy_bill,
                handlers::bill::reject_to_pay_recourse_bill,
                handlers::bill::request_to_recourse_bill_payment,
                handlers::bill::request_to_recourse_bill_acceptance,
            ],
        )
        .mount(
            "/quote",
            routes![
                handlers::quotes::return_quote,
                handlers::quotes::accept_quote
            ],
        )
        .mount(
            "/",
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
            SwaggerUi::new("/swagger-ui/<_..>").url("/api-docs/openapi.json", ApiDocs::openapi()),
        )
        .attach(cors.to_cors().expect("Cors setup failed"));

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

#[catch(default)]
pub fn default_catcher(status: Status, _req: &Request) -> Json<ErrorResponse> {
    Json(ErrorResponse::new(
        "error",
        status.reason().unwrap_or("Unknown error").to_string(),
        status.code,
    ))
}

#[catch(404)]
pub fn not_found(req: &Request) -> Json<ErrorResponse> {
    Json(ErrorResponse::new(
        "not_found",
        format!("We couldn't find the requested path '{}'", req.uri()),
        404,
    ))
}
