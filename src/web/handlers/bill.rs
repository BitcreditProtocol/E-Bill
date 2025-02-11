use super::middleware::IdentityCheck;
use crate::blockchain::Blockchain;
use crate::external::mint::{accept_mint_bitcredit, request_to_mint_bitcredit};
use crate::service::bill_service::{LightBitcreditBillToReturn, RecourseReason};
use crate::service::{contact_service::IdentityPublicData, Result};
use crate::util::date::date_string_to_i64_timestamp;
use crate::util::file::{detect_content_type_for_bytes, UploadFileHandler};
use crate::util::{self, base58_encode, BcrKeys};
use crate::web::data::{
    AcceptBitcreditBillPayload, AcceptMintBitcreditBillPayload, BillCombinedBitcoinKey, BillId,
    BillNumbersToWordsForSum, BillType, BillsResponse, BillsSearchFilterPayload,
    BitcreditBillPayload, EndorseBitcreditBillPayload, EndorsementsResponse,
    MintBitcreditBillPayload, OfferToSellBitcreditBillPayload, PastEndorseesResponse,
    RejectActionBillPayload, RequestRecourseForAcceptancePayload, RequestRecourseForPaymentPayload,
    RequestToAcceptBitcreditBillPayload, RequestToMintBitcreditBillPayload,
    RequestToPayBitcreditBillPayload, UploadBillFilesForm, UploadFilesResponse,
};
use crate::{external, service};
use crate::{service::bill_service::BitcreditBillToReturn, service::ServiceContext};
use log::error;
use rocket::form::Form;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use rocket::{get, post, put, State};
use std::thread;

pub async fn get_current_identity_node_id(state: &State<ServiceContext>) -> String {
    let current_identity = state.get_current_identity().await;
    match current_identity.company {
        None => current_identity.personal,
        Some(company_node_id) => company_node_id,
    }
}

pub async fn get_signer_public_data_and_keys(
    state: &State<ServiceContext>,
) -> Result<(IdentityPublicData, BcrKeys)> {
    let current_identity = state.get_current_identity().await;
    let local_node_id = current_identity.personal;
    let (signer_public_data, signer_keys) = match current_identity.company {
        None => {
            let identity = state.identity_service.get_full_identity().await?;
            match IdentityPublicData::new(identity.identity) {
                Some(identity_public_data) => (identity_public_data, identity.key_pair),
                None => {
                    return Err(service::Error::Validation(String::from(
                        "Drawer is not a bill issuer - does not have a postal address set",
                    )));
                }
            }
        }
        Some(company_node_id) => {
            let (company, keys) = state
                .company_service
                .get_company_and_keys_by_id(&company_node_id)
                .await?;
            if !company.signatories.contains(&local_node_id) {
                return Err(service::Error::Validation(format!(
                    "Signer {local_node_id} for company {company_node_id} is not signatory",
                )));
            }
            (
                IdentityPublicData::from(company),
                BcrKeys::from_private_key(&keys.private_key)?,
            )
        }
    };
    Ok((signer_public_data, signer_keys))
}

#[utoipa::path(
    tag = "Endorsements",
    path = "/bill/endorsements/{id}",
    description = "Get endorsements of the given bill",
    responses(
        (status = 200, description = "Endorsements", body = EndorsementsResponse)
    )
)]
#[get("/endorsements/<id>")]
pub async fn get_endorsements_for_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<EndorsementsResponse>> {
    let result = state
        .bill_service
        .get_endorsements(id, &get_current_identity_node_id(state).await)
        .await?;
    Ok(Json(EndorsementsResponse {
        endorsements: result,
    }))
}

#[utoipa::path(
    tag = "Past Endorsees",
    path = "/bill/past_endorsees/{id}",
    description = "Get all past endorsees of the given bill",
    responses(
        (status = 200, description = "Past Endorsees", body = PastEndorseesResponse)
    )
)]
#[get("/past_endorsees/<id>")]
pub async fn get_past_endorsees_for_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<PastEndorseesResponse>> {
    let result = state
        .bill_service
        .get_past_endorsees(id, &get_current_identity_node_id(state).await)
        .await?;
    Ok(Json(PastEndorseesResponse {
        past_endorsees: result,
    }))
}

