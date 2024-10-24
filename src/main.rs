extern crate core;
#[macro_use]
extern crate rocket;

use std::collections::HashMap;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::{env, fs, mem, thread};

use anyhow::Result;
use bitcoin::PublicKey;
use borsh::{self, BorshDeserialize, BorshSerialize};
use chrono::Utc;
use clap::Parser;
use config::Config;
use libp2p::identity::Keypair;
use libp2p::PeerId;
use moksha_core::primitives::CheckBitcreditQuoteResponse;
use moksha_wallet::localstore::sqlite::SqliteLocalStore;
use openssl::pkey::{Private, Public};
use openssl::rsa;
use openssl::rsa::{Padding, Rsa};
use openssl::sha::sha256;
use rocket::fs::FileServer;
use rocket::serde::{Deserialize, Serialize};
use rocket::yansi::Paint;
use rocket::{Build, Rocket};
use service::contact_service::IdentityPublicData;
use service::{create_service_context, ServiceContext};

use crate::blockchain::{
    start_blockchain_for_new_bill, Block, Chain, ChainToReturn, OperationCode,
};
use crate::constants::{
    BILLS_FOLDER_PATH, BILLS_KEYS_FOLDER_PATH, BOOTSTRAP_FOLDER_PATH,
    COMPOUNDING_INTEREST_RATE_ZERO, IDENTITY_ED_25529_KEYS_FILE_PATH, IDENTITY_FILE_PATH,
    IDENTITY_FOLDER_PATH, IDENTITY_PEER_ID_FILE_PATH, QUOTES_MAP_FOLDER_PATH, QUOTE_MAP_FILE_PATH,
    USEDNET,
};
use crate::dht::network::Client;
use crate::numbers_to_words::encode;

mod api;
mod blockchain;
mod config;
mod constants;
mod dht;
mod error;
mod numbers_to_words;
mod persistence;
mod service;
mod test;
mod web;
mod work_with_mint;

// MAIN
#[tokio::main]
async fn main() -> Result<()> {
    env::set_var("RUST_BACKTRACE", "full");

    env_logger::init();

    // Parse command line arguments and env vars with clap
    let conf = Config::parse();

    init_folders();

    init_wallet().await;

    let mut dht = dht::dht_main(&conf).await.expect("DHT failed to start");

    let local_peer_id = read_peer_id_from_file();
    dht.check_new_bills(local_peer_id.to_string().clone()).await;
    dht.upgrade_table(local_peer_id.to_string().clone()).await;
    dht.subscribe_to_all_bills_topics().await;
    dht.put_bills_for_parties().await;
    dht.start_provide().await;
    dht.receive_updates_for_all_bills_topics().await;
    dht.put_identity_public_data_in_dht().await;

    let service_context = create_service_context(conf.clone(), dht.clone()).await?;
    let _rocket = rocket_main(service_context).launch().await.unwrap();
    Ok(())
}

fn rocket_main(context: ServiceContext) -> Rocket<Build> {
    let conf = context.config.clone();
    let rocket = rocket::build()
        .configure(
            rocket::Config::figment()
                .merge(("port", conf.http_port))
                .merge(("address", conf.http_address.to_owned())),
        )
        .register("/", catchers![web::not_found])
        .manage(context)
        .mount("/exit", routes![web::exit])
        .mount("/opcodes", routes![web::return_operation_codes])
        .mount(
            "/identity",
            routes![
                web::create_identity,
                web::change_identity,
                web::return_identity,
                web::return_peer_id
            ],
        )
        .mount("/bitcredit", FileServer::from("frontend_build"))
        .mount(
            "/contacts",
            routes![
                web::new_contact,
                web::edit_contact,
                web::remove_contact,
                web::return_contacts
            ],
        )
        .mount(
            "/bill",
            routes![
                web::holder,
                web::issue_bill,
                web::endorse_bill,
                web::search_bill,
                web::request_to_accept_bill,
                web::accept_bill_form,
                web::request_to_pay_bill,
                web::return_bill,
                web::return_chain_of_blocks,
                web::return_basic_bill,
                web::sell_bill,
                web::mint_bill,
                web::accept_mint_bill,
                web::find_bill_in_dht,
                web::request_to_mint_bill,
            ],
        )
        .mount("/bills", routes![web::return_bills_list,])
        .mount("/quote", routes![web::return_quote, web::accept_quote])
        .attach(web::CORS);

    match open::that(format!("{}/bitcredit/", conf.http_listen_url()).as_str()) {
        Ok(_) => {}
        Err(_) => {
            info!("Can't open browser.")
        }
    }

    rocket
}

fn init_folders() {
    if !Path::new(QUOTES_MAP_FOLDER_PATH).exists() {
        fs::create_dir(QUOTES_MAP_FOLDER_PATH).expect("Can't create folder quotes.");
    }
    if !Path::new(IDENTITY_FOLDER_PATH).exists() {
        fs::create_dir(IDENTITY_FOLDER_PATH).expect("Can't create folder identity.");
    }
    if !Path::new(BILLS_FOLDER_PATH).exists() {
        fs::create_dir(BILLS_FOLDER_PATH).expect("Can't create folder bills.");
    }
    if !Path::new(BILLS_KEYS_FOLDER_PATH).exists() {
        fs::create_dir(BILLS_KEYS_FOLDER_PATH).expect("Can't create folder bills_keys.");
    }
    if !Path::new(BOOTSTRAP_FOLDER_PATH).exists() {
        fs::create_dir(BOOTSTRAP_FOLDER_PATH).expect("Can't create folder bootstrap.");
    }
}

async fn init_wallet() {
    let dir = PathBuf::from("./data/wallet".to_string());
    if !dir.exists() {
        fs::create_dir_all(dir.clone()).unwrap();
    }
    let db_path = dir.join("wallet.db").to_str().unwrap().to_string();

    let localstore = SqliteLocalStore::with_path(db_path.clone())
        .await
        .expect("Cannot parse local store");

    // //TODO: take from params
    // let mint_url = Url::parse("http://127.0.0.1:3338").expect("Invalid url");
    //
    // let identity: Identity = read_identity_from_file();
    // let bitcoin_key = identity.bitcoin_public_key.clone();
    //
    // let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
    //     .with_localstore(localstore)
    //     .build()
    //     .await
    //     .expect("Could not create wallet");
}

