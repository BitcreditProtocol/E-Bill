use super::contact_service::IdentityPublicData;
use super::identity_service::IdentityWithAll;
use super::notification_service::{self, Notification, NotificationServiceApi};
use crate::blockchain::bill::{
    BillBlock, BillBlockchain, BillBlockchainToReturn, BillOpCode, WaitingForPayment,
};
use crate::blockchain::identity::{IdentityBlock, IdentitySignPersonBillBlockData};
use crate::blockchain::{self, Blockchain};
use crate::constants::{
    ACCEPTED_BY, AMOUNT, COMPOUNDING_INTEREST_RATE_ZERO, ENDORSED_BY, ENDORSED_TO,
    REQ_TO_ACCEPT_BY, REQ_TO_PAY_BY, SOLD_BY, SOLD_TO,
};
use crate::external::bitcoin::BitcoinClientApi;
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::{IdentityChainStoreApi, IdentityStoreApi};
use crate::util::BcrKeys;
use crate::web::data::File;
use crate::{dht, external, persistence, util};
use crate::{
    dht::{Client, GossipsubEvent, GossipsubEventId},
    persistence::bill::BillStoreApi,
};
use crate::{error, CONFIG};
use async_trait::async_trait;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use chrono::Utc;
use log::info;
use rocket::{http::Status, response::Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use utoipa::ToSchema;

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

    /// Errors stemming from cryptography, such as converting keys, encryption and decryption
    #[error("Cryptography error: {0}")]
    Cryptography(#[from] util::crypto::Error),

    #[error("Notification error: {0}")]
    Notification(#[from] notification_service::Error),
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
            Error::Notification(e) => {
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

    /// Gets the full bill for the given bill id
    async fn get_full_bill(
        &self,
        bill_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn>;

    /// Gets the bill for the given bill id
    async fn get_bill(&self, bill_id: &str) -> Result<BitcreditBill>;

    /// Gets the blockchain for the given bill id
    async fn get_blockchain_for_bill(&self, bill_id: &str) -> Result<BillBlockchain>;

    /// Try to get the given bill from the dht and saves it locally, if found
    async fn find_bill_in_dht(&self, bill_id: &str) -> Result<()>;

    /// Gets the keys for a given bill
    async fn get_bill_keys(&self, bill_id: &str) -> Result<BillKeys>;

    /// opens and decrypts the attached file from the given bill
    async fn open_and_decrypt_attached_file(
        &self,
        bill_id: &str,
        file_name: &str,
        bill_private_key: &str,
    ) -> Result<Vec<u8>>;

    /// encrypts and saves the given uploaded file, returning the file name, as well as the hash of
    /// the unencrypted file
    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        bill_id: &str,
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
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
        language: String,
        public_data_drawee: IdentityPublicData,
        public_data_payee: IdentityPublicData,
        file_upload_id: Option<String>,
        timestamp: u64,
    ) -> Result<BitcreditBill>;

    /// propagates the given bill to the DHT
    async fn propagate_bill(
        &self,
        bill_id: &str,
        drawer_node_id: &str,
        drawee_node_id: &str,
        payee_node_id: &str,
    ) -> Result<()>;

    /// propagates the given block to the DHT
    async fn propagate_block(&self, bill_id: &str, block: &BillBlock) -> Result<()>;

    /// adds the given bill for the given node on the DHT
    async fn propagate_bill_for_node(&self, bill_id: &str, node_id: &str) -> Result<()>;

    /// accepts the given bill
    async fn accept_bill(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain>;

    /// request pay for a bill
    async fn request_pay(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain>;

    /// request acceptance for a bill
    async fn request_acceptance(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain>;

    /// mint bitcredit bill
    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        mintnode: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// sell bitcredit bill
    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        timestamp: u64,
        amount_numbers: u64,
    ) -> Result<BillBlockchain>;

    /// endorse bitcredit bill
    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain>;
}

/// The bill service is responsible for all bill-related logic and for syncing them with the dht data.
#[derive(Clone)]
pub struct BillService {
    client: Client,
    store: Arc<dyn BillStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    bitcoin_client: Arc<dyn BitcoinClientApi>,
    notification_service: Arc<dyn NotificationServiceApi>,
    identity_blockchain_store: Arc<dyn IdentityChainStoreApi>,
}

impl BillService {
    pub fn new(
        client: Client,
        store: Arc<dyn BillStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        bitcoin_client: Arc<dyn BitcoinClientApi>,
        notification_service: Arc<dyn NotificationServiceApi>,
        identity_blockchain_store: Arc<dyn IdentityChainStoreApi>,
    ) -> Self {
        Self {
            client,
            store,
            identity_store,
            file_upload_store,
            bitcoin_client,
            notification_service,
            identity_blockchain_store,
        }
    }

    fn get_data_for_new_block(
        &self,
        identity: &IdentityWithAll,
        prefix: &str,
        other_party: Option<(&IdentityPublicData, &str)>,
        postfix: &str,
    ) -> Result<String> {
        let identity_public = IdentityPublicData::new(identity.identity.clone());
        let caller_identity_bytes = serde_json::to_vec(&identity_public)?;
        match other_party {
            None => Ok(format!(
                "{prefix}{}{postfix}",
                &util::base58_encode(&caller_identity_bytes)
            )),
            Some((other_identity, midfix)) => {
                let other_identity_bytes = serde_json::to_vec(&other_identity)?;
                Ok(format!(
                    "{prefix}{}{midfix}{}{postfix}",
                    &util::base58_encode(&other_identity_bytes),
                    &util::base58_encode(&caller_identity_bytes)
                ))
            }
        }
    }

    /// attempts to add a block for the given operation and with the given prefix to the chain,
    /// mutating the chain
    async fn add_block_for_operation(
        &self,
        bill_id: &str,
        blockchain: &mut BillBlockchain,
        timestamp: u64,
        operation_code: BillOpCode,
        identity: IdentityWithAll,
        data_for_new_block: String,
    ) -> Result<BillBlock> {
        let last_block = blockchain.get_latest_block();

        let keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let data_for_new_block_in_bytes = data_for_new_block.as_bytes();
        let data_for_new_block_encrypted =
            util::crypto::encrypt_ecies(data_for_new_block_in_bytes, &keys.public_key)?;
        let data_for_new_block_encrypted_in_string_format =
            util::base58_encode(&data_for_new_block_encrypted);

        let new_block = BillBlock::new(
            last_block.id + 1,
            last_block.hash.clone(),
            data_for_new_block_encrypted_in_string_format,
            operation_code,
            identity.key_pair,
            timestamp,
        )?;

        let try_add_block = blockchain.try_add_block(new_block.clone());
        if try_add_block && blockchain.is_chain_valid() {
            self.store
                .write_blockchain_to_file(bill_id, blockchain.to_pretty_printed_json()?)
                .await?;
            Ok(new_block)
        } else {
            Err(Error::Blockchain(blockchain::Error::BlockchainInvalid))
        }
    }

    async fn add_block_to_identity_chain_for_signed_bill_action(
        &self,
        bill_id: &str,
        block: &BillBlock,
        keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<()> {
        let previous_block = self.identity_blockchain_store.get_latest_block().await?;
        let new_block = IdentityBlock::create_block_for_sign_person_bill(
            &previous_block,
            &IdentitySignPersonBillBlockData {
                bill_id: bill_id.to_owned(),
                block_id: block.id,
                block_hash: block.hash.to_owned(),
                operation: block.operation_code.clone(),
            },
            keys,
            timestamp,
        )?;
        self.identity_blockchain_store.add_block(&new_block).await?;
        Ok(())
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
            let chain_to_return = BillBlockchainToReturn::new(chain.clone(), &bill_keys)?;
            let endorsed = chain.block_with_operation_code_exists(BillOpCode::Endorse);
            let accepted = chain.block_with_operation_code_exists(BillOpCode::Accept);
            let requested_to_pay = chain.block_with_operation_code_exists(BillOpCode::RequestToPay);
            let requested_to_accept =
                chain.block_with_operation_code_exists(BillOpCode::RequestToAccept);

            let holder_public_key = match bill.endorsee {
                None => &bill.payee.node_id,
                Some(ref endorsee) => &endorsee.node_id,
            };
            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill.public_key, holder_public_key)?;
            let mut paid = false;
            if chain.block_with_operation_code_exists(BillOpCode::RequestToPay) {
                let check_if_already_paid = self
                    .bitcoin_client
                    .check_if_paid(&address_to_pay, bill.amount_numbers)
                    .await?;
                paid = check_if_already_paid.0;
            }

            let active_notification = self
                .notification_service
                .get_active_bill_notification(&bill.name)
                .await;

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
                buyer: None,
                seller: None,
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
                active_notification,
            });
        }

        Ok(res)
    }

    async fn get_full_bill(
        &self,
        bill_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn> {
        let identity = self.identity_store.get_full().await?;
        let chain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = chain.get_last_version_bill(&bill_keys)?;

        let drawer = chain.get_drawer(&bill_keys)?;
        let mut link_for_buy = "".to_string();
        let chain_to_return = BillBlockchainToReturn::new(chain.clone(), &bill_keys)?;
        let endorsed = chain.block_with_operation_code_exists(BillOpCode::Endorse);
        let accepted = chain.block_with_operation_code_exists(BillOpCode::Accept);
        let address_for_selling: String = String::new();
        let amount_for_selling = 0;
        let waiting_for_payment =
            chain.is_last_sell_block_waiting_for_payment(&bill_keys, current_timestamp)?;
        let mut waited_for_payment = false;
        let mut buyer = None;
        let mut seller = None;
        if let WaitingForPayment::Yes(payment_info) = waiting_for_payment {
            buyer = Some(payment_info.buyer.clone());
            seller = Some(payment_info.seller.clone());

            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill.public_key, &payment_info.seller.node_id)?;
            waited_for_payment = self
                .bitcoin_client
                .check_if_paid(&address_to_pay, payment_info.amount)
                .await?
                .0;

            if waited_for_payment
                && (identity
                    .identity
                    .node_id
                    .to_string()
                    .eq(&payment_info.buyer.node_id)
                    || identity
                        .identity
                        .node_id
                        .to_string()
                        .eq(&payment_info.seller.node_id))
            {
                let message: String = format!("Payment in relation to a bill {}", &bill.name);
                link_for_buy = self.bitcoin_client.generate_link_to_pay(
                    &address_to_pay,
                    payment_info.amount,
                    &message,
                );
            }
        }
        let requested_to_pay = chain.block_with_operation_code_exists(BillOpCode::RequestToPay);
        let requested_to_accept =
            chain.block_with_operation_code_exists(BillOpCode::RequestToAccept);
        let holder_public_key = match bill.endorsee {
            None => &bill.payee.node_id,
            Some(ref endorsee) => &endorsee.node_id,
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
        if (!endorsed && bill.payee.node_id.clone().eq(&identity.identity.node_id))
            || (endorsed && holder_public_key.eq(&identity.identity.node_id))
        {
            pr_key_bill = self.bitcoin_client.get_combined_private_key(
                &identity
                    .key_pair
                    .get_bitcoin_private_key(CONFIG.bitcoin_network())
                    .to_string(),
                &bill.private_key,
            )?;
        }

        let active_notification = self
            .notification_service
            .get_active_bill_notification(&bill.name)
            .await;

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
            active_notification,
        })
    }

    async fn get_bill(&self, bill_id: &str) -> Result<BitcreditBill> {
        let chain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = chain.get_last_version_bill(&bill_keys)?;
        Ok(bill)
    }

    async fn get_blockchain_for_bill(&self, bill_id: &str) -> Result<BillBlockchain> {
        let chain = self.store.read_bill_chain_from_file(bill_id).await?;
        Ok(chain)
    }

    async fn find_bill_in_dht(&self, bill_id: &str) -> Result<()> {
        let local_node_id = self.identity_store.get().await?.node_id;
        self.client
            .clone()
            .get_bill_data_from_the_network(bill_id, &local_node_id)
            .await?;
        Ok(())
    }

    async fn get_bill_keys(&self, bill_id: &str) -> Result<BillKeys> {
        let keys = self.store.read_bill_keys_from_file(bill_id).await?;
        Ok(keys)
    }

    async fn open_and_decrypt_attached_file(
        &self,
        bill_id: &str,
        file_name: &str,
        bill_private_key: &str,
    ) -> Result<Vec<u8>> {
        let read_file = self
            .file_upload_store
            .open_attached_file(bill_id, file_name)
            .await?;
        let decrypted = util::crypto::decrypt_ecies(&read_file, bill_private_key)?;
        Ok(decrypted)
    }

    async fn encrypt_and_save_uploaded_file(
        &self,
        file_name: &str,
        file_bytes: &[u8],
        bill_id: &str,
        bill_public_key: &str,
    ) -> Result<File> {
        let file_hash = util::sha256_hash(file_bytes);
        let encrypted = util::crypto::encrypt_ecies(file_bytes, bill_public_key)?;
        self.file_upload_store
            .save_attached_file(&encrypted, bill_id, file_name)
            .await?;
        info!("Saved file {file_name} with hash {file_hash} for bill {bill_id}");
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
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
        language: String,
        public_data_drawee: IdentityPublicData,
        public_data_payee: IdentityPublicData,
        file_upload_id: Option<String>,
        timestamp: u64,
    ) -> Result<BitcreditBill> {
        let keys = BcrKeys::new();
        let (bitcoin_private_key, bitcoin_public_key) =
            keys.get_bitcoin_keys(CONFIG.bitcoin_network());
        let public_key = keys.get_public_key();

        let bill_id = util::sha256_hash(public_key.as_bytes());

        self.store
            .write_bill_keys_to_file(
                bill_id.clone(),
                keys.get_private_key_string(),
                public_key.clone(),
            )
            .await?;

        let amount_letters: String = util::numbers_to_words::encode(&amount_numbers);

        let utc = Utc::now();
        let date_of_issue = utc.naive_local().date().to_string();

        let to_payee = drawer_public_data == public_data_payee;

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
                        &bill_id,
                        &public_key,
                    )
                    .await?,
                );
            }
        }

        let bill = BitcreditBill {
            name: bill_id.clone(),
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
            public_key: bitcoin_public_key.to_string(),
            private_key: bitcoin_private_key.to_string(),
            language,
            drawee: public_data_drawee,
            drawer: drawer_public_data.clone(),
            payee: public_data_payee,
            endorsee: None,
            files: bill_files,
        };

        let chain = BillBlockchain::new(
            &bill,
            drawer_public_data,
            drawer_keys.clone(),
            public_key,
            timestamp,
        )?;

        let block = chain.get_latest_block();
        self.add_block_to_identity_chain_for_signed_bill_action(
            &bill_id,
            block,
            &drawer_keys,
            timestamp,
        )
        .await?;

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

        // send notification to all required recipients
        self.notification_service
            .send_bill_is_signed_event(&bill)
            .await?;

        Ok(bill)
    }

    async fn propagate_block(&self, bill_id: &str, block: &BillBlock) -> Result<()> {
        let block_bytes = serde_json::to_vec(block)?;
        let event = GossipsubEvent::new(GossipsubEventId::BillBlock, block_bytes);
        let message = event.to_byte_array()?;

        self.client
            .clone()
            .add_message_to_bill_topic(message, bill_id)
            .await?;
        Ok(())
    }

    async fn propagate_bill_for_node(&self, bill_id: &str, node_id: &str) -> Result<()> {
        self.client
            .clone()
            .add_bill_to_dht_for_node(bill_id, node_id)
            .await?;
        Ok(())
    }

    async fn propagate_bill(
        &self,
        bill_id: &str,
        drawer_node_id: &str,
        drawee_node_id: &str,
        payee_node_id: &str,
    ) -> Result<()> {
        let mut client = self.client.clone();

        for node in [drawer_node_id, drawee_node_id, payee_node_id] {
            if !node.is_empty() {
                info!("issue bill: add {} for node {}", bill_id, &node);
                client.add_bill_to_dht_for_node(bill_id, node).await?;
            }
        }

        client.subscribe_to_bill_topic(bill_id).await?;
        client.start_providing_bill(bill_id).await?;
        Ok(())
    }

    async fn accept_bill(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;

        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        let accepted = blockchain.block_with_operation_code_exists(BillOpCode::Accept);

        if accepted {
            return Err(Error::BillAlreadyAccepted);
        }

        if !bill.drawee.node_id.eq(&my_node_id) {
            return Err(Error::CallerIsNotDrawee);
        }

        let data_for_new_block = self.get_data_for_new_block(&identity, ACCEPTED_BY, None, "")?;
        let block = self
            .add_block_for_operation(
                bill_id,
                &mut blockchain,
                timestamp,
                BillOpCode::Accept,
                identity.clone(),
                data_for_new_block,
            )
            .await?;

        self.add_block_to_identity_chain_for_signed_bill_action(
            bill_id,
            &block,
            &identity.key_pair,
            timestamp,
        )
        .await?;

        self.notification_service
            .send_bill_is_accepted_event(&blockchain.get_last_version_bill(&bill_keys)?)
            .await?;

        Ok(blockchain)
    }

    async fn request_pay(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let data_for_new_block =
                self.get_data_for_new_block(&identity, REQ_TO_PAY_BY, None, "")?;
            let block = self
                .add_block_for_operation(
                    bill_id,
                    &mut blockchain,
                    timestamp,
                    BillOpCode::RequestToPay,
                    identity.clone(),
                    data_for_new_block,
                )
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_request_to_pay_event(&blockchain.get_last_version_bill(&bill_keys)?)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn request_acceptance(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let data_for_new_block =
                self.get_data_for_new_block(&identity, REQ_TO_ACCEPT_BY, None, "")?;
            let block = self
                .add_block_for_operation(
                    bill_id,
                    &mut blockchain,
                    timestamp,
                    BillOpCode::RequestToAccept,
                    identity.clone(),
                    data_for_new_block,
                )
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_request_to_accept_event(&blockchain.get_last_version_bill(&bill_keys)?)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        mintnode: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                ENDORSED_TO,
                Some((&mintnode, ENDORSED_BY)),
                "",
            )?;
            let block = self
                .add_block_for_operation(
                    bill_id,
                    &mut blockchain,
                    timestamp,
                    BillOpCode::Mint,
                    identity.clone(),
                    data_for_new_block,
                )
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            let new_bill = blockchain.get_last_version_bill(&bill_keys)?;
            self.notification_service
                .send_request_to_mint_event(&new_bill)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        timestamp: u64,
        amount_numbers: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_or_sold())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                SOLD_TO,
                Some((&buyer, SOLD_BY)),
                &format!("{}{amount_numbers}", AMOUNT),
            )?;
            let block = self
                .add_block_for_operation(
                    bill_id,
                    &mut blockchain,
                    timestamp,
                    BillOpCode::Sell,
                    identity.clone(),
                    data_for_new_block,
                )
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_request_to_sell_event(&blockchain.get_last_version_bill(&bill_keys)?)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.store.read_bill_chain_from_file(bill_id).await?;
        let bill_keys = self.store.read_bill_keys_from_file(bill_id).await?;
        let bill = blockchain.get_last_version_bill(&bill_keys)?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let data_for_new_block = self.get_data_for_new_block(
                &identity,
                ENDORSED_TO,
                Some((&endorsee, ENDORSED_BY)),
                "",
            )?;
            let block = self
                .add_block_for_operation(
                    bill_id,
                    &mut blockchain,
                    timestamp,
                    BillOpCode::Endorse,
                    identity.clone(),
                    data_for_new_block,
                )
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_bill_is_endorsed_event(&blockchain.get_last_version_bill(&bill_keys)?)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(crate = "rocket::serde")]
pub struct BitcreditBillToReturn {
    pub name: String,
    pub to_payee: bool,
    pub bill_jurisdiction: String,
    pub timestamp_at_drawing: u64,
    /// The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    /// The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    /// The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
    pub place_of_drawing: String,
    pub currency_code: String,
    pub amount_numbers: u64,
    pub amounts_letters: String,
    pub maturity_date: String,
    pub date_of_issue: String,
    pub compounding_interest_rate: u64,
    pub type_of_interest_calculation: bool,
    /// Defaulting to the drawee’s id/ address.
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
    pub buyer: Option<IdentityPublicData>,
    pub seller: Option<IdentityPublicData>,
    pub link_for_buy: String,
    pub link_to_pay: String,
    pub pr_key_bill: String,
    pub number_of_confirmations: u64,
    pub pending: bool,
    pub address_to_pay: String,
    pub chain_of_blocks: BillBlockchainToReturn,
    /// The currently active notification for this bill if any
    pub active_notification: Option<Notification>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
pub struct BitcreditEbillQuote {
    pub bill_id: String,
    pub quote_id: String,
    pub amount: u64,
    pub mint_node_id: String,
    pub mint_url: String,
    pub accepted: bool,
    pub token: String,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
pub struct BitcreditBill {
    pub name: String,
    pub to_payee: bool,
    pub bill_jurisdiction: String,
    pub timestamp_at_drawing: u64,
    // The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    // The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
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
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            name: "".to_string(),
            to_payee: false,
            bill_jurisdiction: "".to_string(),
            timestamp_at_drawing: 0,
            drawee: IdentityPublicData::new_empty(),
            drawer: IdentityPublicData::new_empty(),
            payee: IdentityPublicData::new_empty(),
            endorsee: None,
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
    pub private_key: String,
    pub public_key: String,
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{
        service::{identity_service::Identity, notification_service::MockNotificationServiceApi},
        tests::test::{TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP},
    };
    use blockchain::identity::IdentityBlockchain;
    use core::str;
    use external::bitcoin::MockBitcoinClientApi;
    use futures::channel::mpsc;
    use mockall::predicate::{always, eq};
    use persistence::{
        bill::MockBillStoreApi,
        company::{MockCompanyChainStoreApi, MockCompanyStoreApi},
        file_upload::MockFileUploadStoreApi,
        identity::{MockIdentityChainStoreApi, MockIdentityStoreApi},
    };
    use std::sync::Arc;
    use util::crypto::BcrKeys;

