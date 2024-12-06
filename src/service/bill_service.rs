use super::contact_service::IdentityPublicData;
use super::identity_service::IdentityWithAll;
use crate::blockchain::{
    self, start_blockchain_for_new_bill, Block, Chain, ChainToReturn, OperationCode,
    WaitingForPayment,
};
use crate::constants::{
    ACCEPTED_BY, AMOUNT, COMPOUNDING_INTEREST_RATE_ZERO, ENDORSED_BY, ENDORSED_TO,
    REQ_TO_ACCEPT_BY, REQ_TO_PAY_BY, SOLD_BY, SOLD_TO,
};
use crate::external::bitcoin::BitcoinClientApi;
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::IdentityStoreApi;
use crate::util::get_current_payee_private_key;
use crate::web::data::File;
use crate::CONFIG;
use crate::{dht, external, persistence, util};
use crate::{
    dht::{Client, GossipsubEvent, GossipsubEventId},
    persistence::bill::BillStoreApi,
};
use async_trait::async_trait;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use chrono::Utc;
use log::{error, info};
use rocket::serde::{Deserialize, Serialize};
use rocket::{http::Status, response::Responder};
use std::sync::Arc;
use thiserror::Error;

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// error returned if a bill was already accepted and is attempted to be accepted again
    #[error("Bill was already accepted")]
    BillAlreadyAccepted,

    /// error returned if the caller of an operation is not the drawee, but would have to be for it
    /// to be valid, e.g. accepting a  bill
    #[error("Caller is not drawee")]
    CallerIsNotDrawee,

    /// error returned if the caller of an operation is not the payee, or endorsee, but would have to be for it
    /// to be valid, e.g. requesting payment
    #[error("Caller is not payee, or endorsee")]
    CallerIsNotPayeeOrEndorsee,

    /// errors stemming from json deserialization
    #[error("unable to serialize/deserialize to/from JSON {0}")]
    Json(#[from] serde_json::Error),

    /// errors that stem from interacting with a blockchain
    #[error("Blockchain error: {0}")]
    Blockchain(#[from] blockchain::Error),

    /// errors that stem from interacting with the Dht
    #[error("Dht error: {0}")]
    Dht(#[from] dht::Error),

    /// all errors originating from the persistence layer
    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),

    /// all errors originating from external APIs
    #[error("External API error: {0}")]
    ExternalApi(#[from] external::Error),

    /// errors stemming from cryptography, such as converting keys
    #[error("Cryptography error: {0}")]
    Cryptography(String),
}

impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            Error::BillAlreadyAccepted => Status::BadRequest.respond_to(req),
            Error::CallerIsNotDrawee => Status::BadRequest.respond_to(req),
            Error::CallerIsNotPayeeOrEndorsee => Status::BadRequest.respond_to(req),
            Error::Json(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Persistence(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::ExternalApi(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Blockchain(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Dht(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::Cryptography(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
        }
    }
}

#[async_trait]
pub trait BillServiceApi: Send + Sync {
    /// Gets all bills
    async fn get_bills(&self) -> Result<Vec<BitcreditBillToReturn>>;

    /// Gets the full bill for the given bill name
    async fn get_full_bill(
        &self,
        bill_name: &str,
        current_timestamp: i64,
    ) -> Result<BitcreditBillToReturn>;

    /// Gets the bill for the given bill name
    async fn get_bill(&self, bill_name: &str) -> Result<BitcreditBill>;

    /// Gets the blockchain for the given bill name
    async fn get_blockchain_for_bill(&self, bill_name: &str) -> Result<Chain>;

    /// Try to get the given bill from the dht and saves it locally, if found
    async fn find_bill_in_dht(&self, bill_name: &str) -> Result<()>;

    /// Gets the keys for a given bill
    async fn get_bill_keys(&self, bill_name: &str) -> Result<BillKeys>;

    /// opens and decrypts the attached file from the given bill
    async fn open_and_decrypt_attached_file(
        &self,
        bill_name: &str,
        file_name: &str,
        bill_private_key: &str,
    ) -> Result<Vec<u8>>;

    /// encrypts and saves the given uploaded file, returning the file name, as well as the hash of
    /// the unencrypted file
    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        bill_name: &str,
        bill_public_key: &str,
    ) -> Result<File>;

    /// issues a new bill
    async fn issue_new_bill(
        &self,
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
        file_upload_id: Option<String>,
        timestamp: i64,
    ) -> Result<BitcreditBill>;

    /// propagates the given bill to the DHT
    async fn propagate_bill(
        &self,
        bill_name: &str,
        drawer_peer_id: &str,
        drawee_peer_id: &str,
        payee_peer_id: &str,
    ) -> Result<()>;

    /// propagates the given block to the DHT
    async fn propagate_block(&self, bill_name: &str, block: &Block) -> Result<()>;

    /// adds the given bill for the given node on the DHT
    async fn propagate_bill_for_node(&self, bill_name: &str, node_id: &str) -> Result<()>;

    /// accepts the given bill
    async fn accept_bill(&self, bill_name: &str, timestamp: i64) -> Result<Chain>;

    /// request pay for a bill
    async fn request_pay(&self, bill_name: &str, timestamp: i64) -> Result<Chain>;

    /// request acceptance for a bill
    async fn request_acceptance(&self, bill_name: &str, timestamp: i64) -> Result<Chain>;

    /// mint bitcredit bill
    async fn mint_bitcredit_bill(
        &self,
        bill_name: &str,
        mintnode: IdentityPublicData,
        timestamp: i64,
    ) -> Result<Chain>;

    /// sell bitcredit bill
    async fn sell_bitcredit_bill(
        &self,
        bill_name: &str,
        buyer: IdentityPublicData,
        timestamp: i64,
        amount_numbers: u64,
    ) -> Result<Chain>;

    /// endorse bitcredit bill
    async fn endorse_bitcredit_bill(
        &self,
        bill_name: &str,
        endorsee: IdentityPublicData,
        timestamp: i64,
    ) -> Result<Chain>;
}

/// The bill service is responsible for all bill-related logic and for syncing them with the dht data.
#[derive(Clone)]
pub struct BillService {
    client: Client,
    store: Arc<dyn BillStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    bitcoin_client: Arc<dyn BitcoinClientApi>,
}

impl BillService {
    pub fn new(
        client: Client,
        store: Arc<dyn BillStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        bitcoin_client: Arc<dyn BitcoinClientApi>,
    ) -> Self {
        Self {
            client,
            store,
            identity_store,
            file_upload_store,
            bitcoin_client,
        }
    }

    fn get_data_for_new_block(
        &self,
        identity: &IdentityWithAll,
        prefix: &str,
        other_party: Option<(&IdentityPublicData, &str)>,
        postfix: &str,
    ) -> Result<String> {
        let identity_public =
            IdentityPublicData::new(identity.identity.clone(), identity.peer_id.to_string());
        let caller_identity_bytes = serde_json::to_vec(&identity_public)?;
        match other_party {
            None => Ok(format!(
                "{prefix}{}{postfix}",
                &hex::encode(caller_identity_bytes)
            )),
            Some((other_identity, midfix)) => {
                let other_identity_bytes = serde_json::to_vec(&other_identity)?;
                Ok(format!(
                    "{prefix}{}{midfix}{}{postfix}",
                    &hex::encode(other_identity_bytes),
                    &hex::encode(caller_identity_bytes)
                ))
            }
        }
    }

    /// attempts to add a block for the given operation and with the given prefix to the chain,
    /// mutating the chain
    async fn add_block_for_operation(
        &self,
        bill_name: &str,
        blockchain: &mut Chain,
        timestamp: i64,
        operation_code: OperationCode,
        identity: IdentityWithAll,
        data_for_new_block: String,
    ) -> Result<()> {
        let last_block = blockchain.get_latest_block();

        let keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let data_for_new_block_in_bytes = data_for_new_block.as_bytes();
        let data_for_new_block_encrypted = util::rsa::encrypt_bytes_with_public_key(
            data_for_new_block_in_bytes,
            &keys.public_key_pem,
        );
        let data_for_new_block_encrypted_in_string_format =
            hex::encode(data_for_new_block_encrypted);

        let new_block = Block::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            bill_name.to_owned(),
            identity.identity.public_key_pem,
            operation_code,
            identity.identity.private_key_pem,
            timestamp,
        )?;

        let try_add_block = blockchain.try_add_block(new_block);
        if try_add_block && blockchain.is_chain_valid() {
            self.store
                .write_blockchain_to_file(bill_name, blockchain.to_pretty_printed_json()?)
                .await?;
            Ok(())
        } else {
            Err(Error::Blockchain(blockchain::Error::BlockchainInvalid))
        }
    }
}

#[async_trait]
impl BillServiceApi for BillService {
    async fn get_bills(&self) -> Result<Vec<BitcreditBillToReturn>> {
        let mut res = vec![];
        let bills = self.store.get_bills().await?;

        for bill in bills.into_iter() {
            let chain = self.store.read_bill_chain_from_file(&bill.name).await?;
            let bill_keys = self.store.read_bill_keys_from_file(&bill.name).await?;
            let drawer = chain.get_drawer(&bill_keys)?;
            let chain_to_return = ChainToReturn::new(chain.clone(), &bill_keys)?;
            let endorsed = chain.exist_block_with_operation_code(OperationCode::Endorse);
            let accepted = chain.exist_block_with_operation_code(OperationCode::Accept);
            let requested_to_pay =
                chain.exist_block_with_operation_code(OperationCode::RequestToPay);
            let requested_to_accept =
                chain.exist_block_with_operation_code(OperationCode::RequestToAccept);

            let holder_public_key = if !bill.endorsee.name.is_empty() {
                &bill.endorsee.bitcoin_public_key
            } else {
                &bill.payee.bitcoin_public_key
            };
            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill.public_key, holder_public_key)?;
            let mut paid = false;
            if chain.exist_block_with_operation_code(OperationCode::RequestToPay) {
                let check_if_already_paid = self
                    .bitcoin_client
                    .check_if_paid(&address_to_pay, bill.amount_numbers)
                    .await?;
                paid = check_if_already_paid.0;
            }

            res.push(BitcreditBillToReturn {
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
                paid,
                link_to_pay: "".to_string(),
                link_for_buy: "".to_string(),
                pr_key_bill: "".to_string(),
                number_of_confirmations: 0,
                pending: false,
                address_to_pay,
                chain_of_blocks: chain_to_return,
            });
        }

        Ok(res)
    }

    async fn get_full_bill(
        &self,
        bill_name: &str,
        current_timestamp: i64,
    ) -> Result<BitcreditBillToReturn> {
        let identity = self.identity_store.get_full().await?;
        let chain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = chain.get_last_version_bill(&bill_keys)?;

        let drawer = chain.get_drawer(&bill_keys)?;
        let mut link_for_buy = "".to_string();
        let chain_to_return = ChainToReturn::new(chain.clone(), &bill_keys)?;
        let endorsed = chain.exist_block_with_operation_code(OperationCode::Endorse);
        let accepted = chain.exist_block_with_operation_code(OperationCode::Accept);
        let address_for_selling: String = String::new();
        let amount_for_selling = 0;
        let waiting_for_payment =
            chain.is_last_sell_block_waiting_for_payment(&bill_keys, current_timestamp)?;
        let mut waited_for_payment = false;
        let mut buyer = IdentityPublicData::new_empty();
        let mut seller = IdentityPublicData::new_empty();
        if let WaitingForPayment::Yes(payment_info) = waiting_for_payment {
            buyer = payment_info.buyer;
            seller = payment_info.seller;
            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill.public_key, &seller.bitcoin_public_key)?;
            waited_for_payment = self
                .bitcoin_client
                .check_if_paid(&address_to_pay, payment_info.amount)
                .await?
                .0;

            if waited_for_payment
                && (identity.peer_id.to_string().eq(&buyer.peer_id)
                    || identity.peer_id.to_string().eq(&seller.peer_id))
            {
                let message: String = format!("Payment in relation to a bill {}", &bill.name);
                link_for_buy = self.bitcoin_client.generate_link_to_pay(
                    &address_to_pay,
                    payment_info.amount,
                    &message,
                );
            }
        }
        let requested_to_pay = chain.exist_block_with_operation_code(OperationCode::RequestToPay);
        let requested_to_accept =
            chain.exist_block_with_operation_code(OperationCode::RequestToAccept);
        let holder_public_key = if !bill.endorsee.name.is_empty() {
            &bill.endorsee.bitcoin_public_key
        } else {
            &bill.payee.bitcoin_public_key
        };
        let address_to_pay = self
            .bitcoin_client
            .get_address_to_pay(&bill.public_key, holder_public_key)?;
        let mut check_if_already_paid = (false, 0u64);
        if requested_to_pay {
            check_if_already_paid = self
                .bitcoin_client
                .check_if_paid(&address_to_pay, bill.amount_numbers)
                .await?;
        }
        let paid = check_if_already_paid.0;
        let mut number_of_confirmations: u64 = 0;
        let mut pending = false;
        if paid && check_if_already_paid.1.eq(&0) {
            pending = true;
        } else if paid && !check_if_already_paid.1.eq(&0) {
            let transaction = self
                .bitcoin_client
                .get_transactions(&address_to_pay)
                .await?;
            if let Some(txid) = self.bitcoin_client.get_first_transaction(&transaction) {
                let height = self.bitcoin_client.get_last_block_height().await?;
                number_of_confirmations = height - txid.status.block_height + 1;
            }
        }
        let message: String = format!("Payment in relation to a bill {}", bill.name.clone());
        let link_to_pay = self.bitcoin_client.generate_link_to_pay(
            &address_to_pay,
            bill.amount_numbers,
            &message,
        );
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

        Ok(BitcreditBillToReturn {
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
            paid,
            link_for_buy,
            link_to_pay,
            address_to_pay,
            pr_key_bill,
            number_of_confirmations,
            pending,
            chain_of_blocks: chain_to_return,
        })
    }

    async fn get_bill(&self, bill_name: &str) -> Result<BitcreditBill> {
        let chain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = chain.get_last_version_bill(&bill_keys)?;
        Ok(bill)
    }

    async fn get_blockchain_for_bill(&self, bill_name: &str) -> Result<Chain> {
        let chain = self.store.read_bill_chain_from_file(bill_name).await?;
        Ok(chain)
    }

    async fn find_bill_in_dht(&self, bill_name: &str) -> Result<()> {
        self.client.clone().get_bill(bill_name).await?;
        Ok(())
    }

    async fn get_bill_keys(&self, bill_name: &str) -> Result<BillKeys> {
        let keys = self.store.read_bill_keys_from_file(bill_name).await?;
        Ok(keys)
    }

    async fn open_and_decrypt_attached_file(
        &self,
        bill_name: &str,
        file_name: &str,
        bill_private_key: &str,
    ) -> Result<Vec<u8>> {
        let read_file = self.store.open_attached_file(bill_name, file_name).await?;
        let decrypted = util::rsa::decrypt_bytes_with_private_key(&read_file, bill_private_key);
        Ok(decrypted)
    }

    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        bill_name: &str,
        bill_public_key: &str,
    ) -> Result<File> {
        let file_hash = util::sha256_hash(file_bytes);
        let encrypted = util::rsa::encrypt_bytes_with_public_key(file_bytes, bill_public_key);
        self.store
            .save_attached_file(&encrypted, bill_name, file_name)
            .await?;
        info!("Saved file {file_name} with hash {file_hash} for bill {bill_name}");
        Ok(File {
            name: file_name.to_owned(),
            hash: file_hash,
        })
    }

    async fn issue_new_bill(
        &self,
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
        file_upload_id: Option<String>,
        timestamp: i64,
    ) -> Result<BitcreditBill> {
        let (private_key, public_key) = util::create_bitcoin_keypair(CONFIG.bitcoin_network());

        let bill_name = util::sha256_hash(&public_key.to_bytes());

        let private_key_bitcoin: String = private_key.to_string();
        let public_key_bitcoin: String = public_key.to_string();

        let (private_key_pem, public_key_pem) =
            util::rsa::create_rsa_key_pair().map_err(|e| Error::Cryptography(e.to_string()))?;

        self.store
            .write_bill_keys_to_file(
                bill_name.clone(),
                private_key_pem.clone(),
                public_key_pem.clone(),
            )
            .await?;

        let amount_letters: String = util::numbers_to_words::encode(&amount_numbers);

        let public_data_drawer =
            IdentityPublicData::new(drawer.identity.clone(), drawer.peer_id.to_string());

        let utc = Utc::now();
        let date_of_issue = utc.naive_local().date().to_string();

        let to_payee = public_data_drawer == public_data_payee;

        let mut bill_files: Vec<File> = vec![];
        if let Some(ref upload_id) = file_upload_id {
            let files = self
                .file_upload_store
                .read_temp_upload_files(upload_id)
                .await?;
            for (file_name, file_bytes) in files {
                bill_files.push(
                    self.encrypt_and_save_uploaded_file(
                        &file_name,
                        &file_bytes,
                        &bill_name,
                        &public_key_pem,
                    )
                    .await?,
                );
            }
        }

        let bill = BitcreditBill {
            name: bill_name,
            to_payee,
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
            drawer: public_data_drawer.clone(),
            payee: public_data_payee,
            endorsee: IdentityPublicData::new_empty(),
            files: bill_files,
        };

        let chain = start_blockchain_for_new_bill(
            &bill,
            OperationCode::Issue,
            public_data_drawer,
            drawer.identity.public_key_pem,
            drawer.identity.private_key_pem,
            public_key_pem,
            timestamp,
        )?;
        let json_chain = serde_json::to_string_pretty(&chain)?;
        self.store
            .write_blockchain_to_file(&bill.name, json_chain)
            .await?;

        // clean up temporary file uploads, if there are any, logging any errors
        if let Some(ref upload_id) = file_upload_id {
            if let Err(e) = self
                .file_upload_store
                .remove_temp_upload_folder(upload_id)
                .await
            {
                error!("Error while cleaning up temporary file uploads for {upload_id}: {e}");
            }
        }

        Ok(bill)
    }

    async fn propagate_block(&self, bill_name: &str, block: &Block) -> Result<()> {
        let block_bytes = serde_json::to_vec(block)?;
        let event = GossipsubEvent::new(GossipsubEventId::Block, block_bytes);
        let message = event.to_byte_array()?;

        self.client
            .clone()
            .add_message_to_bill_topic(message, bill_name)
            .await?;
        Ok(())
    }

    async fn propagate_bill_for_node(&self, bill_name: &str, node_id: &str) -> Result<()> {
        self.client
            .clone()
            .add_bill_to_dht_for_node(bill_name, node_id)
            .await?;
        Ok(())
    }

    async fn propagate_bill(
        &self,
        bill_name: &str,
        drawer_peer_id: &str,
        drawee_peer_id: &str,
        payee_peer_id: &str,
    ) -> Result<()> {
        let mut client = self.client.clone();

        for node in [drawer_peer_id, drawee_peer_id, payee_peer_id] {
            if !node.is_empty() {
                info!("issue bill: add {} for node {}", bill_name, &node);
                client.add_bill_to_dht_for_node(bill_name, node).await?;
            }
        }

        client.subscribe_to_bill_topic(bill_name).await?;
        client.start_providing_bill(bill_name).await?;
        Ok(())
    }

    async fn accept_bill(&self, bill_name: &str, timestamp: i64) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;

        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        let accepted = blockchain.exist_block_with_operation_code(OperationCode::Accept);

        if accepted {
            return Err(Error::BillAlreadyAccepted);
        }

        if !bill.drawee.peer_id.eq(&my_peer_id) {
            return Err(Error::CallerIsNotDrawee);
        }

        let identity = self.identity_store.get_full().await?;
        let data_for_new_block = self.get_data_for_new_block(&identity, ACCEPTED_BY, None, "")?;
        self.add_block_for_operation(
            bill_name,
            &mut blockchain,
            timestamp,
            OperationCode::Accept,
            identity,
            data_for_new_block,
        )
        .await?;
        Ok(blockchain)
    }

    async fn request_pay(&self, bill_name: &str, timestamp: i64) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_peer_id.eq(&bill.payee.peer_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (my_peer_id.eq(&bill.endorsee.peer_id))
        {
            let identity = self.identity_store.get_full().await?;
            let data_for_new_block =
                self.get_data_for_new_block(&identity, REQ_TO_PAY_BY, None, "")?;
            self.add_block_for_operation(
                bill_name,
                &mut blockchain,
                timestamp,
                OperationCode::RequestToPay,
                identity,
                data_for_new_block,
            )
            .await?;
            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn request_acceptance(&self, bill_name: &str, timestamp: i64) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_peer_id.eq(&bill.payee.peer_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (my_peer_id.eq(&bill.endorsee.peer_id))
        {
            let identity = self.identity_store.get_full().await?;
            let data_for_new_block =
                self.get_data_for_new_block(&identity, REQ_TO_ACCEPT_BY, None, "")?;
            self.add_block_for_operation(
                bill_name,
                &mut blockchain,
                timestamp,
                OperationCode::RequestToAccept,
                identity,
                data_for_new_block,
            )
            .await?;
            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn mint_bitcredit_bill(
        &self,
        bill_name: &str,
        mintnode: IdentityPublicData,
        timestamp: i64,
    ) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_peer_id.eq(&bill.payee.peer_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (my_peer_id.eq(&bill.endorsee.peer_id))
        {
            let identity = self.identity_store.get_full().await?;
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                ENDORSED_TO,
                Some((&mintnode, ENDORSED_BY)),
                "",
            )?;
            self.add_block_for_operation(
                bill_name,
                &mut blockchain,
                timestamp,
                OperationCode::Mint,
                identity,
                data_for_new_block,
            )
            .await?;
            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn sell_bitcredit_bill(
        &self,
        bill_name: &str,
        buyer: IdentityPublicData,
        timestamp: i64,
        amount_numbers: u64,
    ) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_peer_id.eq(&bill.payee.peer_id) && !blockchain.has_been_endorsed_or_sold())
            || (my_peer_id.eq(&bill.endorsee.peer_id))
        {
            let identity = self.identity_store.get_full().await?;
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                SOLD_TO,
                Some((&buyer, SOLD_BY)),
                &format!("{}{amount_numbers}", AMOUNT),
            )?;
            self.add_block_for_operation(
                bill_name,
                &mut blockchain,
                timestamp,
                OperationCode::Sell,
                identity,
                data_for_new_block,
            )
            .await?;
            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn endorse_bitcredit_bill(
        &self,
        bill_name: &str,
        endorsee: IdentityPublicData,
        timestamp: i64,
    ) -> Result<Chain> {
        let my_peer_id = self.identity_store.get_peer_id().await?.to_string();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_name).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_peer_id.eq(&bill.payee.peer_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (my_peer_id.eq(&bill.endorsee.peer_id))
        {
            let identity = self.identity_store.get_full().await?;
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                ENDORSED_TO,
                Some((&endorsee, ENDORSED_BY)),
                "",
            )?;
            self.add_block_for_operation(
                bill_name,
                &mut blockchain,
                timestamp,
                OperationCode::Endorse,
                identity,
                data_for_new_block,
            )
            .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }
}

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
    pub paid: bool,
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

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
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
    pub payee: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
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
    pub files: Vec<File>,
}

#[cfg(test)]
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
            files: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BillKeys {
    pub private_key_pem: String,
    pub public_key_pem: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        service::identity_service::Identity,
        tests::test::{TEST_PRIVATE_KEY, TEST_PUB_KEY},
    };
    use borsh::to_vec;
    use core::str;
    use external::bitcoin::MockBitcoinClientApi;
    use futures::channel::mpsc;
    use libp2p::{identity::Keypair, PeerId};
    use mockall::predicate::{always, eq};
    use persistence::{
        bill::MockBillStoreApi, company::MockCompanyStoreApi, file_upload::MockFileUploadStoreApi,
        identity::MockIdentityStoreApi,
    };
    use std::sync::Arc;

    fn get_baseline_identity() -> IdentityWithAll {
        let mut identity = Identity::new_empty();
        identity.name = "drawer".to_owned();
        identity.public_key_pem = TEST_PUB_KEY.to_owned();
        identity.private_key_pem = TEST_PRIVATE_KEY.to_owned();
        IdentityWithAll {
            identity,
            peer_id: PeerId::random(),
            key_pair: Keypair::generate_ed25519(),
        }
    }

    fn get_baseline_bill(bill_name: &str) -> BitcreditBill {
        let mut bill = BitcreditBill::new_empty();
        let s = bitcoin::secp256k1::Secp256k1::new();
        let private_key = bitcoin::PrivateKey::new(
            s.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
                .0,
            CONFIG.bitcoin_network(),
        );
        let public_key = private_key.public_key(&s);
        bill.payee = IdentityPublicData::new_empty();
        bill.payee.name = "payee".to_owned();
        bill.payee.bitcoin_public_key = public_key.to_string();
        bill.name = bill_name.to_owned();
        bill.public_key = public_key.to_string();
        bill.private_key = private_key.to_string();
        bill
    }

    fn get_genesis_chain(bill_name: &str, bill: Option<BitcreditBill>) -> Chain {
        let bill = bill.unwrap_or(get_baseline_bill("some name"));
        let data = to_vec(&bill).unwrap();
        let encrypted = util::rsa::encrypt_bytes_with_public_key(&data, TEST_PUB_KEY);
        let encoded = hex::encode(encrypted);
        Chain::new(
            Block::new(
                123456,
                "prevhash".to_string(),
                encoded,
                bill_name.to_string(),
                TEST_PUB_KEY.to_owned(),
                OperationCode::Issue,
                TEST_PRIVATE_KEY.to_owned(),
                1731593928,
            )
            .unwrap(),
        )
    }

    fn get_service(mock_storage: MockBillStoreApi) -> BillService {
        let (sender, _) = mpsc::channel(0);
        let mut bitcoin_client = MockBitcoinClientApi::new();
        bitcoin_client
            .expect_get_address_to_pay()
            .returning(|_, _| Ok(String::from("1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2")));
        BillService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(MockIdentityStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(MockIdentityStoreApi::new()),
            Arc::new(MockFileUploadStoreApi::new()),
            Arc::new(bitcoin_client),
        )
    }

    fn get_service_with_file_upload_store(
        mock_storage: MockBillStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
    ) -> BillService {
        let (sender, _) = mpsc::channel(0);
        BillService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(MockIdentityStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(MockIdentityStoreApi::new()),
            Arc::new(mock_file_upload_storage),
            Arc::new(MockBitcoinClientApi::new()),
        )
    }

    fn get_service_with_identity_store(
        mock_storage: MockBillStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
    ) -> BillService {
        let (sender, _) = mpsc::channel(0);
        let mut bitcoin_client = MockBitcoinClientApi::new();
        bitcoin_client
            .expect_get_address_to_pay()
            .returning(|_, _| Ok(String::from("1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2")));
        bitcoin_client.expect_generate_link_to_pay().returning(|_,_,_| String::from("bitcoin:1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2?amount=0.01&message=Payment in relation to bill some bill
"));
        BillService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(MockIdentityStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(mock_identity_storage),
            Arc::new(MockFileUploadStoreApi::new()),
            Arc::new(bitcoin_client),
        )
    }

    #[tokio::test]
    async fn issue_bill_baseline() {
        let expected_file_name = "invoice_00000000-0000-0000-0000-000000000000.pdf";
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let mut file_upload_storage = MockFileUploadStoreApi::new();
        file_upload_storage
            .expect_read_temp_upload_files()
            .returning(move |_| Ok(vec![(expected_file_name.to_string(), file_bytes.clone())]));
        file_upload_storage
            .expect_remove_temp_upload_folder()
            .returning(|_| Ok(()));
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_bill_keys_to_file()
            .returning(|_, _, _| Ok(()));
        storage
            .expect_save_attached_file()
            .returning(move |_, _, _| Ok(()));
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));

        let service = get_service_with_file_upload_store(storage, file_upload_storage);

        let mut identity = Identity::new_empty();
        identity.public_key_pem = TEST_PUB_KEY.to_owned();
        identity.private_key_pem = TEST_PRIVATE_KEY.to_owned();
        let drawer = IdentityWithAll {
            identity,
            peer_id: PeerId::random(),
            key_pair: Keypair::generate_ed25519(),
        };
        let drawee = IdentityPublicData::new_empty();
        let payee = IdentityPublicData::new_empty();

        let bill = service
            .issue_new_bill(
                String::from("UK"),
                String::from("Vienna"),
                100,
                String::from("London"),
                String::from("2030-01-01"),
                String::from("sa"),
                drawer,
                String::from("en"),
                drawee,
                payee,
                Some("1234".to_string()),
                1731593928,
            )
            .await
            .unwrap();

        assert_eq!(bill.files.first().unwrap().name, expected_file_name);
    }

    #[tokio::test]
    async fn save_encrypt_open_decrypt_compare_hashes() {
        let bill_name = "test_bill_name";
        let file_name = "invoice_00000000-0000-0000-0000-000000000000.pdf";
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let expected_encrypted =
            util::rsa::encrypt_bytes_with_public_key(&file_bytes, TEST_PUB_KEY);

        let mut storage = MockBillStoreApi::new();
        storage
            .expect_save_attached_file()
            .with(always(), eq(bill_name), eq(file_name))
            .times(1)
            .returning(|_, _, _| Ok(()));

        storage
            .expect_open_attached_file()
            .with(eq(bill_name), eq(file_name))
            .times(1)
            .returning(move |_, _| Ok(expected_encrypted.clone()));
        let service = get_service(storage);

        let bill_file = service
            .encrypt_and_save_uploaded_file(file_name, &file_bytes, bill_name, TEST_PUB_KEY)
            .await
            .unwrap();
        assert_eq!(
            bill_file.hash,
            String::from("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
        assert_eq!(bill_file.name, String::from(file_name));

        let decrypted = service
            .open_and_decrypt_attached_file(bill_name, file_name, TEST_PRIVATE_KEY)
            .await
            .unwrap();
        assert_eq!(str::from_utf8(&decrypted).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn save_encrypt_propagates_write_file_error() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_save_attached_file().returning(|_, _, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage);

        assert!(service
            .encrypt_and_save_uploaded_file("file_name", &[], "test", TEST_PUB_KEY)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn open_decrypt_propagates_read_file_error() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_open_attached_file().returning(|_, _| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage);

        assert!(service
            .open_and_decrypt_attached_file("test", "test", TEST_PRIVATE_KEY)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn get_bill_keys_calls_storage() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        let service = get_service(storage);

        assert!(service.get_bill_keys("test").await.is_ok());
        assert_eq!(
            service.get_bill_keys("test").await.unwrap().private_key_pem,
            TEST_PRIVATE_KEY.to_owned()
        );
        assert_eq!(
            service.get_bill_keys("test").await.unwrap().public_key_pem,
            TEST_PUB_KEY.to_owned()
        );
    }

    #[tokio::test]
    async fn get_bill_keys_propagates_errors() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage);

        assert!(service.get_bill_keys("test").await.is_err());
    }

    #[test]
    fn get_data_for_new_block_basic() {
        let service = get_service(MockBillStoreApi::new());
        let prefix = "Accepted by: ";
        let res = service.get_data_for_new_block(&get_baseline_identity(), prefix, None, "");

        assert!(res.is_ok());
        assert!(res.unwrap().contains(prefix));
    }

    #[test]
    fn get_data_for_new_block_with_midfix() {
        let service = get_service(MockBillStoreApi::new());
        let prefix = "Sold by: ";
        let midfix = " sold to: ";
        let res = service.get_data_for_new_block(
            &get_baseline_identity(),
            prefix,
            Some((&IdentityPublicData::new_empty(), midfix)),
            "",
        );

        assert!(res.is_ok());
        let unwrapped = res.unwrap();
        assert!(unwrapped.contains(prefix));
        assert!(unwrapped.contains(midfix));
    }

    #[test]
    fn get_data_for_new_block_with_midfix_and_postfix() {
        let service = get_service(MockBillStoreApi::new());
        let prefix = "Sold by: ";
        let midfix = " sold to: ";
        let postfix = " with amount: 5000 ";
        let res = service.get_data_for_new_block(
            &get_baseline_identity(),
            prefix,
            Some((&IdentityPublicData::new_empty(), midfix)),
            postfix,
        );

        assert!(res.is_ok());
        let unwrapped = res.unwrap();
        assert!(unwrapped.contains(prefix));
        assert!(unwrapped.contains(midfix));
        assert!(unwrapped.contains(postfix));
    }

    #[tokio::test]
    async fn add_block_for_operation_baseline() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        let service = get_service(storage);
        let identity = get_baseline_identity();
        let data_for_new_block = service
            .get_data_for_new_block(&identity, "Requested to pay by ", None, "")
            .unwrap();
        let mut chain = get_genesis_chain("some name", None);

        let res = service
            .add_block_for_operation(
                "some name",
                &mut chain,
                1731593928,
                OperationCode::RequestToPay,
                identity,
                data_for_new_block,
            )
            .await;
        assert!(res.is_ok());
        assert!(chain.blocks.len() == 2);
        assert!(chain.get_latest_block().operation_code == OperationCode::RequestToPay);
    }

    #[tokio::test]
    async fn add_block_for_operation_fails_if_key_fetching_fails() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage);
        let identity = get_baseline_identity();
        let mut chain = get_genesis_chain("some name", None);

        let res = service
            .add_block_for_operation(
                "some name",
                &mut chain,
                1731593928,
                OperationCode::RequestToPay,
                identity.clone(),
                service
                    .get_data_for_new_block(&identity, "Requested to pay by ", None, "")
                    .unwrap(),
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_bills_baseline() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(|_| Ok(get_genesis_chain("some name", None)));
        storage
            .expect_get_bills()
            .returning(|| Ok(vec![get_baseline_bill("some name")]));
        let service = get_service(storage);

        let res = service.get_bills().await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].name, "some name".to_string());
    }

    #[tokio::test]
    async fn get_bills_empty_for_no_bills() {
        let mut storage = MockBillStoreApi::new();
        storage.expect_get_bills().returning(|| Ok(vec![]));
        let service = get_service(storage);

        let res = service.get_bills().await;
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_full_bill_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let drawee_peer_id = bill.drawee.peer_id.clone();
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.get_full_bill("some name", 1731593928).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().name, "some name".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.peer_id, drawee_peer_id);
    }

    #[tokio::test]
    async fn accept_bill_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::Accept);
    }

    #[tokio::test]
    async fn accept_bill_fails_if_drawee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn accept_bill_fails_if_already_accepted() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        let mut chain = get_genesis_chain("some name", Some(bill.clone()));
        chain.blocks.push(
            Block::new(
                123456,
                "prevhash".to_string(),
                "hash".to_string(),
                "some name".to_string(),
                TEST_PUB_KEY.to_owned(),
                OperationCode::Accept,
                TEST_PRIVATE_KEY.to_owned(),
                1731593928,
            )
            .unwrap(),
        );
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(chain.clone()));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_pay_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.request_pay("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::RequestToPay);
    }

    #[tokio::test]
    async fn request_pay_fails_if_payee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.request_pay("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_acceptance_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.request_acceptance("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::RequestToAccept);
    }

    #[tokio::test]
    async fn request_acceptance_fails_if_payee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service.request_acceptance("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .mint_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::Mint);
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_fails_if_payee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .mint_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .sell_bitcredit_bill(
                "some name",
                IdentityPublicData::new_empty(),
                1731593928,
                15000,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::Sell);
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_fails_if_payee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .sell_bitcredit_bill(
                "some name",
                IdentityPublicData::new_empty(),
                1731593928,
                15000,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_baseline() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(identity.peer_id.to_string());
        let mut storage = MockBillStoreApi::new();
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .endorse_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks.len() == 2);
        assert!(res.unwrap().blocks[1].operation_code == OperationCode::Endorse);
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_fails_if_payee_not_caller() {
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_peer_id(PeerId::random().to_string());
        let mut storage = MockBillStoreApi::new();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key_pem: TEST_PRIVATE_KEY.to_owned(),
                public_key_pem: TEST_PUB_KEY.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain("some name", Some(bill.clone()))));
        let mut identity_storage = MockIdentityStoreApi::new();
        identity_storage
            .expect_get_peer_id()
            .returning(move || Ok(identity.peer_id));
        let service = get_service_with_identity_store(storage, identity_storage);

        let res = service
            .endorse_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_err());
    }
}
