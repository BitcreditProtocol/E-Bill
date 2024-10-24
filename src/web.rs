#![feature(proc_macro_hygiene, decl_macro)]

use std::path::Path;
use std::str::FromStr;
use std::{fs, thread};

use bitcoin::secp256k1::Scalar;
use chrono::{Days, Utc};
use libp2p::PeerId;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::form::Form;
use rocket::http::{Header, Status};
use rocket::serde::json::Json;
use rocket::{Request, Response, State};
use rocket_dyn_templates::{context, handlebars, Template};

use crate::blockchain::{Chain, ChainToReturn, GossipsubEvent, GossipsubEventId, OperationCode};
use crate::constants::{BILLS_FOLDER_PATH, BILL_VALIDITY_PERIOD, IDENTITY_FILE_PATH, USEDNET};
use crate::service::ServiceContext;
use crate::service::{contact_service::*, Error};
use crate::work_with_mint::{
    accept_mint_bitcredit, check_bitcredit_quote, client_accept_bitcredit_quote,
    request_to_mint_bitcredit,
};
use crate::{
    accept_bill, api, blockchain, create_whole_identity, endorse_bitcredit_bill, get_bills,
    get_bills_for_list, get_quote_from_map, get_whole_identity, issue_new_bill,
    issue_new_bill_drawer_is_drawee, issue_new_bill_drawer_is_payee, mint_bitcredit_bill,
    read_bill_from_file, read_identity_from_file, read_peer_id_from_file, request_acceptance,
    request_pay, sell_bitcredit_bill, write_identity_to_file, AcceptBitcreditBillForm,
    AcceptMintBitcreditBillForm, BitcreditBill, BitcreditBillForm, BitcreditBillToReturn,
    BitcreditEbillQuote, DeleteContactForm, EditContactForm, EndorseBitcreditBillForm, Identity,
    IdentityForm, IdentityPublicData, IdentityWithAll, MintBitcreditBillForm, NewContactForm,
    NodeId, RequestToAcceptBitcreditBillForm, RequestToMintBitcreditBillForm,
    RequestToPayBitcreditBillForm, SellBitcreditBillForm,
};

use self::handlebars::{Handlebars, JsonRender};

#[get("/")]
pub async fn exit() {
    std::process::exit(0x0100);
}

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

#[get("/return")]
pub async fn return_contacts(state: &State<ServiceContext>) -> Json<Vec<Contact>> {
    let contacts: Vec<Contact> = state
        .contact_service
        .get_contacts()
        .await
        .expect("Can not get contacts.");
    Json(contacts)
}

#[get("/holder/<id>")]
pub async fn holder(id: String) -> Json<bool> {
    let identity: IdentityWithAll = get_whole_identity();
    let bill: BitcreditBill = read_bill_from_file(&id);
    let am_i_holder = identity.peer_id.to_string().eq(&bill.payee.peer_id);
    Json(am_i_holder)
}

#[get("/return")]
pub async fn return_bills_list() -> Json<Vec<BitcreditBillToReturn>> {
    let bills: Vec<BitcreditBillToReturn> = get_bills_for_list();
    Json(bills)
}

#[post("/create", data = "<identity_form>")]
pub async fn create_identity(
    identity_form: Form<IdentityForm>,
    state: &State<ServiceContext>,
) -> Status {
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

    let mut client = state.dht_client();
    let _: IdentityWithAll = get_whole_identity();
    let _ = get_bills();
    client.put_identity_public_data_in_dht().await;

    Status::Ok
}

#[put("/change", data = "<identity_form>")]
pub async fn change_identity(
    identity_form: Form<IdentityForm>,
    state: &State<ServiceContext>,
) -> Status {
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
    let mut client = state.dht_client();
    client.put_identity_public_data_in_dht().await;

    Status::Ok
}

#[get("/return/basic/<id>")]
pub async fn return_basic_bill(id: String) -> Json<BitcreditBill> {
    let bill: BitcreditBill = read_bill_from_file(&id);
    Json(bill)
}

