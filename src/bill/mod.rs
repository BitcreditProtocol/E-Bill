use crate::blockchain::{
    start_blockchain_for_new_bill, Block, Chain, ChainToReturn, OperationCode,
};
use crate::constants::{
    BILLS_FOLDER_PATH, BILLS_KEYS_FOLDER_PATH, COMPOUNDING_INTEREST_RATE_ZERO, USEDNET,
};
use crate::external;
use crate::util::{self, numbers_to_words};
use bitcoin::PublicKey;
use borsh::BorshDeserialize;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use chrono::Utc;
use contacts::IdentityPublicData;
use identity::{get_whole_identity, read_peer_id_from_file, IdentityWithAll};
use openssl::pkey::Private;
use openssl::rsa::Rsa;
use openssl::sha::sha256;
use rocket::serde::{Deserialize, Serialize};
use rocket::FromForm;
use std::path::Path;
use std::{fs, thread};

pub mod contacts;
pub mod identity;
pub mod quotes;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBillToReturn {
    pub name: String,
    pub to_payee: bool,
    pub bill_jurisdiction: String,
    pub timestamp_at_drawing: i64,
    pub drawee: IdentityPublicData,
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    pub endorsee: IdentityPublicData,
    pub place_of_drawing: String,
    pub currency_code: String,
    pub amount_numbers: u64,
    pub amounts_letters: String,
    pub maturity_date: String,
    pub date_of_issue: String,
    pub compounding_interest_rate: u64,
    pub type_of_interest_calculation: bool,
    pub place_of_payment: String,
    pub public_key: String,
    pub private_key: String,
    pub language: String,
    pub accepted: bool,
    pub endorsed: bool,
    pub requested_to_pay: bool,
    pub requested_to_accept: bool,
    pub payed: bool,
    pub waited_for_payment: bool,
    pub address_for_selling: String,
    pub amount_for_selling: u64,
    pub buyer: IdentityPublicData,
    pub seller: IdentityPublicData,
    pub link_for_buy: String,
    pub link_to_pay: String,
    pub pr_key_bill: String,
    pub number_of_confirmations: u64,
    pub pending: bool,
    pub address_to_pay: String,
    pub chain_of_blocks: ChainToReturn,
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditEbillQuote {
    pub bill_id: String,
    pub quote_id: String,
    pub amount: u64,
    pub mint_node_id: String,
    pub mint_url: String,
    pub accepted: bool,
    pub token: String,
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
    pub name: String,
    pub to_payee: bool,
    pub bill_jurisdiction: String,
    pub timestamp_at_drawing: i64,
    // The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    // The party issuing a Bill
    pub drawer: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
    pub payee: IdentityPublicData,
    pub endorsee: IdentityPublicData,
    pub place_of_drawing: String,
    pub currency_code: String,
    //TODO: f64
    pub amount_numbers: u64,
    pub amounts_letters: String,
    pub maturity_date: String,
    pub date_of_issue: String,
    pub compounding_interest_rate: u64,
    pub type_of_interest_calculation: bool,
    // Defaulting to the drawee’s id/ address.
    pub place_of_payment: String,
    pub public_key: String,
    pub private_key: String,
    pub language: String,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BillKeys {
    pub private_key_pem: String,
    pub public_key_pem: String,
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

    let rsa: Rsa<Private> = util::rsa::generation_rsa_key();
    let private_key_pem: String = util::rsa::pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = util::rsa::pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = numbers_to_words::encode(&amount_numbers);

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

    let rsa: Rsa<Private> = util::rsa::generation_rsa_key();
    let private_key_pem: String = util::rsa::pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = util::rsa::pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = numbers_to_words::encode(&amount_numbers);

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

    let rsa: Rsa<Private> = util::rsa::generation_rsa_key();
    let private_key_pem: String = util::rsa::pem_private_key_from_rsa(&rsa);
    let public_key_pem: String = util::rsa::pem_public_key_from_rsa(&rsa);
    write_bill_keys_to_file(
        bill_name.clone(),
        private_key_pem.clone(),
        public_key_pem.clone(),
    );

    let amount_letters: String = numbers_to_words::encode(&amount_numbers);

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
        if util::is_not_hidden(&dir) {
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
        if util::is_not_hidden(&dir) {
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
    bill_name: &str,
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
        let data_for_new_block_encrypted =
            util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
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
    bill_name: &str,
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
        let data_for_new_block_encrypted =
            util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
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
    bill_name: &str,
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
        let data_for_new_block_encrypted =
            util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
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

pub fn request_pay(bill_name: &str, timestamp: i64) -> bool {
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
        let data_for_new_block_encrypted =
            util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
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

pub fn request_acceptance(bill_name: &str, timestamp: i64) -> bool {
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
        let data_for_new_block_encrypted =
            util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
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

pub fn accept_bill(bill_name: &str, timestamp: i64) -> bool {
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
            let data_for_new_block_encrypted =
                util::rsa::encrypt_bytes(&data_for_new_block_in_bytes, &key);
            let data_for_new_block_encrypted_in_string_format =
                hex::encode(data_for_new_block_encrypted);

            let new_block = Block::new(
                last_block.id + 1,
                last_block.hash.clone(),
                data_for_new_block_encrypted_in_string_format,
                bill_name.to_owned(),
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
async fn read_bill_with_chain_from_file(id: &str) -> BitcreditBillToReturn {
    let bill: BitcreditBill = read_bill_from_file(id);
    let chain = Chain::read_chain_from_file(&bill.name);
    let drawer = chain.get_drawer();
    let chain_to_return = ChainToReturn::new(chain.clone());
    let endorsed = chain.exist_block_with_operation_code(OperationCode::Endorse);
    let accepted = chain.exist_block_with_operation_code(OperationCode::Accept);
    let requested_to_pay = chain.exist_block_with_operation_code(OperationCode::RequestToPay);
    let requested_to_accept = chain.exist_block_with_operation_code(OperationCode::RequestToAccept);
    let address_to_pay = external::bitcoin::get_address_to_pay(bill.clone());
    let check_if_already_paid =
        external::bitcoin::check_if_paid(address_to_pay.clone(), bill.amount_numbers).await;
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

pub fn read_bill_from_file(bill_name: &str) -> BitcreditBill {
    let chain = Chain::read_chain_from_file(bill_name);
    chain.get_last_version_bill()
}

pub fn bill_from_byte_array(bill: &[u8]) -> BitcreditBill {
    BitcreditBill::try_from_slice(bill).unwrap()
}

pub fn read_keys_from_bill_file(bill_name: &str) -> BillKeys {
    let input_path = BILLS_KEYS_FOLDER_PATH.to_string() + "/" + bill_name + ".json";
    let blockchain_from_file = fs::read(input_path.clone()).expect("file not found");
    serde_json::from_slice(blockchain_from_file.as_slice()).unwrap()
}