    fn get_baseline_identity() -> IdentityWithAll {
        let keys = BcrKeys::new();
        let mut identity = Identity::new_empty();
        identity.name = "drawer".to_owned();
        identity.node_id = keys.get_public_key();
        IdentityWithAll {
            identity,
            key_pair: keys,
        }
    }

    pub fn get_baseline_bill(bill_id: &str) -> BitcreditBill {
        let mut bill = BitcreditBill::new_empty();
        let keys = BcrKeys::new();
        let (private_key, public_key) = keys.get_bitcoin_keys(CONFIG.bitcoin_network());

        bill.payee = IdentityPublicData::new_empty();
        bill.payee.name = "payee".to_owned();
        bill.payee.node_id = keys.get_public_key();
        bill.name = bill_id.to_owned();
        bill.public_key = public_key.to_string();
        bill.private_key = private_key.to_string();
        bill
    }

    pub fn get_genesis_chain(bill: Option<BitcreditBill>) -> BillBlockchain {
        let bill = bill.unwrap_or(get_baseline_bill("some name"));
        BillBlockchain::new(
            &bill,
            IdentityPublicData::new_empty(),
            get_baseline_identity().key_pair,
            TEST_PUB_KEY_SECP.to_owned(),
            1731593928,
        )
        .unwrap()
    }

