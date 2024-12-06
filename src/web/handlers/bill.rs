use crate::blockchain::Chain;
use crate::external::mint::{accept_mint_bitcredit, request_to_mint_bitcredit};
use crate::service::{contact_service::IdentityPublicData, Result};
use crate::util::file::{detect_content_type_for_bytes, UploadFileHandler};
use crate::web::data::{
    AcceptBitcreditBillPayload, AcceptMintBitcreditBillPayload, BitcreditBillPayload,
    EndorseBitcreditBillPayload, MintBitcreditBillPayload, RequestToAcceptBitcreditBillPayload,
    RequestToMintBitcreditBillPayload, RequestToPayBitcreditBillPayload, SellBitcreditBillPayload,
    UploadBillFilesForm, UploadFilesResponse,
};
use crate::{external, service};
use crate::{
    service::bill_service::{BitcreditBill, BitcreditBillToReturn},
    service::ServiceContext,
};
use rocket::form::Form;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use rocket::{get, post, put, State};
use std::thread;

#[get("/holder/<id>")]
pub async fn holder(state: &State<ServiceContext>, id: String) -> Result<Json<bool>> {
    let identity = state.identity_service.get_full_identity().await?;
    let bill = state.bill_service.get_bill(&id).await?;
    let am_i_holder = identity.peer_id.to_string().eq(&bill.payee.peer_id);
    Ok(Json(am_i_holder))
}

#[get("/attachment/<bill_name>/<file_name>")]
pub async fn attachment(
    state: &State<ServiceContext>,
    bill_name: &str,
    file_name: &str,
) -> Result<(ContentType, Vec<u8>)> {
    let keys = state.bill_service.get_bill_keys(bill_name).await?;
    let file_bytes = state
        .bill_service
        .open_and_decrypt_attached_file(bill_name, file_name, &keys.private_key_pem)
        .await?;

    let content_type = match detect_content_type_for_bytes(&file_bytes) {
        None => None,
        Some(t) => ContentType::parse_flexible(&t),
    }
    .ok_or(service::Error::Validation(String::from(
        "Content Type of the requested file could not be determined",
    )))?;

    Ok((content_type, file_bytes))
}

#[get("/return")]
pub async fn return_bills_list(
    state: &State<ServiceContext>,
) -> Result<Json<Vec<BitcreditBillToReturn>>> {
    let bills = state.bill_service.get_bills().await?;
    Ok(Json(bills))
}

#[get("/return/basic/<id>")]
pub async fn return_basic_bill(
    state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditBill>> {
    let bill = state.bill_service.get_bill(&id).await?;
    Ok(Json(bill))
}

#[get("/chain/return/<id>")]
pub async fn return_chain_of_blocks(
    state: &State<ServiceContext>,
    id: String,
) -> Result<Json<Chain>> {
    let chain = state.bill_service.get_blockchain_for_bill(&id).await?;
    Ok(Json(chain))
}

#[get("/find/<bill_id>")]
pub async fn find_bill_in_dht(state: &State<ServiceContext>, bill_id: String) -> Result<Status> {
    state.bill_service.find_bill_in_dht(&bill_id).await?;
    Ok(Status::Ok)
}

#[get("/return/<id>")]
pub async fn return_bill(
    state: &State<ServiceContext>,
    id: String,
) -> Result<Json<BitcreditBillToReturn>> {
    let current_timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;
    let full_bill = state
        .bill_service
        .get_full_bill(&id, current_timestamp)
        .await?;
    Ok(Json(full_bill))
}

#[get("/dht")]
pub async fn search_bill(state: &State<ServiceContext>) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let mut client = state.dht_client();
    let local_peer_id = state.identity_service.get_peer_id().await?;
    client.check_new_bills(local_peer_id.to_string()).await?;

    Ok(Status::Ok)
}