#[get("/holder/<id>")]
pub async fn holder(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<bool>> {
    let bill_id_u8 = hex::decode(id).unwrap();
    let bill_id_base58 = base58_encode(&bill_id_u8);

    let identity = state.identity_service.get_full_identity().await?;
    let bill = state.bill_service.get_bill(&bill_id_base58).await?;
    let am_i_holder = identity.identity.node_id.eq(&bill.payee.node_id);
    Ok(Json(am_i_holder))
}

#[get("/bitcoin_key/<id>")]
pub async fn bitcoin_key(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<BillCombinedBitcoinKey>> {
    let (caller_public_data, caller_keys) = get_signer_public_data_and_keys(state).await?;
    let combined_key = state
        .bill_service
        .get_combined_bitcoin_key_for_bill(id, &caller_public_data, &caller_keys)
        .await?;
    Ok(Json(combined_key))
}

#[get("/attachment/<bill_id>/<file_name>")]
pub async fn attachment(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    bill_id: &str,
    file_name: &str,
) -> Result<(ContentType, Vec<u8>)> {
    let keys = state.bill_service.get_bill_keys(bill_id).await?;
    let file_bytes = state
        .bill_service
        .open_and_decrypt_attached_file(bill_id, file_name, &keys.private_key)
        .await
        .map_err(|_| service::Error::NotFound)?;

    let content_type = match detect_content_type_for_bytes(&file_bytes) {
        None => None,
        Some(t) => ContentType::parse_flexible(&t),
    }
    .ok_or(service::Error::Validation(String::from(
        "Content Type of the requested file could not be determined",
    )))?;

    Ok((content_type, file_bytes))
}

#[utoipa::path(
    tag = "Bills Search",
    path = "/bill/search",
    description = "Get all bill details for the given filter",
    responses(
        (status = 200, description = "Search for bills", body = BillsResponse<LightBitcreditBillToReturn>)
    )
)]
#[post("/search", format = "json", data = "<bills_filter>")]
pub async fn search(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    bills_filter: Json<BillsSearchFilterPayload>,
) -> Result<Json<BillsResponse<LightBitcreditBillToReturn>>> {
    let filter = bills_filter.0.filter;
    let (from, to) = match filter.date_range {
        None => (None, None),
        Some(date_range) => {
            let from: Option<u64> =
                util::date::date_string_to_i64_timestamp(&date_range.from, None).map(|v| v as u64);
            // Change the date to the end of the day, so we collect bills during the day as well
            let to: Option<u64> = util::date::date_string_to_i64_timestamp(&date_range.to, None)
                .and_then(|v| util::date::end_of_day_as_timestamp(v as u64).map(|v| v as u64));
            (from, to)
        }
    };
    let bills = state
        .bill_service
        .search_bills(
            &filter.currency,
            &filter.search_term,
            from,
            to,
            &filter.role,
            &get_current_identity_node_id(state).await,
        )
        .await?;
    Ok(Json(BillsResponse { bills }))
}