#[get("/chain/return/<id>")]
pub async fn return_chain_of_blocks(id: String) -> Json<Chain> {
    let chain = Chain::read_chain_from_file(&id);
    Json(chain)
}

#[get("/return")]
pub async fn return_operation_codes() -> Json<Vec<OperationCode>> {
    Json(OperationCode::get_all_operation_codes())
}

//PUT
// #[post("/try_mint", data = "<mint_bill_form>")]
// pub async fn try_mint_bill(
//     state: &State<Client>,
//     mint_bill_form: Form<MintBitcreditBillForm>,
// ) -> Status {
//     if !Path::new(IDENTITY_FILE_PATH).exists() {
//         Status::NotAcceptable
//     } else {
//         let mut client = state.inner().clone();
//
//         let public_mint_node =
//             get_identity_public_data(mint_bill_form.mint_node.clone(), client.clone()).await;
//
//         if !public_mint_node.name.is_empty() {
//             client
//                 .add_bill_to_dht_for_node(
//                     &mint_bill_form.bill_name,
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

#[get("/find/<bill_id>")]
pub async fn find_bill_in_dht(state: &State<ServiceContext>, bill_id: String) {
    let mut client = state.dht_client();
    let bill_bytes = client.get_bill(bill_id.to_string().clone()).await;
    if !bill_bytes.is_empty() {
        let path = BILLS_FOLDER_PATH.to_string() + "/" + &bill_id + ".json";
        fs::write(path, bill_bytes.clone()).expect("Can't write file.");
    }
}

//PUT
//TODO: add try_mint_bill here?
#[put("/request_to_mint", data = "<request_to_mint_bill_form>")]
pub async fn request_to_mint_bill(
    state: &State<ServiceContext>,
    request_to_mint_bill_form: Form<RequestToMintBitcreditBillForm>,
) -> Status {
    let mut client = state.dht_client();
    let public_mint_node = state
        .contact_service
        .get_identity_by_name(&request_to_mint_bill_form.mint_node)
        .await
        .expect("Can not get mint node identity.");
    if !public_mint_node.name.is_empty() {
        client
            .add_bill_to_dht_for_node(
                &request_to_mint_bill_form.bill_name,
                &public_mint_node.peer_id.to_string().clone(),
            )
            .await;
    }

    thread::spawn(move || request_to_mint_bitcredit(request_to_mint_bill_form.clone()))
        .join()
        .expect("Thread panicked");
    Status::Ok
}

//This is function for mint software
#[put("/accept_mint", data = "<accept_mint_bill_form>")]
pub async fn accept_mint_bill(accept_mint_bill_form: Form<AcceptMintBitcreditBillForm>) -> Status {
    let bill = read_bill_from_file(&accept_mint_bill_form.bill_name.clone());
    let bill_amount = bill.amount_numbers;
    let holder_node_id = bill.payee.peer_id.clone();

    //TODO: calculate percent
    thread::spawn(move || {
        accept_mint_bitcredit(
            bill_amount,
            accept_mint_bill_form.bill_name.clone(),
            holder_node_id,
        )
    })
    .join()
    .expect("Thread panicked");

    Status::Ok
}

//After accept mint on client side
#[put("/mint", data = "<mint_bill_form>")]
pub async fn mint_bill(
    state: &State<ServiceContext>,
    mint_bill_form: Form<MintBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

        let public_mint_node = state
            .contact_service
            .get_identity_by_name(&mint_bill_form.mint_node)
            .await
            .expect("Can not get mint node identity.");

        if !public_mint_node.name.is_empty() {
            let correct = mint_bitcredit_bill(
                &mint_bill_form.bill_name,
                public_mint_node.clone(),
                timestamp,
            )
            .await;

            if correct {
                let chain: Chain = Chain::read_chain_from_file(&mint_bill_form.bill_name);
                let block = chain.get_latest_block();

                let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
                let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
                let message = event.to_byte_array();

                client
                    .add_message_to_topic(message, mint_bill_form.bill_name.clone())
                    .await;

                client
                    .add_bill_to_dht_for_node(
                        &mint_bill_form.bill_name,
                        &public_mint_node.peer_id.to_string().clone(),
                    )
                    .await;
            } else {
                println!("Can't mint");
            }

            Status::Ok
        } else {
            Status::NotAcceptable
        }
    }
}