#[post("/upload_files", data = "<files_upload_form>")]
pub async fn upload_files(
    state: &State<ServiceContext>,
    files_upload_form: Form<UploadBillFilesForm<'_>>,
) -> Result<Json<UploadFilesResponse>> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }

    if files_upload_form.files.is_empty() {
        return Err(service::Error::PreconditionFailed);
    }

    let files = &files_upload_form.files;
    let upload_file_handlers: Vec<&dyn UploadFileHandler> = files
        .iter()
        .map(|file| file as &dyn UploadFileHandler)
        .collect();

    // Validate Files
    for file in &upload_file_handlers {
        state
            .file_upload_service
            .validate_attached_file(*file)
            .await?;
    }

    let file_upload_response = state
        .file_upload_service
        .upload_files(upload_file_handlers)
        .await?;

    Ok(Json(file_upload_response))
}

#[post("/issue", format = "json", data = "<bill_payload>")]
pub async fn issue_bill(
    state: &State<ServiceContext>,
    bill_payload: Json<BitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }

    let drawer = state.identity_service.get_full_identity().await?;

    let (public_data_drawee, public_data_payee) =
        match (bill_payload.drawer_is_payee, bill_payload.drawer_is_drawee) {
            // Drawer is drawee and payee
            (true, true) => {
                return Err(service::Error::Validation(String::from(
                    "Drawer can't be Drawee and Payee at the same time",
                )));
            }
            // Drawer is payee
            (true, false) => {
                let public_data_drawee = state
                    .contact_service
                    .get_identity_by_name(&bill_payload.drawee_name)
                    .await
                    .map_err(|_| {
                        service::Error::Validation(String::from("Can not get drawee identity."))
                    })?;

                let public_data_payee =
                    IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string());

                (public_data_drawee, public_data_payee)
            }
            // Drawer is drawee
            (false, true) => {
                let public_data_drawee =
                    IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string());

                let public_data_payee = state
                    .contact_service
                    .get_identity_by_name(&bill_payload.payee_name)
                    .await
                    .map_err(|_| {
                        service::Error::Validation(String::from("Can not get payee identity."))
                    })?;

                (public_data_drawee, public_data_payee)
            }
            // Drawer is neither drawee nor payee
            (false, false) => {
                let public_data_drawee = state
                    .contact_service
                    .get_identity_by_name(&bill_payload.drawee_name)
                    .await
                    .map_err(|_| {
                        service::Error::Validation(String::from("Can not get drawee identity."))
                    })?;

                let public_data_payee = state
                    .contact_service
                    .get_identity_by_name(&bill_payload.payee_name)
                    .await
                    .map_err(|_| {
                        service::Error::Validation(String::from("Can not get payee identity."))
                    })?;
                (public_data_drawee, public_data_payee)
            }
        };

    if public_data_drawee.name.is_empty() {
        return Err(service::Error::Validation(String::from(
            "Drawee not found.",
        )));
    }

    if public_data_payee.name.is_empty() {
        return Err(service::Error::Validation(String::from("Payee not found.")));
    }

    if public_data_drawee.name == public_data_payee.name {
        return Err(service::Error::Validation(String::from(
            "Drawee and payee can't be the same.",
        )));
    }

    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;
    let bill = state
        .bill_service
        .issue_new_bill(
            bill_payload.bill_jurisdiction.to_owned(),
            bill_payload.place_of_drawing.to_owned(),
            bill_payload.amount_numbers.to_owned(),
            bill_payload.place_of_payment.to_owned(),
            bill_payload.maturity_date.to_owned(),
            bill_payload.currency_code.to_owned(),
            drawer,
            bill_payload.language.to_owned(),
            public_data_drawee,
            public_data_payee,
            bill_payload.file_upload_id.to_owned(),
            timestamp,
        )
        .await?;

    state
        .bill_service
        .propagate_bill(
            &bill.name,
            &bill.drawer.peer_id,
            &bill.drawee.peer_id,
            &bill.payee.peer_id,
        )
        .await?;

    // If we're the drawee, we immediately accept the bill
    if bill.drawer == bill.drawee {
        let timestamp_accept = external::time::TimeApi::get_atomic_time().await?.timestamp;
        state
            .bill_service
            .accept_bill(&bill.name, timestamp_accept)
            .await?;
    }

    Ok(Status::Ok)
}