pub fn read_quotes_map() -> HashMap<String, BitcreditEbillQuote> {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map();
    }
    let data: Vec<u8> = fs::read(QUOTE_MAP_FILE_PATH).expect("Unable to read quotes.");
    let quotes: HashMap<String, BitcreditEbillQuote> = HashMap::try_from_slice(&data).unwrap();
    quotes
}

pub fn create_quotes_map() {
    let quotes: HashMap<String, BitcreditEbillQuote> = HashMap::new();
    write_quotes_map(quotes);
}

pub fn write_quotes_map(map: HashMap<String, BitcreditEbillQuote>) {
    let quotes_byte = map.try_to_vec().unwrap();
    fs::write(QUOTE_MAP_FILE_PATH, quotes_byte).expect("Unable to write quote in file.");
}

pub fn add_in_quotes_map(quote: BitcreditEbillQuote) {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map();
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map();

    quotes.insert(quote.bill_id.clone(), quote);
    write_quotes_map(quotes);
}

fn get_quote_from_map(bill_id: &String) -> BitcreditEbillQuote {
    let quotes = read_quotes_map();
    if quotes.contains_key(bill_id) {
        let data = quotes.get(bill_id).unwrap().clone();
        data
    } else {
        BitcreditEbillQuote::new_empty()
    }
}

pub fn add_bitcredit_quote_and_amount_in_quotes_map(
    response: CheckBitcreditQuoteResponse,
    bill_id: String,
) {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map();
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map();
    let mut quote = get_quote_from_map(&bill_id);

    quote.amount = response.amount;
    quote.quote_id = response.quote.clone();

    quotes.remove(&bill_id);
    quotes.insert(bill_id.clone(), quote);
    write_quotes_map(quotes);
}

pub fn add_bitcredit_token_in_quotes_map(token: String, bill_id: String) {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map();
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map();
    let mut quote = get_quote_from_map(&bill_id);

    quote.token = token.clone();

    quotes.remove(&bill_id);
    quotes.insert(bill_id.clone(), quote);
    write_quotes_map(quotes);
}

//--------------------------------------------------------------

//-------------------------RSA----------------------------------
fn generation_rsa_key() -> Rsa<Private> {
    Rsa::generate(2048).unwrap()
}

fn pem_private_key_from_rsa(rsa: &Rsa<Private>) -> String {
    let private_key: Vec<u8> = rsa.private_key_to_pem().unwrap();
    String::from_utf8(private_key).unwrap()
}

fn pem_public_key_from_rsa(rsa: &Rsa<Private>) -> String {
    let public_key: Vec<u8> = rsa.public_key_to_pem().unwrap();
    String::from_utf8(public_key).unwrap()
}

fn private_key_from_pem_u8(private_key_u8: &[u8]) -> Rsa<Private> {
    rsa::Rsa::private_key_from_pem(private_key_u8).unwrap()
}

fn public_key_from_pem_u8(public_key_u8: &[u8]) -> Rsa<Public> {
    rsa::Rsa::public_key_from_pem(public_key_u8).unwrap()
}
//--------------------------------------------------------------

//-------------------------Bytes common-------------------------
fn encrypt_bytes_with_public_key(bytes: &[u8], public_key: String) -> Vec<u8> {
    let public_key = Rsa::public_key_from_pem(public_key.as_bytes()).unwrap();

    let key_size: usize = (public_key.size() / 2) as usize; //128

    let mut whole_encrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; key_size];
    let mut temp_buff_encrypted: Vec<u8> = vec![0; public_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;
    let remainder: usize = bytes.len() - key_size * number_of_key_size_in_whole_bill;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number: usize = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let _encrypted_len: usize = public_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff = vec![0; key_size];
        temp_buff_encrypted = vec![0; public_key.size() as usize];
    }

    if remainder != 0 {
        temp_buff = vec![0; remainder];

        let position: usize = key_size * number_of_key_size_in_whole_bill;
        let mut index_in_temp_buff: usize = 0;

        for i in position..bytes.len() {
            temp_buff[index_in_temp_buff] = bytes[i];
            index_in_temp_buff += 1;
        }

        index_in_temp_buff = 0;

        let _encrypted_len: usize = public_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff.clear();
        temp_buff_encrypted.clear();
    }

    whole_encrypted_buff
}

fn decrypt_bytes_with_private_key(bytes: &[u8], private_key: String) -> Vec<u8> {
    let private_key = Rsa::private_key_from_pem(private_key.as_bytes()).unwrap();

    let key_size: usize = private_key.size() as usize; //256

    let mut whole_decrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; private_key.size() as usize];
    let mut temp_buff_decrypted: Vec<u8> = vec![0; private_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;
    // let remainder = bill_bytes.len() - key_size * number_of_key_size_in_whole_bill;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let decrypted_len: usize = private_key
            .private_decrypt(&temp_buff, &mut temp_buff_decrypted, Padding::PKCS1)
            .unwrap();

        whole_decrypted_buff.append(&mut temp_buff_decrypted[0..decrypted_len].to_vec());
        temp_buff = vec![0; private_key.size() as usize];
        temp_buff_decrypted = vec![0; private_key.size() as usize];
    }

    // if remainder != 0 {
    //     let position = key_size * number_of_key_size_in_whole_bill;
    //     let mut index_in_temp_buff = 0;
    //
    //     for i in position..bill_bytes.len() {
    //         temp_buff[index_in_temp_buff] = bill_bytes[i];
    //         index_in_temp_buff = index_in_temp_buff + 1;
    //     }
    //
    //     index_in_temp_buff = 0;
    //
    //     let decrypted_len = rsa_key
    //         .public_decrypt(&*temp_buff, &mut temp_buff_decrypted, Padding::PKCS1)
    //         .unwrap();
    //
    //     whole_decrypted_buff.append(&mut temp_buff_decrypted);
    //     temp_buff.clear();
    //     temp_buff_decrypted.clear();
    // }

    whole_decrypted_buff
}