#[utoipa::path(
    tag = "Bills Light",
    path = "/bill/list/light",
    description = "Get all bill details in a light version",
    responses(
        (status = 200, description = "List of bills light", body = BillsResponse<LightBitcreditBillToReturn>)
    )
)]
#[get("/list/light")]
pub async fn list_light(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Json<BillsResponse<LightBitcreditBillToReturn>>> {
    let bills = state
        .bill_service
        .get_bills(&get_current_identity_node_id(state).await)
        .await?;
    Ok(Json(BillsResponse {
        bills: bills.into_iter().map(|b| b.into()).collect(),
    }))
}

#[utoipa::path(
    tag = "Bills",
    path = "/bill/list",
    description = "Get all bill details",
    responses(
        (status = 200, description = "List of bills", body = BillsResponse<BitcreditBillToReturn>)
    )
)]
#[get("/list")]
pub async fn list(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Json<BillsResponse<BitcreditBillToReturn>>> {
    let bills = state
        .bill_service
        .get_bills(&get_current_identity_node_id(state).await)
        .await?;
    Ok(Json(BillsResponse { bills }))
}

#[utoipa::path(
    tag = "All Bills from all identities",
    path = "/bill/list_all",
    description = "Get all local bills regardless of the selected identity",
    responses(
        (status = 200, description = "List of all local bills", body = BillsResponse<BitcreditBillToReturn>)
    )
)]
#[get("/list_all")]
pub async fn all_bills_from_all_identities(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Json<BillsResponse<BitcreditBillToReturn>>> {
    let bills = state.bill_service.get_bills_from_all_identities().await?;
    Ok(Json(BillsResponse { bills }))
}

#[get("/numbers_to_words_for_sum/<id>")]
pub async fn numbers_to_words_for_sum(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<BillNumbersToWordsForSum>> {
    let bill = state.bill_service.get_bill(id).await?;
    let sum = bill.sum;
    let sum_as_words = util::numbers_to_words::encode(&sum);
    Ok(Json(BillNumbersToWordsForSum { sum, sum_as_words }))
}

#[get("/dht/<bill_id>")]
pub async fn find_and_sync_with_bill_in_dht(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    bill_id: &str,
) -> Result<Status> {
    state
        .bill_service
        .find_and_sync_with_bill_in_dht(bill_id)
        .await?;
    Ok(Status::Ok)
}

#[utoipa::path(
    tag = "Bills",
    path = "/bill/{id}",
    description = "Get bill details by id",
    params(
        ("id" = String, Path, description = "Id of the bill to return")
    ),
    responses(
        (status = 200, description = "The Bill with given id", body = BitcreditBillToReturn),
        (status = 404, description = "Bill not found")
    )
)]
#[get("/detail/<id>")]
pub async fn bill_detail(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    id: &str,
) -> Result<Json<BitcreditBillToReturn>> {
    let current_timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let identity = state.identity_service.get_identity().await?;
    let bill_detail = state
        .bill_service
        .get_detail(
            id,
            &identity,
            &get_current_identity_node_id(state).await,
            current_timestamp,
        )
        .await?;
    Ok(Json(bill_detail))
}

#[get("/check_payment")]
pub async fn check_payment(state: &State<ServiceContext>) -> Result<Status> {
    if !state.identity_service.identity_exists().await {
        return Err(service::Error::PreconditionFailed);
    }

    if let Err(e) = state.bill_service.check_bills_payment().await {
        error!("Error while checking bills payment: {e}");
    }

    if let Err(e) = state.bill_service.check_bills_offer_to_sell_payment().await {
        error!("Error while checking bills offer to sell payment: {e}");
    }

    Ok(Status::Ok)
}

#[get("/dht")]
pub async fn check_dht_for_bills(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
) -> Result<Status> {
    let mut client = state.dht_client();
    client.check_new_bills().await?;

    Ok(Status::Ok)
}