#[put("/sell", format = "json", data = "<sell_bill_payload>")]
pub async fn sell_bill(
    state: &State<ServiceContext>,
    sell_bill_payload: Json<SellBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }

    let public_data_buyer = state
        .contact_service
        .get_identity_by_name(&sell_bill_payload.buyer)
        .await?;

    if public_data_buyer.name.is_empty() {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;

    let chain = state
        .bill_service
        .sell_bitcredit_bill(
            &sell_bill_payload.bill_name,
            public_data_buyer.clone(),
            timestamp,
            sell_bill_payload.amount_numbers,
        )
        .await?;

    state
        .bill_service
        .propagate_block(&sell_bill_payload.bill_name, chain.get_latest_block())
        .await?;

    state
        .bill_service
        .propagate_bill_for_node(
            &sell_bill_payload.bill_name,
            &public_data_buyer.peer_id.to_string(),
        )
        .await?;
    Ok(Status::Ok)
}

#[put("/endorse", format = "json", data = "<endorse_bill_payload>")]
pub async fn endorse_bill(
    state: &State<ServiceContext>,
    endorse_bill_payload: Json<EndorseBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }

    let public_data_endorsee = state
        .contact_service
        .get_identity_by_name(&endorse_bill_payload.endorsee)
        .await?;

    if public_data_endorsee.name.is_empty() {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;

    let chain = state
        .bill_service
        .endorse_bitcredit_bill(
            &endorse_bill_payload.bill_name,
            public_data_endorsee.clone(),
            timestamp,
        )
        .await?;

    state
        .bill_service
        .propagate_block(&endorse_bill_payload.bill_name, chain.get_latest_block())
        .await?;

    state
        .bill_service
        .propagate_bill_for_node(
            &endorse_bill_payload.bill_name,
            &public_data_endorsee.peer_id.to_string(),
        )
        .await?;
    Ok(Status::Ok)
}

#[put(
    "/request_to_pay",
    format = "json",
    data = "<request_to_pay_bill_payload>"
)]
pub async fn request_to_pay_bill(
    state: &State<ServiceContext>,
    request_to_pay_bill_payload: Json<RequestToPayBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;

    let chain = state
        .bill_service
        .request_pay(&request_to_pay_bill_payload.bill_name, timestamp)
        .await?;

    state
        .bill_service
        .propagate_block(
            &request_to_pay_bill_payload.bill_name,
            chain.get_latest_block(),
        )
        .await?;
    Ok(Status::Ok)
}

#[put(
    "/request_to_accept",
    format = "json",
    data = "<request_to_accept_bill_payload>"
)]
pub async fn request_to_accept_bill(
    state: &State<ServiceContext>,
    request_to_accept_bill_payload: Json<RequestToAcceptBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;

    let chain = state
        .bill_service
        .request_acceptance(&request_to_accept_bill_payload.bill_name, timestamp)
        .await?;
    state
        .bill_service
        .propagate_block(
            &request_to_accept_bill_payload.bill_name,
            chain.get_latest_block(),
        )
        .await?;
    Ok(Status::Ok)
}

#[put("/accept", format = "json", data = "<accept_bill_payload>")]
pub async fn accept_bill(
    state: &State<ServiceContext>,
    accept_bill_payload: Json<AcceptBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;
    let chain = state
        .bill_service
        .accept_bill(&accept_bill_payload.bill_name, timestamp)
        .await?;
    state
        .bill_service
        .propagate_block(&accept_bill_payload.bill_name, chain.get_latest_block())
        .await?;
    Ok(Status::Ok)
}

// Mint