fn encrypt_bytes(bytes: &[u8], rsa_key: &Rsa<Private>) -> Vec<u8> {
    let key_size: usize = (rsa_key.size() / 2) as usize; //128

    let mut whole_encrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; key_size];
    let mut temp_buff_encrypted: Vec<u8> = vec![0; rsa_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;
    let remainder: usize = bytes.len() - key_size * number_of_key_size_in_whole_bill;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number: usize = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let _encrypted_len: usize = rsa_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff = vec![0; key_size];
        temp_buff_encrypted = vec![0; rsa_key.size() as usize];
    }

    if remainder != 0 {
        temp_buff = vec![0; remainder];

        let position: usize = key_size * number_of_key_size_in_whole_bill;
        let mut index_in_temp_buff: usize = 0;

        for i in position..bytes.len() {
            temp_buff[index_in_temp_buff] = bytes[i];
            index_in_temp_buff += 1;
        }

        index_in_temp_buff = 0;

        let _encrypted_len: usize = rsa_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff.clear();
        temp_buff_encrypted.clear();
    }

    whole_encrypted_buff
}

fn decrypt_bytes(bytes: &[u8], rsa_key: &Rsa<Private>) -> Vec<u8> {
    let key_size: usize = rsa_key.size() as usize; //256

    let mut whole_decrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; rsa_key.size() as usize];
    let mut temp_buff_decrypted: Vec<u8> = vec![0; rsa_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;
    // let remainder = bill_bytes.len() - key_size * number_of_key_size_in_whole_bill;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let decrypted_len: usize = rsa_key
            .private_decrypt(&temp_buff, &mut temp_buff_decrypted, Padding::PKCS1)
            .unwrap();

        whole_decrypted_buff.append(&mut temp_buff_decrypted[0..decrypted_len].to_vec());
        temp_buff = vec![0; rsa_key.size() as usize];
        temp_buff_decrypted = vec![0; rsa_key.size() as usize];
    }

    // if remainder != 0 {
    //     let position = key_size * number_of_key_size_in_whole_bill;
    //     let mut index_in_temp_buff = 0;
    //
    //     for i in position..bill_bytes.len() {
    //         temp_buff[index_in_temp_buff] = bill_bytes[i];
    //         index_in_temp_buff = index_in_temp_buff + 1;
    //     }
    //
    //     index_in_temp_buff = 0;
    //
    //     let decrypted_len = rsa_key
    //         .public_decrypt(&*temp_buff, &mut temp_buff_decrypted, Padding::PKCS1)
    //         .unwrap();
    //
    //     whole_decrypted_buff.append(&mut temp_buff_decrypted);
    //     temp_buff.clear();
    //     temp_buff_decrypted.clear();
    // }

    whole_decrypted_buff
}

unsafe fn structure_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const u8, size_of::<T>())
}

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| !s.starts_with("."))
        .unwrap_or(false)
}
//--------------------------------------------------------------

//-------------------------Identity-----------------------------

#[derive(BorshSerialize, BorshDeserialize, FromForm, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct NodeId {
    id: String,
}

impl NodeId {
    pub fn new(peer_id: String) -> Self {
        Self { id: peer_id }
    }
}

#[derive(Clone)]
pub struct IdentityWithAll {
    identity: Identity,
    peer_id: PeerId,
    key_pair: Keypair,
}

#[derive(BorshSerialize, BorshDeserialize, FromForm, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct Identity {
    name: String,
    company: String,
    date_of_birth: String,
    city_of_birth: String,
    country_of_birth: String,
    email: String,
    postal_address: String,
    public_key_pem: String,
    private_key_pem: String,
    bitcoin_public_key: String,
    bitcoin_private_key: String,
}

macro_rules! update_field {
    ($self:expr, $other:expr, $field:ident) => {
        if !$other.$field.is_empty() {
            $self.$field = $other.$field.clone();
        }
    };
}

impl Identity {
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            company: "".to_string(),
            date_of_birth: "".to_string(),
            city_of_birth: "".to_string(),
            bitcoin_public_key: "".to_string(),
            postal_address: "".to_string(),
            public_key_pem: "".to_string(),
            email: "".to_string(),
            country_of_birth: "".to_string(),
            private_key_pem: "".to_string(),
            bitcoin_private_key: "".to_string(),
        }
    }

    fn all_changeable_fields_empty(&self) -> bool {
        self.name.is_empty()
            && self.company.is_empty()
            && self.postal_address.is_empty()
            && self.email.is_empty()
    }

    fn all_changeable_fields_equal_to(&self, other: &Self) -> bool {
        self.name == other.name
            && self.company == other.company
            && self.postal_address == other.postal_address
            && self.email == other.email
    }

    fn update_valid(&self, other: &Self) -> bool {
        if other.all_changeable_fields_empty() {
            return false;
        }
        if self.all_changeable_fields_equal_to(other) {
            return false;
        }
        true
    }

    pub fn update_from(&mut self, other: &Identity) {
        update_field!(self, other, name);
        update_field!(self, other, company);
        update_field!(self, other, postal_address);
        update_field!(self, other, email);
    }
}

pub fn get_whole_identity() -> IdentityWithAll {
    let identity: Identity = read_identity_from_file();
    let ed25519_keys: Keypair = read_ed25519_keypair_from_file();
    let peer_id: PeerId = read_peer_id_from_file();

    IdentityWithAll {
        identity,
        peer_id,
        key_pair: ed25519_keys,
    }
}

pub fn create_whole_identity(
    name: String,
    company: String,
    date_of_birth: String,
    city_of_birth: String,
    country_of_birth: String,
    email: String,
    postal_address: String,
) -> IdentityWithAll {
    let identity = create_new_identity(
        name,
        company,
        date_of_birth,
        city_of_birth,
        country_of_birth,
        email,
        postal_address,
    );

    let ed25519_keys = read_ed25519_keypair_from_file();
    let peer_id = read_peer_id_from_file();

    write_identity_to_file(&identity);

    IdentityWithAll {
        identity,
        peer_id,
        key_pair: ed25519_keys,
    }
}