#[get("/return/<id>")]
pub async fn return_quote(id: String) -> Json<BitcreditEbillQuote> {
    let mut quote = get_quote_from_map(&id);
    let copy_id = id.clone();
    if !quote.bill_id.is_empty() && quote.quote_id.is_empty() {
        thread::spawn(move || check_bitcredit_quote(&copy_id))
            .join()
            .expect("Thread panicked");
    }
    quote = get_quote_from_map(&id);
    Json(quote)
}

#[put("/accept/<id>")]
pub async fn accept_quote(state: &State<ServiceContext>, id: String) -> Json<BitcreditEbillQuote> {
    let mut quote = get_quote_from_map(&id);

    let public_data_endorsee = state
        .contact_service
        .get_identity_by_name(&quote.mint_node_id)
        .await
        .expect("Could not get endorsee public data");

    if !public_data_endorsee.name.is_empty() {
        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;
        endorse_bitcredit_bill(&quote.bill_id, public_data_endorsee.clone(), timestamp);
    }

    let copy_id = id.clone();
    if !quote.bill_id.is_empty() && !quote.quote_id.is_empty() {
        thread::spawn(move || client_accept_bitcredit_quote(&copy_id))
            .join()
            .expect("Thread panicked");
    }
    quote = get_quote_from_map(&id);
    Json(quote)
}