    fn get_service(
        mock_storage: MockBillStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_chain_storage: MockIdentityChainStoreApi,
    ) -> BillService {
        get_service_base(
            mock_storage,
            mock_identity_storage,
            mock_file_upload_storage,
            mock_identity_chain_storage,
            MockNotificationServiceApi::new(),
        )
    }

    fn get_service_base(
        mock_storage: MockBillStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_chain_storage: MockIdentityChainStoreApi,
        mock_notification_service: MockNotificationServiceApi,
    ) -> BillService {
        let (sender, _) = mpsc::channel(0);
        let mut bitcoin_client = MockBitcoinClientApi::new();
        bitcoin_client
            .expect_get_address_to_pay()
            .returning(|_, _| Ok(String::from("1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2")));
        bitcoin_client.expect_generate_link_to_pay().returning(|_,_,_| String::from("bitcoin:1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2?amount=0.01&message=Payment in relation to bill some bill"));
        BillService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(MockCompanyChainStoreApi::new()),
                Arc::new(MockIdentityStoreApi::new()),
                Arc::new(MockFileUploadStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(mock_identity_storage),
            Arc::new(mock_file_upload_storage),
            Arc::new(bitcoin_client),
            Arc::new(mock_notification_service),
            Arc::new(mock_identity_chain_storage),
        )
    }