pub fn generate_dht_logic() {
    let ed25519_keys = Keypair::generate_ed25519();
    let peer_id = ed25519_keys.public().to_peer_id();

    write_dht_logic(&peer_id, &ed25519_keys);
}

fn write_dht_logic(peer_id: &PeerId, ed25519_keys: &Keypair) {
    write_peer_id_to_file(peer_id);
    write_ed25519_keypair_to_file(ed25519_keys);
}

fn create_new_identity(
    name: String,
    company: String,
    date_of_birth: String,
    city_of_birth: String,
    country_of_birth: String,
    email: String,
    postal_address: String,
) -> Identity {
    let rsa: Rsa<Private> = generation_rsa_key();
    let private_key_pem: String = pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = pem_public_key_from_rsa(&rsa);

    let s = bitcoin::secp256k1::Secp256k1::new();
    let private_key = bitcoin::PrivateKey::new(
        s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
            .0,
        USEDNET,
    );
    let public_key = private_key.public_key(&s).to_string();
    let private_key = private_key.to_string();

    Identity {
        name,
        company,
        date_of_birth,
        city_of_birth,
        country_of_birth,
        email,
        postal_address,
        public_key_pem,
        private_key_pem,
        bitcoin_public_key: public_key,
        bitcoin_private_key: private_key.clone(),
    }
}

fn write_identity_to_file(identity: &Identity) {
    let data: Vec<u8> = identity_to_byte_array(identity);
    fs::write(IDENTITY_FILE_PATH, data).expect("Unable to write file identity");
}

fn write_ed25519_keypair_to_file(ed25519_keys: &Keypair) {
    let data: &[u8] = unsafe { structure_as_u8_slice(ed25519_keys) };
    let data_sized = byte_array_to_size_array_keypair(data);
    fs::write(IDENTITY_ED_25529_KEYS_FILE_PATH, *data_sized)
        .expect("Unable to write keypair ed25519 in file");
}

fn write_peer_id_to_file(peer_id: &PeerId) {
    let data: &[u8] = unsafe { structure_as_u8_slice(peer_id) };
    let data_sized = byte_array_to_size_array_peer_id(data);
    fs::write(IDENTITY_PEER_ID_FILE_PATH, *data_sized).expect("Unable to write peer id in file");
}

fn read_identity_from_file() -> Identity {
    let data: Vec<u8> = fs::read(IDENTITY_FILE_PATH).expect("Unable to read file identity");
    identity_from_byte_array(&data)
}

fn read_ed25519_keypair_from_file() -> Keypair {
    let data: Vec<u8> =
        fs::read(IDENTITY_ED_25529_KEYS_FILE_PATH).expect("Unable to read file keypair");
    let key_pair_bytes_sized = byte_array_to_size_array_keypair(data.as_slice());
    let key_pair: Keypair = unsafe { mem::transmute_copy(key_pair_bytes_sized) };
    key_pair
}

fn read_peer_id_from_file() -> PeerId {
    let data: Vec<u8> =
        fs::read(IDENTITY_PEER_ID_FILE_PATH).expect("Unable to read file with peer id");
    let peer_id_bytes_sized = byte_array_to_size_array_peer_id(data.as_slice());
    let peer_id: PeerId = unsafe { mem::transmute_copy(peer_id_bytes_sized) };
    peer_id
}

fn identity_to_byte_array(identity: &Identity) -> Vec<u8> {
    identity.try_to_vec().unwrap()
}

fn identity_from_byte_array(identity: &[u8]) -> Identity {
    Identity::try_from_slice(identity).unwrap()
}

fn byte_array_to_size_array_keypair(array: &[u8]) -> &[u8; size_of::<Keypair>()] {
    array.try_into().expect("slice with incorrect length")
}

fn byte_array_to_size_array_peer_id(array: &[u8]) -> &[u8; size_of::<PeerId>()] {
    array.try_into().expect("slice with incorrect length")
}
//--------------------------------------------------------------