#[get("/return/<id>")]
pub async fn return_bill(id: String) -> Json<BitcreditBillToReturn> {
    let identity: IdentityWithAll = get_whole_identity();
    let bill: BitcreditBill = read_bill_from_file(&id);
    let chain = Chain::read_chain_from_file(&bill.name);
    let drawer = chain.get_drawer();
    let mut link_for_buy = "".to_string();
    let chain_to_return = ChainToReturn::new(chain.clone());
    let endorsed = chain.exist_block_with_operation_code(OperationCode::Endorse);
    let accepted = chain.exist_block_with_operation_code(OperationCode::Accept);
    let mut address_for_selling: String = String::new();
    let mut amount_for_selling = 0;
    let waiting_for_payment = chain.waiting_for_payment();
    let mut payment_deadline_has_passed = false;
    let mut waited_for_payment = waiting_for_payment.0;
    if waited_for_payment {
        payment_deadline_has_passed = chain.check_if_payment_deadline_has_passed().await;
    }
    if payment_deadline_has_passed {
        waited_for_payment = false;
    }
    let mut buyer = waiting_for_payment.1;
    let mut seller = waiting_for_payment.2;
    if waited_for_payment
        && (identity.peer_id.to_string().eq(&buyer.peer_id)
            || identity.peer_id.to_string().eq(&seller.peer_id))
    {
        address_for_selling = waiting_for_payment.3;
        amount_for_selling = waiting_for_payment.4;
        let message: String = format!("Payment in relation to a bill {}", bill.name.clone());
        link_for_buy =
            generate_link_to_pay(address_for_selling.clone(), amount_for_selling, message).await;
    } else {
        buyer = IdentityPublicData::new_empty();
        seller = IdentityPublicData::new_empty();
    }
    let requested_to_pay = chain.exist_block_with_operation_code(OperationCode::RequestToPay);
    let requested_to_accept = chain.exist_block_with_operation_code(OperationCode::RequestToAccept);
    let address_to_pay = get_address_to_pay(bill.clone());
    //TODO: add last_sell_block_paid
    let check_if_already_paid = check_if_paid(address_to_pay.clone(), bill.amount_numbers).await;
    let mut check_if_already_paid = (false, 0u64);
    if requested_to_pay {
        check_if_already_paid =
            check_if_paid(address_to_pay.clone(), bill.amount_numbers.clone()).await;
    }
    let payed = check_if_already_paid.0;
    let mut number_of_confirmations: u64 = 0;
    let mut pending = false;
    if payed && check_if_already_paid.1.eq(&0) {
        pending = true;
    } else if payed && !check_if_already_paid.1.eq(&0) {
        let transaction = api::get_transactions_testet(address_to_pay.clone()).await;
        let txid = api::Txid::get_first_transaction(transaction.clone()).await;
        let height = api::get_testnet_last_block_height().await;
        number_of_confirmations = height - txid.status.block_height + 1;
    }
    let address_to_pay = get_address_to_pay(bill.clone());
    let message: String = format!("Payment in relation to a bill {}", bill.name.clone());
    let link_to_pay =
        generate_link_to_pay(address_to_pay.clone(), bill.amount_numbers, message).await;
    let mut pr_key_bill = String::new();
    if !endorsed
        && bill
            .payee
            .bitcoin_public_key
            .clone()
            .eq(&identity.identity.bitcoin_public_key)
    {
        pr_key_bill = get_current_payee_private_key(identity.identity.clone(), bill.clone());
    } else if endorsed
        && bill
            .endorsee
            .bitcoin_public_key
            .eq(&identity.identity.bitcoin_public_key)
    {
        pr_key_bill = get_current_payee_private_key(identity.identity.clone(), bill.clone());
    }

    let full_bill = BitcreditBillToReturn {
        name: bill.name,
        to_payee: bill.to_payee,
        bill_jurisdiction: bill.bill_jurisdiction,
        timestamp_at_drawing: bill.timestamp_at_drawing,
        drawee: bill.drawee,
        drawer,
        payee: bill.payee,
        endorsee: bill.endorsee,
        place_of_drawing: bill.place_of_drawing,
        currency_code: bill.currency_code,
        amount_numbers: bill.amount_numbers,
        amounts_letters: bill.amounts_letters,
        maturity_date: bill.maturity_date,
        date_of_issue: bill.date_of_issue,
        compounding_interest_rate: bill.compounding_interest_rate,
        type_of_interest_calculation: bill.type_of_interest_calculation,
        place_of_payment: bill.place_of_payment,
        public_key: bill.public_key,
        private_key: bill.private_key,
        language: bill.language,
        accepted,
        endorsed,
        requested_to_pay,
        requested_to_accept,
        waited_for_payment,
        address_for_selling,
        amount_for_selling,
        buyer,
        seller,
        payed,
        link_for_buy,
        link_to_pay,
        address_to_pay,
        pr_key_bill,
        number_of_confirmations,
        pending,
        chain_of_blocks: chain_to_return,
    };
    Json(full_bill)
}

async fn generate_link_to_pay(address: String, amount: u64, message: String) -> String {
    //todo check what net we used
    let link = format!("bitcoin:{}?amount={}&message={}", address, amount, message);
    link
}

pub async fn check_if_paid(address: String, amount: u64) -> (bool, u64) {
    //todo check what net we used
    let info_about_address = api::AddressInfo::get_testnet_address_info(address.clone()).await;
    let received_summ = info_about_address.chain_stats.funded_txo_sum;
    let spent_summ = info_about_address.chain_stats.spent_txo_sum;
    let received_summ_mempool = info_about_address.mempool_stats.funded_txo_sum;
    let spent_summ_mempool = info_about_address.mempool_stats.spent_txo_sum;
    if amount.eq(&(received_summ + spent_summ + received_summ_mempool + spent_summ_mempool)) {
        (true, received_summ)
    } else {
        (false, 0)
    }
}

pub fn get_address_to_pay(bill: BitcreditBill) -> String {
    let public_key_bill = bitcoin::PublicKey::from_str(&bill.public_key).unwrap();

    let mut person_to_pay = bill.payee.clone();

    if !bill.endorsee.name.is_empty() {
        person_to_pay = bill.endorsee.clone();
    }

    let public_key_holder = person_to_pay.bitcoin_public_key;
    let public_key_bill_holder = bitcoin::PublicKey::from_str(&public_key_holder).unwrap();

    let public_key_bill = public_key_bill
        .inner
        .combine(&public_key_bill_holder.inner)
        .unwrap();
    let pub_key_bill = bitcoin::PublicKey::new(public_key_bill);

    bitcoin::Address::p2pkh(&pub_key_bill, USEDNET).to_string()
}