#[post("/upload_files", data = "<files_upload_form>")]
pub async fn upload_files(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    files_upload_form: Form<UploadBillFilesForm<'_>>,
) -> Result<Json<UploadFilesResponse>> {
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
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    bill_payload: Json<BitcreditBillPayload>,
) -> Result<Json<BillId>> {
    let sum = util::currency::parse_sum(&bill_payload.sum)?;

    if util::date::date_string_to_i64_timestamp(&bill_payload.issue_date, None).is_none() {
        return Err(service::Error::Validation(String::from(
            "invalid issue date",
        )));
    }

    if util::date::date_string_to_i64_timestamp(&bill_payload.maturity_date, None).is_none() {
        return Err(service::Error::Validation(String::from(
            "invalid maturity date",
        )));
    }

    let (drawer_public_data, drawer_keys) = get_signer_public_data_and_keys(state).await?;

    let bill_type = BillType::try_from(bill_payload.t)?;

    if bill_payload.drawee == bill_payload.payee {
        return Err(service::Error::Validation(String::from(
            "Drawee can't be Payee at the same time",
        )));
    }

    let (public_data_drawee, public_data_payee) = match bill_type {
        // Drawer is payee
        BillType::SelfDrafted => {
            let public_data_drawee = match state
                .contact_service
                .get_identity_by_node_id(&bill_payload.drawee)
                .await
            {
                Ok(Some(drawee)) => drawee,
                Ok(None) | Err(_) => {
                    return Err(service::Error::Validation(String::from(
                        "Can not get drawee identity from contacts.",
                    )));
                }
            };

            let public_data_payee = drawer_public_data.clone();

            (public_data_drawee, public_data_payee)
        }
        // Drawer is drawee
        BillType::PromissoryNote => {
            let public_data_drawee = drawer_public_data.clone();

            let public_data_payee = match state
                .contact_service
                .get_identity_by_node_id(&bill_payload.payee)
                .await
            {
                Ok(Some(drawee)) => drawee,
                Ok(None) | Err(_) => {
                    return Err(service::Error::Validation(String::from(
                        "Can not get payee identity from contacts.",
                    )));
                }
            };

            (public_data_drawee, public_data_payee)
        }
        // Drawer is neither drawee nor payee
        BillType::ThreeParties => {
            let public_data_drawee = match state
                .contact_service
                .get_identity_by_node_id(&bill_payload.drawee)
                .await
            {
                Ok(Some(drawee)) => drawee,
                Ok(None) | Err(_) => {
                    return Err(service::Error::Validation(String::from(
                        "Can not get drawee identity from contacts.",
                    )));
                }
            };

            let public_data_payee = match state
                .contact_service
                .get_identity_by_node_id(&bill_payload.payee)
                .await
            {
                Ok(Some(drawee)) => drawee,
                Ok(None) | Err(_) => {
                    return Err(service::Error::Validation(String::from(
                        "Can not get payee identity from contacts.",
                    )));
                }
            };

            (public_data_drawee, public_data_payee)
        }
    };

    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let bill = state
        .bill_service
        .issue_new_bill(
            bill_payload.country_of_issuing.to_owned(),
            bill_payload.city_of_issuing.to_owned(),
            bill_payload.issue_date.to_owned(),
            bill_payload.maturity_date.to_owned(),
            public_data_drawee,
            public_data_payee,
            sum,
            bill_payload.currency.to_owned(),
            bill_payload.country_of_payment.to_owned(),
            bill_payload.city_of_payment.to_owned(),
            bill_payload.language.to_owned(),
            bill_payload.file_upload_id.to_owned(),
            drawer_public_data.clone(),
            drawer_keys.clone(),
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    let bill_clone = bill.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_bill(
                &bill_clone.id,
                &bill_clone.drawer.node_id,
                &bill_clone.drawee.node_id,
                &bill_clone.payee.node_id,
            )
            .await
        {
            error!("Error propagating bill on DHT: {e}");
        }
    });

    // If we're the drawee, we immediately accept the bill
    if bill.drawer == bill.drawee {
        let timestamp_accept = external::time::TimeApi::get_atomic_time().await.timestamp;
        state
            .bill_service
            .accept_bill(
                &bill.id,
                &drawer_public_data,
                &drawer_keys,
                timestamp_accept,
            )
            .await?;
    }

    Ok(Json(BillId {
        id: bill.id.clone(),
    }))
}