//-------------------------Bill---------------------------------
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBillToReturn {
    name: String,
    to_payee: bool,
    bill_jurisdiction: String,
    timestamp_at_drawing: i64,
    drawee: IdentityPublicData,
    drawer: IdentityPublicData,
    payee: IdentityPublicData,
    endorsee: IdentityPublicData,
    place_of_drawing: String,
    currency_code: String,
    amount_numbers: u64,
    amounts_letters: String,
    maturity_date: String,
    date_of_issue: String,
    compounding_interest_rate: u64,
    type_of_interest_calculation: bool,
    place_of_payment: String,
    public_key: String,
    private_key: String,
    language: String,
    accepted: bool,
    endorsed: bool,
    requested_to_pay: bool,
    requested_to_accept: bool,
    payed: bool,
    waited_for_payment: bool,
    address_for_selling: String,
    amount_for_selling: u64,
    buyer: IdentityPublicData,
    seller: IdentityPublicData,
    link_for_buy: String,
    link_to_pay: String,
    pr_key_bill: String,
    number_of_confirmations: u64,
    pending: bool,
    address_to_pay: String,
    chain_of_blocks: ChainToReturn,
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditEbillQuote {
    bill_id: String,
    quote_id: String,
    amount: u64,
    mint_node_id: String,
    mint_url: String,
    accepted: bool,
    token: String,
}

impl BitcreditEbillQuote {
    pub fn new_empty() -> Self {
        Self {
            bill_id: "".to_string(),
            quote_id: "".to_string(),
            amount: 0,
            mint_node_id: "".to_string(),
            mint_url: "".to_string(),
            accepted: false,
            token: "".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBillForList {
    name: String,
    to_payee: bool,
    bill_jurisdiction: String,
    timestamp_at_drawing: i64,
    drawee: IdentityPublicData,
    drawer: IdentityPublicData,
    payee: IdentityPublicData,
    endorsee: IdentityPublicData,
    place_of_drawing: String,
    currency_code: String,
    amount_numbers: u64,
    amounts_letters: String,
    maturity_date: String,
    date_of_issue: String,
    compounding_interest_rate: u64,
    type_of_interest_calculation: bool,
    place_of_payment: String,
    public_key: String,
    private_key: String,
    language: String,
    chain_of_blocks: ChainToReturn,
}

#[derive(BorshSerialize, BorshDeserialize, FromForm, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBill {
    name: String,
    to_payee: bool,
    bill_jurisdiction: String,
    timestamp_at_drawing: i64,
    // The party obliged to pay a Bill
    drawee: IdentityPublicData,
    // The party issuing a Bill
    drawer: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
    payee: IdentityPublicData,
    endorsee: IdentityPublicData,
    place_of_drawing: String,
    currency_code: String,
    //TODO: f64
    amount_numbers: u64,
    amounts_letters: String,
    maturity_date: String,
    date_of_issue: String,
    compounding_interest_rate: u64,
    type_of_interest_calculation: bool,
    // Defaulting to the drawee’s id/ address.
    place_of_payment: String,
    public_key: String,
    private_key: String,
    language: String,
}

impl BitcreditBill {
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            to_payee: false,
            bill_jurisdiction: "".to_string(),
            timestamp_at_drawing: 0,
            drawee: IdentityPublicData::new_empty(),
            drawer: IdentityPublicData::new_empty(),
            payee: IdentityPublicData::new_empty(),
            endorsee: IdentityPublicData::new_empty(),
            place_of_drawing: "".to_string(),
            currency_code: "".to_string(),
            amount_numbers: 0,
            amounts_letters: "".to_string(),
            maturity_date: "".to_string(),
            date_of_issue: "".to_string(),
            compounding_interest_rate: 0,
            type_of_interest_calculation: false,
            place_of_payment: "".to_string(),
            public_key: "".to_string(),
            private_key: "".to_string(),
            language: "".to_string(),
        }
    }
}

impl BitcreditBillToReturn {
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            to_payee: false,
            bill_jurisdiction: "".to_string(),
            timestamp_at_drawing: 0,
            drawee: IdentityPublicData::new_empty(),
            drawer: IdentityPublicData::new_empty(),
            payee: IdentityPublicData::new_empty(),
            endorsee: IdentityPublicData::new_empty(),
            place_of_drawing: "".to_string(),
            currency_code: "".to_string(),
            amount_numbers: 0,
            amounts_letters: "".to_string(),
            maturity_date: "".to_string(),
            date_of_issue: "".to_string(),
            compounding_interest_rate: 0,
            type_of_interest_calculation: false,
            place_of_payment: "".to_string(),
            public_key: "".to_string(),
            private_key: "".to_string(),
            language: "".to_string(),
            accepted: false,
            endorsed: false,
            requested_to_pay: false,
            requested_to_accept: false,
            payed: false,
            waited_for_payment: false,
            address_for_selling: String::new(),
            amount_for_selling: 0,
            buyer: IdentityPublicData::new_empty(),
            seller: IdentityPublicData::new_empty(),
            link_for_buy: "".to_string(),
            link_to_pay: "".to_string(),
            address_to_pay: "".to_string(),
            pr_key_bill: "".to_string(),
            number_of_confirmations: 0,
            pending: false,
            chain_of_blocks: ChainToReturn { blocks: vec![] },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BillKeys {
    private_key_pem: String,
    public_key_pem: String,
}

pub fn issue_new_bill(
    bill_jurisdiction: String,
    place_of_drawing: String,
    amount_numbers: u64,
    place_of_payment: String,
    maturity_date: String,
    currency_code: String,
    drawer: IdentityWithAll,
    language: String,
    public_data_drawee: IdentityPublicData,
    public_data_payee: IdentityPublicData,
    timestamp: i64,
) -> BitcreditBill {
    let s = bitcoin::secp256k1::Secp256k1::new();
    let private_key = bitcoin::PrivateKey::new(
        s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
            .0,
        USEDNET,
    );
    let public_key = private_key.public_key(&s);

    let bill_name: String = create_bill_name(&public_key);

    let private_key_bitcoin: String = private_key.to_string();
    let public_key_bitcoin: String = public_key.to_string();

    let rsa: Rsa<Private> = generation_rsa_key();
    let private_key_pem: String = pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = encode(&amount_numbers);

    let public_data_drawer =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    let utc = Utc::now();
    let date_of_issue = utc.naive_local().date().to_string();
    // let maturity_date = utc
    //     .checked_add_days(Days::new(BILL_VALIDITY_PERIOD))
    //     .unwrap()
    //     .naive_local()
    //     .date()
    //     .to_string();

    let new_bill = BitcreditBill {
        name: bill_name.clone(),
        to_payee: false,
        bill_jurisdiction,
        timestamp_at_drawing: timestamp,
        place_of_drawing,
        currency_code,
        amount_numbers,
        amounts_letters: amount_letters,
        maturity_date,
        date_of_issue,
        compounding_interest_rate: COMPOUNDING_INTEREST_RATE_ZERO,
        type_of_interest_calculation: false,
        place_of_payment,
        public_key: public_key_bitcoin,
        private_key: private_key_bitcoin,
        language,
        drawee: public_data_drawee,
        drawer: public_data_drawer,
        payee: public_data_payee,
        endorsee: IdentityPublicData::new_empty(),
    };

    let drawer_public_data =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    start_blockchain_for_new_bill(
        &new_bill,
        OperationCode::Issue,
        drawer_public_data,
        drawer.identity.public_key_pem.clone(),
        drawer.identity.private_key_pem.clone(),
        private_key_pem.clone(),
        timestamp,
    );

    new_bill
}

pub fn issue_new_bill_drawer_is_payee(
    bill_jurisdiction: String,
    place_of_drawing: String,
    amount_numbers: u64,
    place_of_payment: String,
    maturity_date: String,
    currency_code: String,
    drawer: IdentityWithAll,
    language: String,
    public_data_drawee: IdentityPublicData,
    timestamp: i64,
) -> BitcreditBill {
    let s = bitcoin::secp256k1::Secp256k1::new();
    let private_key = bitcoin::PrivateKey::new(
        s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
            .0,
        USEDNET,
    );
    let public_key = private_key.public_key(&s);

    let bill_name: String = create_bill_name(&public_key);

    let private_key_bitcoin: String = private_key.to_string();
    let public_key_bitcoin: String = public_key.to_string();

    let rsa: Rsa<Private> = generation_rsa_key();
    let private_key_pem: String = pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = encode(&amount_numbers);

    let public_data_payee =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    let utc = Utc::now();
    let date_of_issue = utc.naive_local().date().to_string();
    // let maturity_date = utc
    //     .checked_add_days(Days::new(BILL_VALIDITY_PERIOD))
    //     .unwrap()
    //     .naive_local()
    //     .date()
    //     .to_string();

    let new_bill = BitcreditBill {
        name: bill_name.clone(),
        to_payee: true,
        bill_jurisdiction,
        timestamp_at_drawing: timestamp,
        place_of_drawing,
        currency_code,
        amount_numbers,
        amounts_letters: amount_letters,
        maturity_date,
        date_of_issue,
        compounding_interest_rate: COMPOUNDING_INTEREST_RATE_ZERO,
        type_of_interest_calculation: false,
        place_of_payment,
        public_key: public_key_bitcoin,
        private_key: private_key_bitcoin,
        language,
        drawee: public_data_drawee,
        drawer: public_data_payee.clone(),
        payee: public_data_payee,
        endorsee: IdentityPublicData::new_empty(),
    };

    let drawer_public_data =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    start_blockchain_for_new_bill(
        &new_bill,
        OperationCode::Issue,
        drawer_public_data,
        drawer.identity.public_key_pem.clone(),
        drawer.identity.private_key_pem.clone(),
        private_key_pem.clone(),
        timestamp,
    );

    new_bill
}

pub fn issue_new_bill_drawer_is_drawee(
    bill_jurisdiction: String,
    place_of_drawing: String,
    amount_numbers: u64,
    place_of_payment: String,
    maturity_date: String,
    currency_code: String,
    drawer: IdentityWithAll,
    language: String,
    public_data_payee: IdentityPublicData,
    timestamp: i64,
) -> BitcreditBill {
    let s = bitcoin::secp256k1::Secp256k1::new();
    let private_key = bitcoin::PrivateKey::new(
        s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
            .0,
        USEDNET,
    );
    let public_key = private_key.public_key(&s);

    let bill_name: String = create_bill_name(&public_key);

    let private_key_bitcoin: String = private_key.to_string();
    let public_key_bitcoin: String = public_key.to_string();

    let rsa: Rsa<Private> = generation_rsa_key();
    let private_key_pem: String = pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = encode(&amount_numbers);

    let public_data_drawee =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    let utc = Utc::now();
    let date_of_issue = utc.naive_local().date().to_string();
    // let maturity_date = utc
    //     .checked_add_days(Days::new(BILL_VALIDITY_PERIOD))
    //     .unwrap()
    //     .naive_local()
    //     .date()
    //     .to_string();

    let new_bill = BitcreditBill {
        name: bill_name.clone(),
        to_payee: false,
        bill_jurisdiction,
        timestamp_at_drawing: timestamp,
        place_of_drawing,
        currency_code,
        amount_numbers,
        amounts_letters: amount_letters,
        maturity_date,
        date_of_issue,
        compounding_interest_rate: COMPOUNDING_INTEREST_RATE_ZERO,
        type_of_interest_calculation: false,
        place_of_payment,
        public_key: public_key_bitcoin,
        private_key: private_key_bitcoin,
        language,
        drawee: public_data_drawee.clone(),
        drawer: public_data_drawee,
        payee: public_data_payee,
        endorsee: IdentityPublicData::new_empty(),
    };

    let drawer_public_data =
        IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string().clone());

    start_blockchain_for_new_bill(
        &new_bill,
        OperationCode::Issue,
        drawer_public_data,
        drawer.identity.public_key_pem.clone(),
        drawer.identity.private_key_pem.clone(),
        private_key_pem.clone(),
        timestamp,
    );

    new_bill
}

fn write_bill_keys_to_file(bill_name: String, private_key: String, public_key: String) {
    let keys: BillKeys = BillKeys {
        private_key_pem: private_key,
        public_key_pem: public_key,
    };

    let output_path = BILLS_KEYS_FOLDER_PATH.to_string() + "/" + bill_name.as_str() + ".json";
    fs::write(
        output_path.clone(),
        serde_json::to_string_pretty(&keys).unwrap(),
    )
    .unwrap();
}

fn create_bill_name(public_key: &PublicKey) -> String {
    let bill_name_hash: Vec<u8> = sha256(&public_key.to_bytes()).to_vec();

    hex::encode(bill_name_hash)
}

pub fn get_bills() -> Vec<BitcreditBill> {
    let mut bills = Vec::new();
    let paths = fs::read_dir(BILLS_FOLDER_PATH).unwrap();
    for _path in paths {
        let dir = _path.unwrap();
        if is_not_hidden(&dir) {
            let file_name = dir
                .file_name()
                .to_str()
                .expect("File name error")
                .to_string();
            //TODO change
            let path_without_extension = Path::file_stem(Path::new(&file_name))
                .expect("File name error")
                .to_str()
                .expect("File name error")
                .to_string();
            let bill = read_bill_from_file(&path_without_extension);
            bills.push(bill);
        }
    }
    bills
}

pub fn get_bills_for_list() -> Vec<BitcreditBillToReturn> {
    let mut bills = Vec::new();
    let paths = fs::read_dir(BILLS_FOLDER_PATH).unwrap();
    for _path in paths {
        let dir = _path.unwrap();
        if is_not_hidden(&dir) {
            let file_name = dir
                .file_name()
                .to_str()
                .expect("File name error")
                .to_string();
            //TODO change
            let path_without_extension = Path::file_stem(Path::new(&file_name))
                .expect("File name error")
                .to_str()
                .expect("File name error")
                .to_string();
            let bill =
                thread::spawn(move || read_bill_with_chain_from_file(&path_without_extension))
                    .join()
                    .expect("Thread panicked");
            bills.push(bill);
        }
    }
    bills
}

pub fn endorse_bitcredit_bill(
    bill_name: &String,
    endorsee: IdentityPublicData,
    timestamp: i64,
) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();

    let exist_block_with_code_endorse =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Endorse);

    let exist_block_with_code_mint =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Mint);

    let exist_block_with_code_sell =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Sell);

    if (my_peer_id.eq(&bill.payee.peer_id)
        && !exist_block_with_code_endorse
        && !exist_block_with_code_sell
        && !exist_block_with_code_mint)
        || (my_peer_id.eq(&bill.endorsee.peer_id))
    {
        let identity = get_whole_identity();

        let my_identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());
        let endorsed_by = serde_json::to_vec(&my_identity_public).unwrap();

        let data_for_new_block_in_bytes = serde_json::to_vec(&endorsee).unwrap();
        let data_for_new_block = "Endorsed to ".to_string()
            + &hex::encode(data_for_new_block_in_bytes)
            + " endorsed by "
            + &hex::encode(endorsed_by);

        let keys = read_keys_from_bill_file(bill_name);
        let key: Rsa<Private> = Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

        let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
        let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.clone(),
            identity.identity.public_key_pem.clone(),
            OperationCode::Endorse,
            identity.identity.private_key_pem.clone(),
            timestamp,
        );

        let try_add_block = blockchain_from_file.try_add_block(new_block.clone());
        if try_add_block && blockchain_from_file.is_chain_valid() {
            blockchain_from_file.write_chain_to_file(&bill.name);
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub async fn mint_bitcredit_bill(
    bill_name: &String,
    mintnode: IdentityPublicData,
    timestamp: i64,
) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();

    let exist_block_with_code_endorse =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Endorse);

    let exist_block_with_code_mint =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Mint);

    let exist_block_with_code_sell =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Sell);

    if (my_peer_id.eq(&bill.payee.peer_id)
        && !exist_block_with_code_endorse
        && !exist_block_with_code_sell
        && !exist_block_with_code_mint)
        || (my_peer_id.eq(&bill.endorsee.peer_id))
    {
        let identity = get_whole_identity();

        let my_identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());
        let minted_by = serde_json::to_vec(&my_identity_public).unwrap();

        let data_for_new_block_in_bytes = serde_json::to_vec(&mintnode).unwrap();
        let data_for_new_block = "Endorsed to ".to_string()
            + &hex::encode(data_for_new_block_in_bytes)
            + " endorsed by "
            + &hex::encode(minted_by);

        let keys = read_keys_from_bill_file(bill_name);
        let key: Rsa<Private> = Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

        let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
        let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.clone(),
            identity.identity.public_key_pem.clone(),
            OperationCode::Mint,
            identity.identity.private_key_pem.clone(),
            timestamp,
        );

        let try_add_block = blockchain_from_file.try_add_block(new_block.clone());

        if try_add_block && blockchain_from_file.is_chain_valid() {
            let bill_id = bill.name.clone();

            blockchain_from_file.write_chain_to_file(&bill_id);
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub fn sell_bitcredit_bill(
    bill_name: &String,
    buyer: IdentityPublicData,
    timestamp: i64,
    amount_numbers: u64,
) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();

    let exist_block_with_code_endorse =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Endorse);

    let exist_block_with_code_sell =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Sell);

    if (my_peer_id.eq(&bill.payee.peer_id)
        && !exist_block_with_code_endorse
        && !exist_block_with_code_sell)
        || (my_peer_id.eq(&bill.endorsee.peer_id))
    {
        let identity = get_whole_identity();

        let my_identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());
        let seller = serde_json::to_vec(&my_identity_public).unwrap();

        let buyer_u8 = serde_json::to_vec(&buyer).unwrap();
        let data_for_new_block = "Sold to ".to_string()
            + &hex::encode(buyer_u8)
            + " sold by "
            + &hex::encode(seller)
            + " amount: "
            + &amount_numbers.to_string();

        let keys = read_keys_from_bill_file(bill_name);
        let key: Rsa<Private> = Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

        let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
        let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.clone(),
            identity.identity.public_key_pem.clone(),
            OperationCode::Sell,
            identity.identity.private_key_pem.clone(),
            timestamp,
        );

        let try_add_block = blockchain_from_file.try_add_block(new_block.clone());
        if try_add_block && blockchain_from_file.is_chain_valid() {
            blockchain_from_file.write_chain_to_file(&bill.name);
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub fn request_pay(bill_name: &String, timestamp: i64) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();

    let exist_block_with_code_endorse =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Endorse);

    let exist_block_with_code_mint =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Mint);

    let exist_block_with_code_sell =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Sell);

    if (my_peer_id.eq(&bill.payee.peer_id)
        && !exist_block_with_code_endorse
        && !exist_block_with_code_sell
        && !exist_block_with_code_mint)
        || (my_peer_id.eq(&bill.endorsee.peer_id))
    {
        let identity = get_whole_identity();

        let my_identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());

        let data_for_new_block_in_bytes = serde_json::to_vec(&my_identity_public).unwrap();
        let data_for_new_block =
            "Requested to pay by ".to_string() + &hex::encode(data_for_new_block_in_bytes);

        let keys = read_keys_from_bill_file(bill_name);
        let key: Rsa<Private> = Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

        let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
        let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.clone(),
            identity.identity.public_key_pem.clone(),
            OperationCode::RequestToPay,
            identity.identity.private_key_pem.clone(),
            timestamp,
        );

        let try_add_block = blockchain_from_file.try_add_block(new_block.clone());
        if try_add_block && blockchain_from_file.is_chain_valid() {
            blockchain_from_file.write_chain_to_file(&bill.name);
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub fn request_acceptance(bill_name: &String, timestamp: i64) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();

    let exist_block_with_code_endorse =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Endorse);

    let exist_block_with_code_sell =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Sell);

    let exist_block_with_code_mint =
        blockchain_from_file.exist_block_with_operation_code(OperationCode::Mint);

    if (my_peer_id.eq(&bill.payee.peer_id)
        && !exist_block_with_code_endorse
        && !exist_block_with_code_sell
        && !exist_block_with_code_mint)
        || (my_peer_id.eq(&bill.endorsee.peer_id))
    {
        let identity = get_whole_identity();

        let my_identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());

        let data_for_new_block_in_bytes = serde_json::to_vec(&my_identity_public).unwrap();
        let data_for_new_block =
            "Requested to accept by ".to_string() + &hex::encode(data_for_new_block_in_bytes);

        let keys = read_keys_from_bill_file(bill_name);
        let key: Rsa<Private> = Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

        let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
        let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.clone(),
            identity.identity.public_key_pem.clone(),
            OperationCode::RequestToAccept,
            identity.identity.private_key_pem.clone(),
            timestamp,
        );

        let try_add_block = blockchain_from_file.try_add_block(new_block.clone());
        if try_add_block && blockchain_from_file.is_chain_valid() {
            blockchain_from_file.write_chain_to_file(&bill.name);
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub fn accept_bill(bill_name: &String, timestamp: i64) -> bool {
    let my_peer_id = read_peer_id_from_file().to_string();
    let bill = read_bill_from_file(bill_name);

    let mut blockchain_from_file = Chain::read_chain_from_file(bill_name);
    let last_block = blockchain_from_file.get_latest_block();
    let accepted = blockchain_from_file.exist_block_with_operation_code(OperationCode::Accept);

    if bill.drawee.peer_id.eq(&my_peer_id) {
        if !accepted {
            let identity = get_whole_identity();

            let my_identity_public =
                IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());

            let data_for_new_block_in_bytes = serde_json::to_vec(&my_identity_public).unwrap();
            let data_for_new_block =
                "Accepted by ".to_string() + &hex::encode(data_for_new_block_in_bytes);

            let keys = read_keys_from_bill_file(bill_name);
            let key: Rsa<Private> =
                Rsa::private_key_from_pem(keys.private_key_pem.as_bytes()).unwrap();

            let data_for_new_block_in_bytes = data_for_new_block.as_bytes().to_vec();
            let data_for_new_block_encrypted = encrypt_bytes(&data_for_new_block_in_bytes, &key);
            let data_for_new_block_encrypted_in_string_format =
                hex::encode(data_for_new_block_encrypted);

            let new_block = Block::new(
                last_block.id + 1,
                last_block.hash.clone(),
                data_for_new_block_encrypted_in_string_format,
                bill_name.clone(),
                identity.identity.public_key_pem.clone(),
                OperationCode::Accept,
                identity.identity.private_key_pem.clone(),
                timestamp,
            );

            let try_add_block = blockchain_from_file.try_add_block(new_block.clone());
            if try_add_block && blockchain_from_file.is_chain_valid() {
                blockchain_from_file.write_chain_to_file(&bill.name);
                true
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    }
}

#[tokio::main]
async fn read_bill_with_chain_from_file(id: &String) -> BitcreditBillToReturn {
    let bill: BitcreditBill = read_bill_from_file(id);
    let chain = Chain::read_chain_from_file(&bill.name);
    let drawer = chain.get_drawer();
    let chain_to_return = ChainToReturn::new(chain.clone());
    let endorsed = chain.exist_block_with_operation_code(OperationCode::Endorse);
    let accepted = chain.exist_block_with_operation_code(OperationCode::Accept);
    let requested_to_pay = chain.exist_block_with_operation_code(OperationCode::RequestToPay);
    let requested_to_accept = chain.exist_block_with_operation_code(OperationCode::RequestToAccept);
    let address_to_pay = web::get_address_to_pay(bill.clone());
    let check_if_already_paid =
        web::check_if_paid(address_to_pay.clone(), bill.amount_numbers).await;
    let payed = check_if_already_paid.0;

    BitcreditBillToReturn {
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
        waited_for_payment: false,
        address_for_selling: "".to_string(),
        amount_for_selling: 0,
        buyer: IdentityPublicData::new_empty(),
        seller: IdentityPublicData::new_empty(),
        requested_to_pay,
        requested_to_accept,
        payed,
        link_to_pay: "".to_string(),
        link_for_buy: "".to_string(),
        pr_key_bill: "".to_string(),
        number_of_confirmations: 0,
        pending: false,
        address_to_pay,
        chain_of_blocks: chain_to_return,
    }
}

fn read_bill_from_file(bill_name: &String) -> BitcreditBill {
    let chain = Chain::read_chain_from_file(bill_name);
    chain.get_last_version_bill()
}

fn bill_to_byte_array(bill: &BitcreditBill) -> Vec<u8> {
    bill.try_to_vec().unwrap()
}

fn bill_from_byte_array(bill: &[u8]) -> BitcreditBill {
    BitcreditBill::try_from_slice(bill).unwrap()
}

fn read_keys_from_bill_file(bill_name: &str) -> BillKeys {
    let input_path = BILLS_KEYS_FOLDER_PATH.to_string() + "/" + bill_name + ".json";
    let blockchain_from_file = fs::read(input_path.clone()).expect("file not found");
    serde_json::from_slice(blockchain_from_file.as_slice()).unwrap()
}
//--------------------------------------------------------------

//-------------------------Forms--------------------------------
#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBillForm {
    pub bill_jurisdiction: String,
    pub place_of_drawing: String,
    pub currency_code: String,
    pub amount_numbers: u64,
    pub language: String,
    pub drawee_name: String,
    pub payee_name: String,
    pub place_of_payment: String,
    pub maturity_date: String,
    pub drawer_is_payee: bool,
    pub drawer_is_drawee: bool,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct EndorseBitcreditBillForm {
    pub endorsee: String,
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct MintBitcreditBillForm {
    pub mint_node: String,
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct AcceptMintBitcreditBillForm {
    pub interest: u64,
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct RequestToMintBitcreditBillForm {
    pub mint_node: String,
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct SellBitcreditBillForm {
    pub buyer: String,
    pub bill_name: String,
    pub amount_numbers: u64,
    pub currency_code: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct RequestToAcceptBitcreditBillForm {
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct RequestToPayBitcreditBillForm {
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct AcceptBitcreditBillForm {
    pub bill_name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct IdentityForm {
    name: String,
    company: String,
    date_of_birth: String,
    city_of_birth: String,
    country_of_birth: String,
    email: String,
    postal_address: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct NewContactForm {
    pub name: String,
    pub node_id: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct EditContactForm {
    pub old_name: String,
    pub name: String,
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct DeleteContactForm {
    pub name: String,
}
//-------------------------------------------------------------
