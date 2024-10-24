use super::super::data::{
    AcceptBitcreditBillForm, AcceptMintBitcreditBillForm, BitcreditBillForm,
    EndorseBitcreditBillForm, MintBitcreditBillForm, RequestToAcceptBitcreditBillForm,
    RequestToMintBitcreditBillForm, RequestToPayBitcreditBillForm, SellBitcreditBillForm,
};
use crate::bill::{
    accept_bill,
    contacts::{get_current_payee_private_key, get_identity_public_data, IdentityPublicData},
    endorse_bitcredit_bill, get_bills_for_list,
    identity::{get_whole_identity, read_peer_id_from_file, IdentityWithAll},
    issue_new_bill, issue_new_bill_drawer_is_drawee, issue_new_bill_drawer_is_payee,
    mint_bitcredit_bill, read_bill_from_file, request_acceptance, request_pay, sell_bitcredit_bill,
    BitcreditBill, BitcreditBillToReturn,
};
use crate::blockchain::{Chain, ChainToReturn, GossipsubEvent, GossipsubEventId, OperationCode};
use crate::constants::{BILLS_FOLDER_PATH, IDENTITY_FILE_PATH};
use crate::dht::Client;
use crate::external;
use crate::external::mint::{accept_mint_bitcredit, request_to_mint_bitcredit};
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, post, put, State};
use std::path::Path;
use std::{fs, thread};

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

#[get("/find/<bill_id>")]
pub async fn find_bill_in_dht(state: &State<Client>, bill_id: String) {
    let mut client = state.inner().clone();
    let bill_bytes = client.get_bill(bill_id.to_string().clone()).await;
    if !bill_bytes.is_empty() {
        let path = BILLS_FOLDER_PATH.to_string() + "/" + &bill_id + ".json";
        fs::write(path, bill_bytes.clone()).expect("Can't write file.");
    }
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
        link_for_buy = external::bitcoin::generate_link_to_pay(
            address_for_selling.clone(),
            amount_for_selling,
            message,
        )
        .await;
    } else {
        buyer = IdentityPublicData::new_empty();
        seller = IdentityPublicData::new_empty();
    }
    let requested_to_pay = chain.exist_block_with_operation_code(OperationCode::RequestToPay);
    let requested_to_accept = chain.exist_block_with_operation_code(OperationCode::RequestToAccept);
    let address_to_pay = external::bitcoin::get_address_to_pay(bill.clone());
    //TODO: add last_sell_block_paid
    // let check_if_already_paid =
    //     external::bitcoin::check_if_paid(address_to_pay.clone(), bill.amount_numbers).await;
    let mut check_if_already_paid = (false, 0u64);
    if requested_to_pay {
        check_if_already_paid =
            external::bitcoin::check_if_paid(address_to_pay.clone(), bill.amount_numbers).await;
    }
    let payed = check_if_already_paid.0;
    let mut number_of_confirmations: u64 = 0;
    let mut pending = false;
    if payed && check_if_already_paid.1.eq(&0) {
        pending = true;
    } else if payed && !check_if_already_paid.1.eq(&0) {
        let transaction = external::bitcoin::get_transactions(address_to_pay.clone()).await;
        let txid = external::bitcoin::Txid::get_first_transaction(transaction.clone()).await;
        let height = external::bitcoin::get_last_block_height().await;
        number_of_confirmations = height - txid.status.block_height + 1;
    }
    let address_to_pay = external::bitcoin::get_address_to_pay(bill.clone());
    let message: String = format!("Payment in relation to a bill {}", bill.name.clone());
    let link_to_pay = external::bitcoin::generate_link_to_pay(
        address_to_pay.clone(),
        bill.amount_numbers,
        message,
    )
    .await;
    let mut pr_key_bill = String::new();
    if (!endorsed
        && bill
            .payee
            .bitcoin_public_key
            .clone()
            .eq(&identity.identity.bitcoin_public_key))
        || (endorsed
            && bill
                .endorsee
                .bitcoin_public_key
                .eq(&identity.identity.bitcoin_public_key))
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

#[get("/dht")]
pub async fn search_bill(state: &State<Client>) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();
        let local_peer_id = read_peer_id_from_file();
        client.check_new_bills(local_peer_id.to_string()).await;

        Status::Ok
    }
}

#[post("/issue", data = "<bill_form>")]
pub async fn issue_bill(state: &State<Client>, bill_form: Form<BitcreditBillForm>) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut status: Status = Status::Ok;

        let form_bill = bill_form.into_inner();
        let drawer = get_whole_identity();
        let mut client = state.inner().clone();
        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;
        let mut bill = BitcreditBill::new_empty();

        if form_bill.drawer_is_payee {
            let public_data_drawee =
                get_identity_public_data(form_bill.drawee_name, client.clone()).await;

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
            let public_data_payee =
                get_identity_public_data(form_bill.payee_name, client.clone()).await;

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
            let public_data_drawee =
                get_identity_public_data(form_bill.drawee_name, client.clone()).await;

            let public_data_payee =
                get_identity_public_data(form_bill.payee_name, client.clone()).await;

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
                let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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
    state: &State<Client>,
    sell_bill_form: Form<SellBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let public_data_buyer =
            get_identity_public_data(sell_bill_form.buyer.clone(), client.clone()).await;

        if !public_data_buyer.name.is_empty() {
            let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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
    state: &State<Client>,
    endorse_bill_form: Form<EndorseBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let public_data_endorsee =
            get_identity_public_data(endorse_bill_form.endorsee.clone(), client.clone()).await;

        if !public_data_endorsee.name.is_empty() {
            let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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

            Status::Ok
        } else {
            Status::NotAcceptable
        }
    }
}

#[put("/request_to_pay", data = "<request_to_pay_bill_form>")]
pub async fn request_to_pay_bill(
    state: &State<Client>,
    request_to_pay_bill_form: Form<RequestToPayBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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
    state: &State<Client>,
    request_to_accept_bill_form: Form<RequestToAcceptBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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
    state: &State<Client>,
    accept_bill_form: Form<AcceptBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

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

// Mint

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

//PUT
//TODO: add try_mint_bill here?
#[put("/request_to_mint", data = "<request_to_mint_bill_form>")]
pub async fn request_to_mint_bill(
    state: &State<Client>,
    request_to_mint_bill_form: Form<RequestToMintBitcreditBillForm>,
) -> Status {
    let mut client = state.inner().clone();
    let public_mint_node =
        get_identity_public_data(request_to_mint_bill_form.mint_node.clone(), client.clone()).await;
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
pub async fn accept_mint_bill(
    _state: &State<Client>,
    accept_mint_bill_form: Form<AcceptMintBitcreditBillForm>,
) -> Status {
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
    state: &State<Client>,
    mint_bill_form: Form<MintBitcreditBillForm>,
) -> Status {
    if !Path::new(IDENTITY_FILE_PATH).exists() {
        Status::NotAcceptable
    } else {
        let mut client = state.inner().clone();

        let timestamp = external::time::TimeApi::get_atomic_time().await.timestamp;

        let public_mint_node =
            get_identity_public_data(mint_bill_form.mint_node.clone(), client.clone()).await;

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