#[put("/offer_to_sell", format = "json", data = "<offer_to_sell_payload>")]
pub async fn offer_to_sell_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    offer_to_sell_payload: Json<OfferToSellBitcreditBillPayload>,
) -> Result<Status> {
    let public_data_buyer = match state
        .contact_service
        .get_identity_by_node_id(&offer_to_sell_payload.buyer)
        .await
    {
        Ok(Some(buyer)) => buyer,
        Ok(None) | Err(_) => {
            return Err(service::Error::Validation(String::from(
                "Can not get buyer identity from contacts.",
            )));
        }
    };

    let sum = util::currency::parse_sum(&offer_to_sell_payload.sum)?;
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .offer_to_sell_bitcredit_bill(
            &offer_to_sell_payload.bill_id,
            public_data_buyer.clone(),
            sum,
            &offer_to_sell_payload.currency,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&offer_to_sell_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }

        if let Err(e) = bill_service_clone
            .propagate_bill_for_node(
                &offer_to_sell_payload.bill_id,
                &public_data_buyer.node_id.to_string(),
            )
            .await
        {
            error!("Error propagating bill for node on DHT: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put("/endorse", format = "json", data = "<endorse_bill_payload>")]
pub async fn endorse_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    endorse_bill_payload: Json<EndorseBitcreditBillPayload>,
) -> Result<Status> {
    let public_data_endorsee = match state
        .contact_service
        .get_identity_by_node_id(&endorse_bill_payload.endorsee)
        .await
    {
        Ok(Some(endorsee)) => endorsee,
        Ok(None) | Err(_) => {
            return Err(service::Error::Validation(String::from(
                "Can not get endorsee identity from contacts.",
            )));
        }
    };

    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;
    let chain = state
        .bill_service
        .endorse_bitcredit_bill(
            &endorse_bill_payload.bill_id,
            public_data_endorsee.clone(),
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&endorse_bill_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }

        if let Err(e) = bill_service_clone
            .propagate_bill_for_node(
                &endorse_bill_payload.bill_id,
                &public_data_endorsee.node_id.to_string(),
            )
            .await
        {
            error!("Error propagating bill for node on DHT: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put(
    "/request_to_pay",
    format = "json",
    data = "<request_to_pay_bill_payload>"
)]
pub async fn request_to_pay_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    request_to_pay_bill_payload: Json<RequestToPayBitcreditBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .request_pay(
            &request_to_pay_bill_payload.bill_id,
            &request_to_pay_bill_payload.currency,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(
                &request_to_pay_bill_payload.bill_id,
                chain.get_latest_block(),
            )
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put(
    "/request_to_accept",
    format = "json",
    data = "<request_to_accept_bill_payload>"
)]
pub async fn request_to_accept_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    request_to_accept_bill_payload: Json<RequestToAcceptBitcreditBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .request_acceptance(
            &request_to_accept_bill_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();

    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(
                &request_to_accept_bill_payload.bill_id,
                chain.get_latest_block(),
            )
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put("/accept", format = "json", data = "<accept_bill_payload>")]
pub async fn accept_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    accept_bill_payload: Json<AcceptBitcreditBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .accept_bill(
            &accept_bill_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&accept_bill_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

// Mint

#[put(
    "/request_to_mint",
    format = "json",
    data = "<request_to_mint_bill_payload>"
)]
pub async fn request_to_mint_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    request_to_mint_bill_payload: Json<RequestToMintBitcreditBillPayload>,
) -> Result<Status> {
    let public_mint_node = state
        .contact_service
        .get_identity_by_node_id(&request_to_mint_bill_payload.mint_node)
        .await?;
    if let Some(public_mint) = public_mint_node {
        state
            .bill_service
            .propagate_bill_for_node(&request_to_mint_bill_payload.bill_id, &public_mint.node_id)
            .await?;
    }
    let bill_keys = state
        .bill_service
        .get_bill_keys(&request_to_mint_bill_payload.bill_id)
        .await?;

    let bill = state
        .bill_service
        .get_bill(&request_to_mint_bill_payload.bill_id)
        .await?;

    let maturity_date_timestamp = date_string_to_i64_timestamp(&bill.maturity_date, None).unwrap();

    // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
    // thread, but this logic will be replaced soon
    thread::spawn(move || {
        request_to_mint_bitcredit(
            request_to_mint_bill_payload.into_inner(),
            bill_keys,
            maturity_date_timestamp,
            bill.sum,
        )
    })
    .join()
    .expect("Thread panicked");
    Ok(Status::Ok)
}

//This is function for mint software
#[put("/accept_mint", format = "json", data = "<accept_mint_bill_payload>")]
pub async fn accept_mint_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    accept_mint_bill_payload: Json<AcceptMintBitcreditBillPayload>,
) -> Result<Status> {
    let bill = state
        .bill_service
        .get_bill(&accept_mint_bill_payload.bill_id)
        .await?;
    let holder_node_id = bill.payee.node_id.clone();

    let sum = util::currency::parse_sum(&accept_mint_bill_payload.sum)?;

    //TODO: calculate percent
    // Usage of thread::spawn is necessary here, because we spawn a new tokio runtime in the
    // thread, but this logic will be replaced soon
    thread::spawn(move || {
        accept_mint_bitcredit(
            sum,
            accept_mint_bill_payload.bill_id.clone(),
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
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    mint_bill_payload: Json<MintBitcreditBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let sum = util::currency::parse_sum(&mint_bill_payload.sum)?;

    let public_mint_node = match state
        .contact_service
        .get_identity_by_node_id(&mint_bill_payload.mint_node)
        .await
    {
        Ok(Some(drawee)) => drawee,
        Ok(None) | Err(_) => {
            return Err(service::Error::Validation(String::from(
                "Can not get public mint node identity from contacts.",
            )));
        }
    };
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .mint_bitcredit_bill(
            &mint_bill_payload.bill_id,
            sum,
            &mint_bill_payload.currency,
            public_mint_node.clone(),
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&mint_bill_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }

        if let Err(e) = bill_service_clone
            .propagate_bill_for_node(
                &mint_bill_payload.bill_id,
                &public_mint_node.node_id.to_string(),
            )
            .await
        {
            error!("Error propagating bill for node on DHT: {e}");
        }
    });

    Ok(Status::Ok)
}

// Rejection
#[put("/reject_to_accept", format = "json", data = "<reject_payload>")]
pub async fn reject_to_accept_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    reject_payload: Json<RejectActionBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .reject_acceptance(
            &reject_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();

    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&reject_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put("/reject_to_pay", format = "json", data = "<reject_payload>")]
pub async fn reject_to_pay_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    reject_payload: Json<RejectActionBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .reject_payment(
            &reject_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();

    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&reject_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put("/reject_to_buy", format = "json", data = "<reject_payload>")]
pub async fn reject_to_buy_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    reject_payload: Json<RejectActionBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .reject_buying(
            &reject_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();

    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&reject_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

#[put("/reject_to_pay_recourse", format = "json", data = "<reject_payload>")]
pub async fn reject_to_pay_recourse_bill(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    reject_payload: Json<RejectActionBillPayload>,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let chain = state
        .bill_service
        .reject_payment_for_recourse(
            &reject_payload.bill_id,
            &signer_public_data,
            &signer_keys,
            timestamp,
        )
        .await?;

    let bill_service_clone = state.bill_service.clone();

    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&reject_payload.bill_id, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}

// Recourse
#[put(
    "/request_recourse_for_payment",
    format = "json",
    data = "<request_recourse_payload>"
)]
pub async fn request_to_recourse_bill_payment(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    request_recourse_payload: Json<RequestRecourseForPaymentPayload>,
) -> Result<Status> {
    let sum = util::currency::parse_sum(&request_recourse_payload.sum)?;
    request_recourse(
        state,
        RecourseReason::Pay(sum, request_recourse_payload.currency.clone()),
        &request_recourse_payload.bill_id,
        &request_recourse_payload.recoursee,
    )
    .await
}