fn get_current_payee_private_key(identity: Identity, bill: BitcreditBill) -> String {
    let private_key_bill = bitcoin::PrivateKey::from_str(&bill.private_key).unwrap();

    let private_key_bill_holder =
        bitcoin::PrivateKey::from_str(&identity.bitcoin_private_key).unwrap();

    let privat_key_bill = private_key_bill
        .inner
        .add_tweak(&Scalar::from(private_key_bill_holder.inner))
        .unwrap();

    bitcoin::PrivateKey::new(privat_key_bill, USEDNET).to_string()
}

#[get("/dht")]
pub async fn search_bill(state: &State<ServiceContext>) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();
        let local_peer_id = read_peer_id_from_file();
        client.check_new_bills(local_peer_id.to_string()).await;

        Status::Ok
    }
}

#[post("/issue", data = "<bill_form>")]
pub async fn issue_bill(
    state: &State<ServiceContext>,
    bill_form: Form<BitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut status: Status = Status::Ok;

        let form_bill = bill_form.into_inner();
        let drawer = get_whole_identity();
        let mut client = state.dht_client();
        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;
        let mut bill = BitcreditBill::new_empty();

        if form_bill.drawer_is_payee {
            let public_data_drawee = state
                .contact_service
                .get_identity_by_name(&form_bill.drawee_name)
                .await
                .expect("Can not get drawee identity.");

            if !public_data_drawee.name.is_empty() {
                bill = issue_new_bill_drawer_is_payee(
                    form_bill.bill_jurisdiction,
                    form_bill.place_of_drawing,
                    form_bill.amount_numbers,
                    form_bill.place_of_payment,
                    form_bill.maturity_date,
                    form_bill.currency_code,
                    drawer.clone(),
                    form_bill.language,
                    public_data_drawee,
                    timestamp,
                );
            } else {
                status = Status::NotAcceptable
            }
        } else if form_bill.drawer_is_drawee {
            let public_data_payee = state
                .contact_service
                .get_identity_by_name(&form_bill.payee_name)
                .await
                .expect("Can not get payee identity.");

            if !public_data_payee.name.is_empty() {
                bill = issue_new_bill_drawer_is_drawee(
                    form_bill.bill_jurisdiction,
                    form_bill.place_of_drawing,
                    form_bill.amount_numbers,
                    form_bill.place_of_payment,
                    form_bill.maturity_date,
                    form_bill.currency_code,
                    drawer.clone(),
                    form_bill.language,
                    public_data_payee,
                    timestamp,
                );
            } else {
                status = Status::NotAcceptable
            }
        } else {
            let public_data_drawee = state
                .contact_service
                .get_identity_by_name(&form_bill.drawee_name)
                .await
                .expect("Can not get drawee identity.");

            let public_data_payee = state
                .contact_service
                .get_identity_by_name(&form_bill.payee_name)
                .await
                .expect("Can not get payee public data");

            if !public_data_payee.name.is_empty() && !public_data_drawee.name.is_empty() {
                bill = issue_new_bill(
                    form_bill.bill_jurisdiction,
                    form_bill.place_of_drawing,
                    form_bill.amount_numbers,
                    form_bill.place_of_payment,
                    form_bill.maturity_date,
                    form_bill.currency_code,
                    drawer.clone(),
                    form_bill.language,
                    public_data_drawee,
                    public_data_payee,
                    timestamp,
                );
            } else {
                status = Status::NotAcceptable
            }
        }

        if status.eq(&Status::Ok) {
            let mut nodes: Vec<String> = Vec::new();
            let my_peer_id = drawer.peer_id.to_string().clone();
            nodes.push(my_peer_id.to_string());
            nodes.push(bill.drawee.peer_id.clone());
            nodes.push(bill.payee.peer_id.clone());

            for node in nodes {
                if !node.is_empty() {
                    println!("Add {} for node {}", &bill.name, &node);
                    client.add_bill_to_dht_for_node(&bill.name, &node).await;
                }
            }

            client.subscribe_to_topic(bill.name.clone()).await;

            client.put(&bill.name).await;

            if form_bill.drawer_is_drawee {
                let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

                let correct = accept_bill(&bill.name, timestamp);

                if correct {
                    let chain: Chain = Chain::read_chain_from_file(&bill.name);
                    let block = chain.get_latest_block();

                    let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
                    let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
                    let message = event.to_byte_array();

                    client
                        .add_message_to_topic(message, bill.name.clone())
                        .await;
                }
            }
        }

        status
    }
}