    fn get_storages() -> (
        MockBillStoreApi,
        MockIdentityStoreApi,
        MockFileUploadStoreApi,
        MockIdentityChainStoreApi,
    ) {
        let mut identity_chain_store = MockIdentityChainStoreApi::new();
        identity_chain_store
            .expect_get_latest_block()
            .returning(|| {
                let identity = Identity::new_empty();
                Ok(
                    IdentityBlockchain::new(&identity.into(), &BcrKeys::new(), 1731593928)
                        .unwrap()
                        .get_latest_block()
                        .clone(),
                )
            });
        identity_chain_store
            .expect_add_block()
            .returning(|_| Ok(()));
        (
            MockBillStoreApi::new(),
            MockIdentityStoreApi::new(),
            MockFileUploadStoreApi::new(),
            identity_chain_store,
        )
    }

    #[tokio::test]
    async fn issue_bill_baseline() {
        let (mut storage, identity_storage, mut file_upload_storage, identity_chain_store) =
            get_storages();
        let expected_file_name = "invoice_00000000-0000-0000-0000-000000000000.pdf";
        let file_bytes = String::from("hello world").as_bytes().to_vec();

        file_upload_storage
            .expect_read_temp_upload_files()
            .returning(move |_| Ok(vec![(expected_file_name.to_string(), file_bytes.clone())]));
        file_upload_storage
            .expect_remove_temp_upload_folder()
            .returning(|_| Ok(()));
        file_upload_storage
            .expect_save_attached_file()
            .returning(move |_, _, _| Ok(()));
        storage
            .expect_write_bill_keys_to_file()
            .returning(|_, _, _| Ok(()));
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));

