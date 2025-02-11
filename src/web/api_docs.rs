use super::handlers;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "BitCredit E-Bills API",
        description = "Allows to execute operations and monitor state of a BitCredit E-Bill instance",
        version = "1.0.0",
    ),
    paths(
        handlers::notifications::list_notifications,
        handlers::notifications::mark_notification_done,
        handlers::notifications::websocket,
        handlers::notifications::sse,
        handlers::bill::list,
        handlers::bill::all_bills_from_all_identities,
        handlers::bill::list_light,
        handlers::bill::search,
        handlers::bill::bill_detail,
        handlers::bill::get_past_endorsees_for_bill,
        handlers::bill::get_endorsements_for_bill,
        handlers::identity::return_identity,
        handlers::identity::create_identity,
        handlers::identity::change_identity,
        handlers::identity::active,
        handlers::identity::switch,
        handlers::identity::get_seed_phrase,
        handlers::identity::recover_from_seed_phrase,
        handlers::identity::backup_identity,
        handlers::identity::restore_identity,
        handlers::search,
    )
)]
pub struct ApiDocs;