#[put("/sell", data = "<sell_bill_form>")]
pub async fn sell_bill(
    state: &State<ServiceContext>,
    sell_bill_form: Form<SellBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let public_data_buyer = state
            .contact_service
            .get_identity_by_name(&sell_bill_form.buyer)
            .await
            .expect("Can not get buyer identity.");

        if !public_data_buyer.name.is_empty() {
            let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

            let correct = sell_bitcredit_bill(
                &sell_bill_form.bill_name,
                public_data_buyer.clone(),
                timestamp,
                sell_bill_form.amount_numbers,
            );

            if correct {
                let chain: Chain = Chain::read_chain_from_file(&sell_bill_form.bill_name);
                let block = chain.get_latest_block();

                let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
                let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
                let message = event.to_byte_array();

                client
                    .add_message_to_topic(message, sell_bill_form.bill_name.clone())
                    .await;

                client
                    .add_bill_to_dht_for_node(
                        &sell_bill_form.bill_name,
                        &public_data_buyer.peer_id.to_string().clone(),
                    )
                    .await;
            }
            Status::Ok
        } else {
            Status::NotAcceptable
        }
    }
}

#[put("/endorse", data = "<endorse_bill_form>")]
pub async fn endorse_bill(
    state: &State<ServiceContext>,
    endorse_bill_form: Form<EndorseBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let public_data_endorsee = state
            .contact_service
            .get_identity_by_name(&endorse_bill_form.endorsee)
            .await
            .expect("Can not get endorsee identity.");

        if !public_data_endorsee.name.is_empty() {
            let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

            let correct = endorse_bitcredit_bill(
                &endorse_bill_form.bill_name,
                public_data_endorsee.clone(),
                timestamp,
            );

            if correct {
                let chain: Chain = Chain::read_chain_from_file(&endorse_bill_form.bill_name);
                let block = chain.get_latest_block();

                let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
                let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
                let message = event.to_byte_array();

                client
                    .add_message_to_topic(message, endorse_bill_form.bill_name.clone())
                    .await;

                client
                    .add_bill_to_dht_for_node(
                        &endorse_bill_form.bill_name,
                        &public_data_endorsee.peer_id.to_string().clone(),
                    )
                    .await;
            }

            let _ = get_bills();
            let _: Identity = read_identity_from_file();

            Status::Ok
        } else {
            Status::NotAcceptable
        }
    }
}

#[put("/request_to_pay", data = "<request_to_pay_bill_form>")]
pub async fn request_to_pay_bill(
    state: &State<ServiceContext>,
    request_to_pay_bill_form: Form<RequestToPayBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

        let correct = request_pay(&request_to_pay_bill_form.bill_name, timestamp);

        if correct {
            let chain: Chain = Chain::read_chain_from_file(&request_to_pay_bill_form.bill_name);
            let block = chain.get_latest_block();

            let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
            let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
            let message = event.to_byte_array();

            client
                .add_message_to_topic(message, request_to_pay_bill_form.bill_name.clone())
                .await;
        }
        Status::Ok
    }
}