#[put(
    "/request_recourse_for_acceptance",
    format = "json",
    data = "<request_recourse_payload>"
)]
pub async fn request_to_recourse_bill_acceptance(
    _identity: IdentityCheck,
    state: &State<ServiceContext>,
    request_recourse_payload: Json<RequestRecourseForAcceptancePayload>,
) -> Result<Status> {
    request_recourse(
        state,
        RecourseReason::Accept,
        &request_recourse_payload.bill_id,
        &request_recourse_payload.recoursee,
    )
    .await
}

async fn request_recourse(
    state: &State<ServiceContext>,
    recourse_reason: RecourseReason,
    bill_id: &str,
    recoursee_node_id: &str,
) -> Result<Status> {
    let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
    let (signer_public_data, signer_keys) = get_signer_public_data_and_keys(state).await?;

    let public_data_recoursee = match state
        .contact_service
        .get_identity_by_node_id(recoursee_node_id)
        .await
    {
        Ok(Some(buyer)) => buyer,
        Ok(None) | Err(_) => {
            return Err(service::Error::Validation(String::from(
                "Can not get recoursee identity from contacts.",
            )));
        }
    };

    let chain = state
        .bill_service
        .request_recourse(
            bill_id,
            &public_data_recoursee,
            &signer_public_data,
            &signer_keys,
            recourse_reason,
            timestamp,
        )
        .await?;

    let bill_id_clone = bill_id.to_owned();
    let bill_service_clone = state.bill_service.clone();
    tokio::spawn(async move {
        if let Err(e) = bill_service_clone
            .propagate_block(&bill_id_clone, chain.get_latest_block())
            .await
        {
            error!("Error propagating block: {e}");
        }
    });
    Ok(Status::Ok)
}