        let mut notification_service = MockNotificationServiceApi::new();

        // should send a bill is signed event
        notification_service
            .expect_send_bill_is_signed_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let identity = Identity::new_empty();
        let drawer = IdentityWithAll {
            identity,
            key_pair: BcrKeys::new(),
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
                IdentityPublicData::new(drawer.identity),
                drawer.key_pair,
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
        let (storage, identity_storage, mut file_upload_storage, identity_chain_store) =
            get_storages();
        let bill_id = "test_bill_id";
        let file_name = "invoice_00000000-0000-0000-0000-000000000000.pdf";
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let expected_encrypted =
            util::crypto::encrypt_ecies(&file_bytes, TEST_PUB_KEY_SECP).unwrap();

        file_upload_storage
            .expect_save_attached_file()
            .with(always(), eq(bill_id), eq(file_name))
            .times(1)
            .returning(|_, _, _| Ok(()));

        file_upload_storage
            .expect_open_attached_file()
            .with(eq(bill_id), eq(file_name))
            .times(1)
            .returning(move |_, _| Ok(expected_encrypted.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let bill_file = service
            .encrypt_and_save_uploaded_file(file_name, &file_bytes, bill_id, TEST_PUB_KEY_SECP)
            .await
            .unwrap();
        assert_eq!(
            bill_file.hash,
            String::from("DULfJyE3WQqNxy3ymuhAChyNR3yufT88pmqvAazKFMG4")
        );
        assert_eq!(bill_file.name, String::from(file_name));

        let decrypted = service
            .open_and_decrypt_attached_file(bill_id, file_name, TEST_PRIVATE_KEY_SECP)
            .await
            .unwrap();
        assert_eq!(str::from_utf8(&decrypted).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn save_encrypt_propagates_write_file_error() {
        let (storage, identity_storage, mut file_upload_storage, identity_chain_store) =
            get_storages();
        file_upload_storage
            .expect_save_attached_file()
            .returning(|_, _, _| {
                Err(persistence::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "test error",
                )))
            });
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        assert!(service
            .encrypt_and_save_uploaded_file("file_name", &[], "test", TEST_PUB_KEY_SECP)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn open_decrypt_propagates_read_file_error() {
        let (storage, identity_storage, mut file_upload_storage, identity_chain_store) =
            get_storages();
        file_upload_storage
            .expect_open_attached_file()
            .returning(|_, _| {
                Err(persistence::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "test error",
                )))
            });
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        assert!(service
            .open_and_decrypt_attached_file("test", "test", TEST_PRIVATE_KEY_SECP)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn get_bill_keys_calls_storage() {
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        assert!(service.get_bill_keys("test").await.is_ok());
        assert_eq!(
            service.get_bill_keys("test").await.unwrap().private_key,
            TEST_PRIVATE_KEY_SECP.to_owned()
        );
        assert_eq!(
            service.get_bill_keys("test").await.unwrap().public_key,
            TEST_PUB_KEY_SECP.to_owned()
        );
    }

    #[tokio::test]
    async fn get_bill_keys_propagates_errors() {
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        assert!(service.get_bill_keys("test").await.is_err());
    }

    #[test]
    fn get_data_for_new_block_basic() {
        let (storage, identity_storage, file_upload_storage, identity_chain_store) = get_storages();
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );
        let prefix = "Accepted by: ";
        let res = service.get_data_for_new_block(&get_baseline_identity(), prefix, None, "");

        assert!(res.is_ok());
        assert!(res.unwrap().contains(prefix));
    }

    #[test]
    fn get_data_for_new_block_with_midfix() {
        let (storage, identity_storage, file_upload_storage, identity_chain_store) = get_storages();
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );
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
        let (storage, identity_storage, file_upload_storage, identity_chain_store) = get_storages();
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );
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
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );
        let identity = get_baseline_identity();
        let data_for_new_block = service
            .get_data_for_new_block(&identity, "Requested to pay by ", None, "")
            .unwrap();
        let mut chain = get_genesis_chain(None);

        let res = service
            .add_block_for_operation(
                "some name",
                &mut chain,
                1731593928,
                BillOpCode::RequestToPay,
                identity,
                data_for_new_block,
            )
            .await;
        assert!(res.is_ok());
        assert!(chain.blocks().len() == 2);
        assert!(chain.get_latest_block().operation_code == BillOpCode::RequestToPay);
    }