#[put("/request_to_accept", data = "<request_to_accept_bill_form>")]
pub async fn request_to_accept_bill(
    state: &State<ServiceContext>,
    request_to_accept_bill_form: Form<RequestToAcceptBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

        let correct = request_acceptance(&request_to_accept_bill_form.bill_name, timestamp);

        if correct {
            let chain: Chain = Chain::read_chain_from_file(&request_to_accept_bill_form.bill_name);
            let block = chain.get_latest_block();

            let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
            let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
            let message = event.to_byte_array();

            client
                .add_message_to_topic(message, request_to_accept_bill_form.bill_name.clone())
                .await;
        }
        Status::Ok
    }
}

#[put("/accept", data = "<accept_bill_form>")]
pub async fn accept_bill_form(
    state: &State<ServiceContext>,
    accept_bill_form: Form<AcceptBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.dht_client();

        let timestamp = api::TimeApi::get_atomic_time().await.timestamp;

        let correct = accept_bill(&accept_bill_form.bill_name, timestamp);

        if correct {
            let chain: Chain = Chain::read_chain_from_file(&accept_bill_form.bill_name);
            let block = chain.get_latest_block();

            let block_bytes = serde_json::to_vec(block).expect("Error serializing block");
            let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
            let message = event.to_byte_array();

            client
                .add_message_to_topic(message, accept_bill_form.bill_name.clone())
                .await;
        }
        Status::Ok
    }
}

#[delete("/remove", data = "<remove_contact_form>")]
pub async fn remove_contact(
    remove_contact_form: Form<DeleteContactForm>,
    state: &State<ServiceContext>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        state
            .contact_service
            .delete_identity_by_name(&remove_contact_form.name)
            .await
            .unwrap_or(());
        Status::Ok
    }
}

#[post("/new", data = "<new_contact_form>")]
pub async fn new_contact(
    state: &State<ServiceContext>,
    new_contact_form: Form<NewContactForm>,
) -> Result<Json<Vec<Contact>>, Error> {
    let mut client = state.dht_client();
    let dht_identity = client
        .get_identity_public_data_from_dht(new_contact_form.node_id.clone())
        .await;

    let identity = if dht_identity.name.is_empty() {
        IdentityPublicData::new_only_peer_id(new_contact_form.node_id.clone())
    } else {
        dht_identity.to_owned()
    };

    state
        .contact_service
        .create_identity(&new_contact_form.name, identity)
        .await?;
    let result = state.contact_service.get_contacts().await?;
    Ok(Json(result))
}

#[put("/edit", data = "<edit_contact_form>")]
pub async fn edit_contact(
    edit_contact_form: Form<EditContactForm>,
    state: &State<ServiceContext>,
) -> Result<Json<Vec<Contact>>, Error> {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Err(Error::PreconditionFailed)
    } else {
        state
            .contact_service
            .update_identity_name(&edit_contact_form.old_name, &edit_contact_form.name)
            .await?;

        let contacts = state.contact_service.get_contacts().await?;

        Ok(Json(contacts))
    }
}

#[catch(404)]
pub fn not_found(req: &Request) -> String {
    format!("We couldn't find the requested path '{}'", req.uri())
}

pub fn customize(hbs: &mut Handlebars) {
    hbs.register_helper("wow", Box::new(wow_helper));
    hbs.register_template_string(
        "hbs/about.html",
        r#"
        {{#*inline "page"}}
        <section id="about">
        <h1>About - Here's another page!</h1>
        </section>
        {{/inline}}
        {{> hbs/layout}}
    "#,
    )
    .expect("valid HBS template");
}

fn wow_helper(
    h: &handlebars::Helper<'_, '_>,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext<'_, '_>,
    out: &mut dyn handlebars::Output,
) -> handlebars::HelperResult {
    if let Some(param) = h.param(0) {
        out.write("<b><i>")?;
        out.write(&param.value().render())?;
        out.write("</b></i>")?;
    }

    Ok(())
}

pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to responses",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "POST, GET, PATCH, OPTIONS",
        ));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}
