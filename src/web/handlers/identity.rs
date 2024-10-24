use super::super::data::IdentityForm;
use crate::bill::identity::{
    create_whole_identity, get_whole_identity, read_identity_from_file, read_peer_id_from_file,
    write_identity_to_file, Identity, IdentityWithAll, NodeId,
};
use crate::constants::IDENTITY_FILE_PATH;
use crate::dht::Client;
use libp2p::PeerId;
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, post, put, State};
use std::path::Path;

#[get("/return")]
pub async fn return_identity() -> Json<Identity> {
    let my_identity = if !Path::new(IDENTITY_FILE_PATH).exists() {
        Identity::new_empty()
    } else {
        let identity: IdentityWithAll = get_whole_identity();
        identity.identity
    };
    Json(my_identity)
}

#[get("/peer_id/return")]
pub async fn return_peer_id() -> Json<NodeId> {
    let peer_id: PeerId = read_peer_id_from_file();
    let node_id = NodeId::new(peer_id.to_string());
    Json(node_id)
}

#[post("/create", data = "<identity_form>")]
pub async fn create_identity(identity_form: Form<IdentityForm>, state: &State<Client>) -> Status {
    println!("Create identity");
    let identity: IdentityForm = identity_form.into_inner();
    create_whole_identity(
        identity.name,
        identity.company,
        identity.date_of_birth,
        identity.city_of_birth,
        identity.country_of_birth,
        identity.email,
        identity.postal_address,
    );

    let mut client = state.inner().clone();
    client.put_identity_public_data_in_dht().await;

    Status::Ok
}

#[put("/change", data = "<identity_form>")]
pub async fn change_identity(identity_form: Form<IdentityForm>, state: &State<Client>) -> Status {
    println!("Change identity");

    let identity_form: IdentityForm = identity_form.into_inner();
    let mut identity_changes: Identity = Identity::new_empty();
    identity_changes.name = identity_form.name.trim().to_string();
    identity_changes.company = identity_form.company.trim().to_string();
    identity_changes.email = identity_form.email.trim().to_string();
    identity_changes.postal_address = identity_form.postal_address.trim().to_string();

    let mut my_identity: Identity;
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        return Status::NotAcceptable;
    }
    my_identity = read_identity_from_file();

    if !my_identity.update_valid(&identity_changes) {
        return Status::NotAcceptable;
    }
    my_identity.update_from(&identity_changes);

    write_identity_to_file(&my_identity);
    let mut client = state.inner().clone();
    client.put_identity_public_data_in_dht().await;

    Status::Ok
}