    #[tokio::test]
    async fn add_block_for_operation_fails_if_key_fetching_fails() {
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );
        let identity = get_baseline_identity();
        let mut chain = get_genesis_chain(None);

        let res = service
            .add_block_for_operation(
                "some name",
                &mut chain,
                1731593928,
                BillOpCode::RequestToPay,
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
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();

        let mut notification_service = MockNotificationServiceApi::new();

        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(|_| Ok(get_genesis_chain(None)));
        storage
            .expect_get_bills()
            .returning(|| Ok(vec![get_baseline_bill("some name")]));

        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some name"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service.get_bills().await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].name, "some name".to_string());
    }

    #[tokio::test]
    async fn get_bills_empty_for_no_bills() {
        let (mut storage, identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        storage.expect_get_bills().returning(|| Ok(vec![]));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service.get_bills().await;
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_full_bill_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some name"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service.get_full_bill("some name", 1731593928).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().name, "some name".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
    }

    #[tokio::test]
    async fn accept_bill_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Should send bill accepted event
        notification_service
            .expect_send_bill_is_accepted_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::Accept);
    }

    #[tokio::test]
    async fn accept_bill_fails_if_drawee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn accept_bill_fails_if_already_accepted() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let keys = identity.key_pair.clone();
        let mut bill = get_baseline_bill("some name");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let mut chain = get_genesis_chain(Some(bill.clone()));
        chain.blocks_mut().push(
            BillBlock::new(
                123456,
                "prevhash".to_string(),
                "hash".to_string(),
                BillOpCode::Accept,
                keys,
                1731593928,
            )
            .unwrap(),
        );
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(chain.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service.accept_bill("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_pay_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to pay event should be sent
        notification_service
            .expect_send_request_to_pay_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service.request_pay("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::RequestToPay);
    }

    #[tokio::test]
    async fn request_pay_fails_if_payee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service.request_pay("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_acceptance_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to accept event should be sent
        notification_service
            .expect_send_request_to_accept_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service.request_acceptance("some name", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::RequestToAccept);
    }

    #[tokio::test]
    async fn request_acceptance_fails_if_payee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service.request_acceptance("some name", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Asset request to mint event is sent
        notification_service
            .expect_send_request_to_mint_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service
            .mint_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::Mint);
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_fails_if_payee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service
            .mint_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_baseline() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_request_to_sell_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service
            .sell_bitcredit_bill(
                "some name",
                IdentityPublicData::new_empty(),
                1731593928,
                15000,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::Sell);
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_fails_if_payee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

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
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage
            .expect_write_blockchain_to_file()
            .returning(|_, _| Ok(()));
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Bill is endorsed event should be sent
        notification_service
            .expect_send_bill_is_endorsed_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
        );

        let res = service
            .endorse_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].operation_code == BillOpCode::Endorse);
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_fails_if_payee_not_caller() {
        let (mut storage, mut identity_storage, file_upload_storage, identity_chain_store) =
            get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some name");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_read_bill_keys_from_file().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage
            .expect_read_bill_chain_from_file()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
        );

        let res = service
            .endorse_bitcredit_bill("some name", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_err());
    }
}