//PUT
// #[post("/try_mint", format = "json", data = "<mint_bill_payload>")]
// pub async fn try_mint_bill(
//     state: &State<ServiceContext>,
//     mint_bill_payload: Json<MintBitcreditBillPayload>,
// ) -> Status {
//     if !state.identity_service.identity_exists().await {
//         Err(service::Error::PreconditionFailed)
//     } else {
//         let mut client = state.inner().clone();
//
//         let public_mint_node =
//             get_identity_public_data(mint_bill_payload.mint_node.clone(), client.clone()).await;
//
//         if !public_mint_node.name.is_empty() {
//             client
//                 .add_bill_to_dht_for_node(
//                     &mint_bill_payload.bill_name,
//                     &public_mint_node.peer_id.to_string().clone(),
//                 )
//                 .await;
//
//             Status::Ok
//         } else {
//             Status::NotAcceptable
//         }
//     }
// }

//PUT
//TODO: add try_mint_bill here?
#[put(
    "/request_to_mint",
    format = "json",
    data = "<request_to_mint_bill_payload>"
)]
pub async fn request_to_mint_bill(
    state: &State<ServiceContext>,
    request_to_mint_bill_payload: Json<RequestToMintBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let public_mint_node = state
        .contact_service
        .get_identity_by_name(&request_to_mint_bill_payload.mint_node)
        .await?;
    if !public_mint_node.name.is_empty() {
        state
            .bill_service
            .propagate_bill_for_node(
                &request_to_mint_bill_payload.bill_name,
                &public_mint_node.peer_id.to_string(),
            )
            .await?;
    }
    let bill_keys = state
        .bill_service
        .get_bill_keys(&request_to_mint_bill_payload.bill_name)
        .await?;

    // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
    // thread, but this logic will be replaced soon
    thread::spawn(move || {
        request_to_mint_bitcredit(request_to_mint_bill_payload.into_inner(), bill_keys)
    })
    .join()
    .expect("Thread panicked");
    Ok(Status::Ok)
}

//This is function for mint software
#[put("/accept_mint", format = "json", data = "<accept_mint_bill_payload>")]
pub async fn accept_mint_bill(
    state: &State<ServiceContext>,
    accept_mint_bill_payload: Json<AcceptMintBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let bill = state
        .bill_service
        .get_bill(&accept_mint_bill_payload.bill_name)
        .await?;
    let bill_amount = bill.amount_numbers;
    let holder_node_id = bill.payee.peer_id.clone();

    //TODO: calculate percent
    // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
    // thread, but this logic will be replaced soon
    thread::spawn(move || {
        accept_mint_bitcredit(
            bill_amount,
            accept_mint_bill_payload.bill_name.clone(),
            holder_node_id,
        )
    })
    .join()
    .expect("Thread panicked");

    Ok(Status::Ok)
}

//After accept mint on client side
#[put("/mint", format = "json", data = "<mint_bill_payload>")]
pub async fn mint_bill(
    state: &State<ServiceContext>,
    mint_bill_payload: Json<MintBitcreditBillPayload>,
) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }
    let timestamp = external::time::TimeApi::get_atomic_time().await?.timestamp;

    let public_mint_node = state
        .contact_service
        .get_identity_by_name(&mint_bill_payload.mint_node)
        .await?;

    if public_mint_node.name.is_empty() {
        return Err(service::Error::PreconditionFailed);
    }
    let chain = state
        .bill_service
        .mint_bitcredit_bill(
            &mint_bill_payload.bill_name,
            public_mint_node.clone(),
            timestamp,
        )
        .await?;

    state
        .bill_service
        .propagate_block(&mint_bill_payload.bill_name, chain.get_latest_block())
        .await?;

    state
        .bill_service
        .propagate_bill_for_node(
            &mint_bill_payload.bill_name,
            &public_mint_node.peer_id.to_string(),
        )
        .await?;

    Ok(Status::Ok)
}
