use super::company_service::CompanyKeys;
use super::contact_service::{ContactType, IdentityPublicData};
use super::identity_service::Identity;
use super::notification_service::{self, Notification, NotificationServiceApi};
use crate::blockchain::bill::block::{
    BillAcceptBlockData, BillEndorseBlockData, BillIdentityBlockData, BillIssueBlockData,
    BillMintBlockData, BillOfferToSellBlockData, BillRequestToAcceptBlockData,
    BillRequestToPayBlockData, BillSellBlockData, BillSignatoryBlockData,
};
use crate::blockchain::bill::{
    BillBlock, BillBlockchain, BillBlockchainToReturn, BillOpCode, WaitingForPayment,
};
use crate::blockchain::company::{CompanyBlock, CompanySignCompanyBillBlockData};
use crate::blockchain::identity::{IdentityBlock, IdentitySignPersonBillBlockData};
use crate::blockchain::{self, Blockchain};
use crate::constants::PAYMENT_DEADLINE_SECONDS;
use crate::external::bitcoin::BitcoinClientApi;
use crate::persistence::bill::BillChainStoreApi;
use crate::persistence::company::CompanyChainStoreApi;
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::{IdentityChainStoreApi, IdentityStoreApi};
use crate::persistence::ContactStoreApi;
use crate::util::BcrKeys;
use crate::web::data::{BillCombinedBitcoinKey, File};
use crate::{dht, external, persistence, util};
use crate::{
    dht::{Client, GossipsubEvent, GossipsubEventId},
    persistence::bill::BillStoreApi,
};
use crate::{error, CONFIG};
use async_trait::async_trait;
use borsh::to_vec;
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
    /// error returned if the combined bitcoin private key for a bill can't be returned to the
    /// caller
    #[error("Caller can not request combined bitcoin key for bill")]
    CannotReturnCombinedBitcoinKeyForBill,

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

    /// error returned if the bill is not currently an offer to sell waiting for payment
    #[error("Caller is not payee, or endorsee")]
    BillIsNotOfferToSellWaitingForPayment,

    /// error returned if the selling data of selling a bill does not match the waited for offer to
    /// sell
    #[error("Sell data does not match offer to sell")]
    BillSellDataInvalid,

    /// error returned if the bill is offered to sell and waiting for payment
    #[error("Bill is offered to sell and waiting for payment")]
    BillIsOfferedToSellAndWaitingForPayment,

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

    #[error("io error {0}")]
    Io(#[from] std::io::Error),
}

impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'o> {
        match self {
            Error::BillAlreadyAccepted => Status::BadRequest.respond_to(req),
            Error::CannotReturnCombinedBitcoinKeyForBill => Status::BadRequest.respond_to(req),
            Error::BillIsNotOfferToSellWaitingForPayment => Status::BadRequest.respond_to(req),
            Error::BillIsOfferedToSellAndWaitingForPayment => Status::BadRequest.respond_to(req),
            Error::BillSellDataInvalid => Status::BadRequest.respond_to(req),
            Error::CallerIsNotDrawee => Status::BadRequest.respond_to(req),
            Error::CallerIsNotPayeeOrEndorsee => Status::BadRequest.respond_to(req),
            Error::Io(e) => {
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

    /// Gets the combined bitcoin private key for a given bill
    async fn get_combined_bitcoin_key_for_bill(
        &self,
        bill_id: &str,
    ) -> Result<BillCombinedBitcoinKey>;

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
    async fn request_pay(
        &self,
        bill_id: &str,
        currency_code: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// request acceptance for a bill
    async fn request_acceptance(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain>;

    /// mint bitcredit bill
    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        amount_numbers: u64,
        currency_code: &str,
        mintnode: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// offer to sell bitcredit bill
    async fn offer_to_sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        amount_numbers: u64,
        currency_code: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// sell bitcredit bill
    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        amount_numbers: u64,
        currency_code: &str,
        payment_address: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// endorse bitcredit bill
    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// Check payment status of bills that are requested to pay and not expired and not paid yet, updating their
    /// paid status if they were paid
    async fn check_bills_payment(&self) -> Result<()>;

    /// Check payment status of bills that are waiting for a payment on an OfferToSell block, which
    /// haven't been expired, adding a Sell block if they were paid
    async fn check_bills_offer_to_sell_payment(&self) -> Result<()>;
}

/// The bill service is responsible for all bill-related logic and for syncing them with the dht data.
#[derive(Clone)]
pub struct BillService {
    client: Client,
    store: Arc<dyn BillStoreApi>,
    blockchain_store: Arc<dyn BillChainStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
    bitcoin_client: Arc<dyn BitcoinClientApi>,
    notification_service: Arc<dyn NotificationServiceApi>,
    identity_blockchain_store: Arc<dyn IdentityChainStoreApi>,
    company_blockchain_store: Arc<dyn CompanyChainStoreApi>,
    contact_store: Arc<dyn ContactStoreApi>,
}

impl BillService {
    pub fn new(
        client: Client,
        store: Arc<dyn BillStoreApi>,
        blockchain_store: Arc<dyn BillChainStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
        file_upload_store: Arc<dyn FileUploadStoreApi>,
        bitcoin_client: Arc<dyn BitcoinClientApi>,
        notification_service: Arc<dyn NotificationServiceApi>,
        identity_blockchain_store: Arc<dyn IdentityChainStoreApi>,
        company_blockchain_store: Arc<dyn CompanyChainStoreApi>,
        contact_store: Arc<dyn ContactStoreApi>,
    ) -> Self {
        Self {
            client,
            store,
            blockchain_store,
            identity_store,
            file_upload_store,
            bitcoin_client,
            notification_service,
            identity_blockchain_store,
            company_blockchain_store,
            contact_store,
        }
    }

    async fn validate_and_add_block(
        &self,
        bill_id: &str,
        blockchain: &mut BillBlockchain,
        new_block: BillBlock,
    ) -> Result<()> {
        let try_add_block = blockchain.try_add_block(new_block.clone());
        if try_add_block && blockchain.is_chain_valid() {
            self.blockchain_store.add_block(bill_id, &new_block).await?;
            Ok(())
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
                operation: block.op_code.clone(),
            },
            keys,
            timestamp,
        )?;
        self.identity_blockchain_store.add_block(&new_block).await?;
        Ok(())
    }

    async fn add_block_to_company_chain_for_signed_bill_action(
        &self,
        company_id: &str,
        bill_id: &str,
        block: &BillBlock,
        signatory_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        timestamp: u64,
    ) -> Result<()> {
        let previous_block = self
            .company_blockchain_store
            .get_latest_block(company_id)
            .await?;
        let new_block = CompanyBlock::create_block_for_sign_company_bill(
            company_id.to_owned(),
            &previous_block,
            &CompanySignCompanyBillBlockData {
                bill_id: bill_id.to_owned(),
                block_id: block.id,
                block_hash: block.hash.to_owned(),
                operation: block.op_code.clone(),
            },
            signatory_keys,
            company_keys,
            timestamp,
        )?;
        self.company_blockchain_store
            .add_block(company_id, &new_block)
            .await?;
        Ok(())
    }

    /// If it's our identity, we take the fields from there, otherwise we check contacts, or leave
    /// them empty
    async fn extend_bill_chain_identity_data_from_contacts_or_identity(
        &self,
        chain_identity: BillIdentityBlockData,
        identity: &Identity,
    ) -> IdentityPublicData {
        let (email, nostr_relay) = match chain_identity.node_id {
            ref v if *v == identity.node_id => {
                (Some(identity.email.clone()), identity.nostr_relay.clone())
            }
            ref other_node_id => {
                if let Ok(Some(contact)) = self.contact_store.get(other_node_id).await {
                    (
                        Some(contact.email.clone()),
                        contact.nostr_relays.first().cloned(),
                    )
                } else {
                    (None, None)
                }
            }
        };
        IdentityPublicData {
            t: chain_identity.t,
            node_id: chain_identity.node_id,
            name: chain_identity.name,
            postal_address: chain_identity.postal_address,
            email,
            nostr_relay,
        }
    }

    /// We try to get the additional contact fields from the identity or contacts for each identity
    /// on the bill
    async fn get_last_version_bill(
        &self,
        chain: &BillBlockchain,
        bill_keys: &BillKeys,
        identity: &Identity,
    ) -> Result<BitcreditBill> {
        let bill_first_version = chain.get_first_version_bill(bill_keys)?;

        let mut last_endorsee = None;

        if chain.blocks().len() > 1 && chain.has_been_endorsed_sold_or_minted() {
            let last_version_block_endorse =
                chain.get_last_version_block_with_op_code(BillOpCode::Endorse);
            let last_version_block_mint =
                chain.get_last_version_block_with_op_code(BillOpCode::Mint);
            let last_version_block_offer_to_sell =
                chain.get_last_version_block_with_op_code(BillOpCode::OfferToSell);
            let last_version_block_sold =
                chain.get_last_version_block_with_op_code(BillOpCode::Sell);

            // If the last block is sold, the buyer is the holder
            if (last_version_block_endorse.id < last_version_block_sold.id)
                && (last_version_block_mint.id < last_version_block_sold.id)
                && (last_version_block_offer_to_sell.id < last_version_block_sold.id)
            {
                let block_data_decrypted: BillSellBlockData =
                    last_version_block_sold.get_decrypted_block_bytes(bill_keys)?;
                let buyer = block_data_decrypted.buyer;

                last_endorsee = Some(buyer);
            // If the last block is offer to sell, the seller is the holder
            } else if chain.block_with_operation_code_exists(BillOpCode::OfferToSell.clone())
                && (last_version_block_offer_to_sell.id > last_version_block_endorse.id)
                && (last_version_block_offer_to_sell.id > last_version_block_mint.id)
            {
                let block_data_decrypted: BillOfferToSellBlockData =
                    last_version_block_offer_to_sell.get_decrypted_block_bytes(bill_keys)?;
                let seller = block_data_decrypted.seller;

                last_endorsee = Some(seller);
            // If the last block is endorse, the endorsee is the holder
            } else if chain.block_with_operation_code_exists(BillOpCode::Endorse.clone())
                && (last_version_block_endorse.id > last_version_block_mint.id)
                && (last_version_block_endorse.id > last_version_block_offer_to_sell.id)
            {
                let block_data_decrypted: BillEndorseBlockData =
                    last_version_block_endorse.get_decrypted_block_bytes(bill_keys)?;
                let endorsee = block_data_decrypted.endorsee;

                last_endorsee = Some(endorsee);
            // If the last block is mint, the mint is the holder
            } else if chain.block_with_operation_code_exists(BillOpCode::Mint.clone())
                && (last_version_block_mint.id > last_version_block_endorse.id)
                && (last_version_block_mint.id > last_version_block_offer_to_sell.id)
            {
                let block_data_decrypted: BillMintBlockData =
                    last_version_block_mint.get_decrypted_block_bytes(bill_keys)?;
                let mint = block_data_decrypted.endorsee;

                last_endorsee = Some(mint);
            }
        }

        let mut payee = bill_first_version.payee;

        if let Some(ref endorsee) = last_endorsee {
            payee = endorsee.clone();
        }

        let drawee_contact = self
            .extend_bill_chain_identity_data_from_contacts_or_identity(
                bill_first_version.drawee,
                identity,
            )
            .await;
        let drawer_contact = self
            .extend_bill_chain_identity_data_from_contacts_or_identity(
                bill_first_version.drawer,
                identity,
            )
            .await;
        let payee_contact = self
            .extend_bill_chain_identity_data_from_contacts_or_identity(payee, identity)
            .await;
        let endorsee_contact = match last_endorsee {
            Some(endorsee) => {
                let endorsee_contact = self
                    .extend_bill_chain_identity_data_from_contacts_or_identity(endorsee, identity)
                    .await;
                Some(endorsee_contact)
            }
            None => None,
        };

        Ok(BitcreditBill {
            id: bill_first_version.id,
            bill_jurisdiction: bill_first_version.bill_jurisdiction,
            drawee: drawee_contact,
            drawer: drawer_contact,
            payee: payee_contact,
            endorsee: endorsee_contact,
            place_of_drawing: bill_first_version.place_of_drawing,
            currency_code: bill_first_version.currency_code,
            amount_numbers: bill_first_version.amount_numbers,
            amounts_letters: bill_first_version.amounts_letters,
            maturity_date: bill_first_version.maturity_date,
            date_of_issue: bill_first_version.date_of_issue,
            place_of_payment: bill_first_version.place_of_payment,
            language: bill_first_version.language,
            files: bill_first_version.files,
        })
    }
}

#[async_trait]
impl BillServiceApi for BillService {
    async fn get_bills(&self) -> Result<Vec<BitcreditBillToReturn>> {
        let mut res = vec![];
        let bill_ids = self.store.get_ids().await?;
        let identity = self.identity_store.get().await?;

        for bill_id in bill_ids {
            let chain = self.blockchain_store.get_chain(&bill_id).await?;
            let bill_keys = self.store.get_keys(&bill_id).await?;
            let bill = self
                .get_last_version_bill(&chain, &bill_keys, &identity)
                .await?;
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
                .get_address_to_pay(&bill_keys.public_key, holder_public_key)?;
            let mut paid = false;
            if requested_to_pay {
                paid = self.store.is_paid(&bill.id).await?;
            }

            let active_notification = self
                .notification_service
                .get_active_bill_notification(&bill.id)
                .await;

            res.push(BitcreditBillToReturn {
                id: bill.id,
                bill_jurisdiction: bill.bill_jurisdiction,
                drawee: bill.drawee,
                drawer: bill.drawer,
                payee: bill.payee,
                endorsee: bill.endorsee,
                place_of_drawing: bill.place_of_drawing,
                currency_code: bill.currency_code,
                amount_numbers: bill.amount_numbers,
                amounts_letters: bill.amounts_letters,
                maturity_date: bill.maturity_date,
                date_of_issue: bill.date_of_issue,
                place_of_payment: bill.place_of_payment,
                language: bill.language,
                accepted,
                endorsed,
                waited_for_payment: false,
                buyer: None,
                seller: None,
                requested_to_pay,
                requested_to_accept,
                paid,
                link_to_pay: "".to_string(),
                link_for_buy: "".to_string(),
                address_to_pay,
                chain_of_blocks: chain_to_return,
                active_notification,
            });
        }

        Ok(res)
    }

    async fn get_combined_bitcoin_key_for_bill(
        &self,
        bill_id: &str,
    ) -> Result<BillCombinedBitcoinKey> {
        let identity = self.identity_store.get_full().await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        let bill = self
            .get_last_version_bill(&chain, &bill_keys, &identity.identity)
            .await?;
        let endorsed = chain.block_with_operation_code_exists(BillOpCode::Endorse);

        let holder_public_key = match bill.endorsee {
            None => &bill.payee.node_id,
            Some(ref endorsee) => &endorsee.node_id,
        };

        if (!endorsed && bill.payee.node_id.clone().eq(&identity.identity.node_id))
            || (endorsed && holder_public_key.eq(&identity.identity.node_id))
        {
            let private_key = self.bitcoin_client.get_combined_private_key(
                &identity
                    .key_pair
                    .get_bitcoin_private_key(CONFIG.bitcoin_network()),
                &BcrKeys::from_private_key(&bill_keys.private_key)?
                    .get_bitcoin_private_key(CONFIG.bitcoin_network()),
            )?;
            return Ok(BillCombinedBitcoinKey { private_key });
        }
        Err(Error::CannotReturnCombinedBitcoinKeyForBill)
    }

    async fn get_full_bill(
        &self,
        bill_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn> {
        let identity = self.identity_store.get_full().await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, &identity.identity)
            .await?;

        let mut link_for_buy = "".to_string();
        let chain_to_return = BillBlockchainToReturn::new(chain.clone(), &bill_keys)?;
        let endorsed = chain.block_with_operation_code_exists(BillOpCode::Endorse);
        let accepted = chain.block_with_operation_code_exists(BillOpCode::Accept);
        let waiting_for_payment =
            chain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, current_timestamp)?;
        let mut waited_for_payment = false;
        let mut buyer = None;
        let mut seller = None;
        if let WaitingForPayment::Yes(payment_info) = waiting_for_payment {
            waited_for_payment = true;
            buyer = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.buyer.clone(),
                    &identity.identity,
                )
                .await,
            );
            seller = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.seller.clone(),
                    &identity.identity,
                )
                .await,
            );

            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &payment_info.seller.node_id)?;

            if identity
                .identity
                .node_id
                .to_string()
                .eq(&payment_info.buyer.node_id)
                || identity
                    .identity
                    .node_id
                    .to_string()
                    .eq(&payment_info.seller.node_id)
            {
                let message: String = format!("Payment in relation to a bill {}", &bill.id);
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
            .get_address_to_pay(&bill_keys.public_key, holder_public_key)?;
        let mut paid = false;
        if requested_to_pay {
            paid = self.store.is_paid(&bill.id).await?;
        }
        let message: String = format!("Payment in relation to a bill {}", bill.id.clone());
        let link_to_pay = self.bitcoin_client.generate_link_to_pay(
            &address_to_pay,
            bill.amount_numbers,
            &message,
        );

        let active_notification = self
            .notification_service
            .get_active_bill_notification(&bill.id)
            .await;

        Ok(BitcreditBillToReturn {
            id: bill.id,
            bill_jurisdiction: bill.bill_jurisdiction,
            drawee: bill.drawee,
            drawer: bill.drawer,
            payee: bill.payee,
            endorsee: bill.endorsee,
            place_of_drawing: bill.place_of_drawing,
            currency_code: bill.currency_code,
            amount_numbers: bill.amount_numbers,
            amounts_letters: bill.amounts_letters,
            maturity_date: bill.maturity_date,
            date_of_issue: bill.date_of_issue,
            place_of_payment: bill.place_of_payment,
            language: bill.language,
            accepted,
            endorsed,
            requested_to_pay,
            requested_to_accept,
            waited_for_payment,
            buyer,
            seller,
            paid,
            link_for_buy,
            link_to_pay,
            address_to_pay,
            chain_of_blocks: chain_to_return,
            active_notification,
        })
    }

    async fn get_bill(&self, bill_id: &str) -> Result<BitcreditBill> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let identity = self.identity_store.get().await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, &identity)
            .await?;
        Ok(bill)
    }

    async fn get_blockchain_for_bill(&self, bill_id: &str) -> Result<BillBlockchain> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
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
        let keys = self.store.get_keys(bill_id).await?;
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
        let public_key = keys.get_public_key();

        let bill_id = util::sha256_hash(public_key.as_bytes());

        self.store
            .save_keys(
                &bill_id,
                &BillKeys {
                    private_key: keys.get_private_key_string(),
                    public_key: keys.get_public_key(),
                },
            )
            .await?;

        let amount_letters: String = util::numbers_to_words::encode(&amount_numbers);

        let utc = Utc::now();
        let date_of_issue = utc.naive_local().date().to_string();

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
            id: bill_id.clone(),
            bill_jurisdiction,
            place_of_drawing,
            currency_code,
            amount_numbers,
            amounts_letters: amount_letters,
            maturity_date,
            date_of_issue,
            place_of_payment,
            language,
            drawee: public_data_drawee,
            drawer: drawer_public_data.clone(),
            payee: public_data_payee,
            endorsee: None,
            files: bill_files,
        };

        let identity_keys = self.identity_store.get_key_pair().await?;
        let (signatory_keys, company_keys, signatory_identity) = match drawer_public_data.t {
            ContactType::Person => (drawer_keys.clone(), None, None),
            ContactType::Company => {
                let identity = self.identity_store.get().await?;
                (
                    identity_keys.clone(),
                    Some(drawer_keys.clone()),
                    Some(BillSignatoryBlockData {
                        node_id: identity.node_id,
                        name: identity.name,
                    }),
                )
            }
        };

        let chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill.clone(), signatory_identity, timestamp),
            signatory_keys,
            company_keys,
            keys.clone(),
            timestamp,
        )?;

        let block = chain.get_first_block();
        match drawer_public_data.t {
            ContactType::Person => {
                self.add_block_to_identity_chain_for_signed_bill_action(
                    &bill_id,
                    block,
                    &identity_keys,
                    timestamp,
                )
                .await?;
            }
            ContactType::Company => {
                self.add_block_to_company_chain_for_signed_bill_action(
                    &drawer_public_data.node_id,
                    &bill_id,
                    block,
                    &identity_keys,
                    &CompanyKeys {
                        private_key: drawer_keys.get_private_key_string(),
                        public_key: drawer_keys.get_public_key(),
                    },
                    timestamp,
                )
                .await?;
            }
        }

        self.blockchain_store.add_block(&bill.id, block).await?;

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
        let block_bytes = to_vec(block)?;
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
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        let accepted = blockchain.block_with_operation_code_exists(BillOpCode::Accept);

        if accepted {
            return Err(Error::BillAlreadyAccepted);
        }

        if !bill.drawee.node_id.eq(&my_node_id) {
            return Err(Error::CallerIsNotDrawee);
        }

        let previous_block = blockchain.get_latest_block();
        let block = BillBlock::create_block_for_accept(
            bill_id.to_owned(),
            previous_block,
            &BillAcceptBlockData {
                accepter: IdentityPublicData::new(identity.identity.clone()).into(),
                signatory: None,
                signing_timestamp: timestamp,
                signing_address: identity.identity.postal_address.clone(),
            },
            &identity.key_pair,
            None, // company keys
            &BcrKeys::from_private_key(&bill_keys.private_key)?,
            timestamp,
        )?;
        self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
            .await?;

        self.add_block_to_identity_chain_for_signed_bill_action(
            bill_id,
            &block,
            &identity.key_pair,
            timestamp,
        )
        .await?;

        let last_version_bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;
        self.notification_service
            .send_bill_is_accepted_event(&last_version_bill)
            .await?;

        Ok(blockchain)
    }

    async fn request_pay(
        &self,
        bill_id: &str,
        currency_code: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_request_to_pay(
                bill_id.to_owned(),
                previous_block,
                &BillRequestToPayBlockData {
                    requester: IdentityPublicData::new(identity.identity.clone()).into(),
                    currency_code: currency_code.to_owned(),
                    signatory: None, // company signatory
                    signing_timestamp: timestamp,
                    signing_address: identity.identity.postal_address.clone(),
                },
                &identity.key_pair,
                None, // company keys
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            let last_version_bill = self
                .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
                .await?;
            self.notification_service
                .send_request_to_pay_event(&last_version_bill)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn request_acceptance(&self, bill_id: &str, timestamp: u64) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_request_to_accept(
                bill_id.to_owned(),
                previous_block,
                &BillRequestToAcceptBlockData {
                    requester: IdentityPublicData::new(identity.identity.clone()).into(),
                    signatory: None, // company signatory
                    signing_timestamp: timestamp,
                    signing_address: identity.identity.postal_address.clone(),
                },
                &identity.key_pair,
                None, // company keys
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            let last_version_bill = self
                .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
                .await?;
            self.notification_service
                .send_request_to_accept_event(&last_version_bill)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        amount_numbers: u64,
        currency_code: &str,
        mintnode: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_mint(
                bill_id.to_owned(),
                previous_block,
                &BillMintBlockData {
                    endorser: IdentityPublicData::new(identity.identity.clone()).into(),
                    endorsee: mintnode.into(),
                    currency_code: currency_code.to_owned(),
                    amount: amount_numbers,
                    signatory: None, // company signatory
                    signing_timestamp: timestamp,
                    signing_address: identity.identity.postal_address.clone(),
                },
                &identity.key_pair,
                None, // company keys
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            let last_version_bill = self
                .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
                .await?;
            self.notification_service
                .send_request_to_mint_event(&last_version_bill)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn offer_to_sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        amount_numbers: u64,
        currency_code: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_or_sold())
            || (Some(my_node_id.clone()).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            // The address to pay is the seller's address combined with the bill's address
            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &my_node_id)?;
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_offer_to_sell(
                bill_id.to_owned(),
                previous_block,
                &BillOfferToSellBlockData {
                    seller: IdentityPublicData::new(identity.identity.clone()).into(),
                    buyer: buyer.clone().into(),
                    currency_code: currency_code.to_owned(),
                    amount: amount_numbers,
                    payment_address: address_to_pay,
                    signatory: None, // company signatory
                    signing_timestamp: timestamp,
                    signing_address: identity.identity.postal_address.clone(),
                },
                &identity.key_pair,
                None, // company keys
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_offer_to_sell_event(bill_id, &buyer)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        amount_numbers: u64,
        currency_code: &str,
        payment_address: &str,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if let Ok(WaitingForPayment::Yes(payment_info)) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)
        {
            if payment_info.amount != amount_numbers
                || payment_info.currency_code != currency_code
                || payment_info.payment_address != payment_address
                || payment_info.buyer.node_id != buyer.node_id
                || payment_info.seller.node_id != my_node_id
            {
                return Err(Error::BillSellDataInvalid);
            }

            if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_or_sold())
                || (Some(my_node_id.clone()).eq(&bill.endorsee.map(|e| e.node_id)))
            {
                // The address to pay is the seller's address combined with the bill's address
                let previous_block = blockchain.get_latest_block();
                let block = BillBlock::create_block_for_sold(
                    bill_id.to_owned(),
                    previous_block,
                    &BillSellBlockData {
                        seller: IdentityPublicData::new(identity.identity.clone()).into(),
                        buyer: buyer.into(),
                        currency_code: currency_code.to_owned(),
                        amount: amount_numbers,
                        payment_address: payment_address.to_owned(),
                        signatory: None, // company signatory
                        signing_timestamp: timestamp,
                        signing_address: identity.identity.postal_address.clone(),
                    },
                    &identity.key_pair,
                    None, // company keys
                    &BcrKeys::from_private_key(&bill_keys.private_key)?,
                    timestamp,
                )?;
                self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                    .await?;

                self.add_block_to_identity_chain_for_signed_bill_action(
                    bill_id,
                    &block,
                    &identity.key_pair,
                    timestamp,
                )
                .await?;

                let last_version_bill = self
                    .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
                    .await?;
                self.notification_service
                    .send_bill_is_sold_event(&last_version_bill)
                    .await?;

                return Ok(blockchain);
            } else {
                return Err(Error::CallerIsNotPayeeOrEndorsee);
            }
        }
        Err(Error::BillIsNotOfferToSellWaitingForPayment)
    }

    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;
        let my_node_id = identity.identity.node_id.clone();
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let WaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (my_node_id.eq(&bill.payee.node_id) && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(my_node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_endorse(
                bill_id.to_owned(),
                previous_block,
                &BillEndorseBlockData {
                    endorser: IdentityPublicData::new(identity.identity.clone()).into(),
                    endorsee: endorsee.into(),
                    signatory: None, // company signatory
                    signing_timestamp: timestamp,
                    signing_address: identity.identity.postal_address.clone(),
                },
                &identity.key_pair,
                None, // company keys
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_block_to_identity_chain_for_signed_bill_action(
                bill_id,
                &block,
                &identity.key_pair,
                timestamp,
            )
            .await?;

            let last_version_bill = self
                .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
                .await?;
            self.notification_service
                .send_bill_is_endorsed_event(&last_version_bill)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotPayeeOrEndorsee)
    }

    async fn check_bills_payment(&self) -> Result<()> {
        let identity = self.identity_store.get().await?;
        let bill_ids_waiting_for_payment = self.store.get_bill_ids_waiting_for_payment().await?;

        for bill_id in bill_ids_waiting_for_payment {
            info!("Checking bill payment for {bill_id}");
            let chain = self.blockchain_store.get_chain(&bill_id).await?;
            let bill_keys = self.store.get_keys(&bill_id).await?;
            let bill = self
                .get_last_version_bill(&chain, &bill_keys, &identity)
                .await?;

            // We only check payment, if the maturity date hasn't expired
            if let Some(maturity_date_timestamp) =
                util::date::date_string_to_i64_timestamp(&bill.maturity_date, None)
            {
                if maturity_date_timestamp
                    > (util::date::now().timestamp() - PAYMENT_DEADLINE_SECONDS as i64)
                {
                    let holder_public_key = match bill.endorsee {
                        None => &bill.payee.node_id,
                        Some(ref endorsee) => &endorsee.node_id,
                    };
                    let address_to_pay = self
                        .bitcoin_client
                        .get_address_to_pay(&bill_keys.public_key, holder_public_key)?;
                    if let Ok((paid, sum)) = self
                        .bitcoin_client
                        .check_if_paid(&address_to_pay, bill.amount_numbers)
                        .await
                    {
                        if paid && sum > 0 {
                            self.store.set_to_paid(&bill_id, &address_to_pay).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_bills_offer_to_sell_payment(&self) -> Result<()> {
        let identity = self.identity_store.get().await?;
        let bill_ids_waiting_for_offer_to_sell_payment =
            self.store.get_bill_ids_waiting_for_sell_payment().await?;
        let now = external::time::TimeApi::get_atomic_time().await.timestamp;

        for bill_id in bill_ids_waiting_for_offer_to_sell_payment {
            info!("Checking bill offer to sell payment for {bill_id}");
            let bill_keys = self.store.get_keys(&bill_id).await?;
            let chain = self.blockchain_store.get_chain(&bill_id).await?;
            if let Ok(WaitingForPayment::Yes(payment_info)) =
                chain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, now)
            {
                // check if paid
                if let Ok((paid, sum)) = self
                    .bitcoin_client
                    .check_if_paid(&payment_info.payment_address, payment_info.amount)
                    .await
                {
                    if paid && sum > 0 {
                        self.sell_bitcredit_bill(
                            &bill_id,
                            self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                payment_info.buyer.clone(),
                                &identity,
                            )
                            .await,
                            payment_info.amount,
                            &payment_info.currency_code,
                            &payment_info.payment_address,
                            now,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct BitcreditBillToReturn {
    pub id: String,
    pub bill_jurisdiction: String,
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
    /// Defaulting to the drawee’s id/ address.
    pub place_of_payment: String,
    pub language: String,
    pub accepted: bool,
    pub endorsed: bool,
    pub requested_to_pay: bool,
    pub requested_to_accept: bool,
    pub paid: bool,
    pub waited_for_payment: bool,
    pub buyer: Option<IdentityPublicData>,
    pub seller: Option<IdentityPublicData>,
    pub link_for_buy: String,
    pub link_to_pay: String,
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
    pub id: String,
    pub bill_jurisdiction: String,
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
    // Defaulting to the drawee’s id/ address.
    pub place_of_payment: String,
    pub language: String,
    pub files: Vec<File>,
}

#[cfg(test)]
impl BitcreditBill {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            id: "".to_string(),
            bill_jurisdiction: "".to_string(),
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
            place_of_payment: "".to_string(),
            language: "".to_string(),
            files: vec![],
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone)]
pub struct BillKeys {
    pub private_key: String,
    pub public_key: String,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        service::{
            company_service::tests::get_valid_company_block,
            identity_service::{Identity, IdentityWithAll},
            notification_service::MockNotificationServiceApi,
        },
        tests::tests::{TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP},
    };
    use blockchain::{bill::block::BillIssueBlockData, identity::IdentityBlockchain};
    use core::str;
    use external::bitcoin::MockBitcoinClientApi;
    use futures::channel::mpsc;
    use mockall::predicate::{always, eq};
    use persistence::{
        bill::{MockBillChainStoreApi, MockBillStoreApi},
        company::{MockCompanyChainStoreApi, MockCompanyStoreApi},
        contact::MockContactStoreApi,
        db::contact::tests::get_baseline_contact,
        file_upload::MockFileUploadStoreApi,
        identity::{MockIdentityChainStoreApi, MockIdentityStoreApi},
    };
    use std::sync::Arc;
    use util::crypto::BcrKeys;

    fn get_baseline_identity() -> IdentityWithAll {
        let keys = BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap();
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

        bill.maturity_date = "2099-10-15".to_string();
        bill.payee = IdentityPublicData::new_empty();
        bill.payee.name = "payee".to_owned();
        bill.payee.node_id = keys.get_public_key();
        bill.id = bill_id.to_owned();
        bill
    }

    pub fn get_genesis_chain(bill: Option<BitcreditBill>) -> BillBlockchain {
        let bill = bill.unwrap_or(get_baseline_bill("some id"));
        BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            get_baseline_identity().key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap()
    }

    fn get_service(
        mock_storage: MockBillStoreApi,
        mock_chain_storage: MockBillChainStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_chain_storage: MockIdentityChainStoreApi,
        mock_company_chain_storage: MockCompanyChainStoreApi,
        mock_contact_storage: MockContactStoreApi,
    ) -> BillService {
        get_service_base(
            mock_storage,
            mock_chain_storage,
            mock_identity_storage,
            mock_file_upload_storage,
            mock_identity_chain_storage,
            MockNotificationServiceApi::new(),
            mock_company_chain_storage,
            mock_contact_storage,
        )
    }

    fn get_service_base(
        mock_storage: MockBillStoreApi,
        mock_chain_storage: MockBillChainStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        mock_file_upload_storage: MockFileUploadStoreApi,
        mock_identity_chain_storage: MockIdentityChainStoreApi,
        mock_notification_storage: MockNotificationServiceApi,
        mock_company_chain_storage: MockCompanyChainStoreApi,
        mock_contact_storage: MockContactStoreApi,
    ) -> BillService {
        let (sender, _) = mpsc::channel(0);
        let mut bitcoin_client = MockBitcoinClientApi::new();
        bitcoin_client
            .expect_check_if_paid()
            .returning(|_, _| Ok((true, 100)));
        bitcoin_client
            .expect_get_combined_private_key()
            .returning(|_, _| Ok(String::from("123412341234")));
        bitcoin_client
            .expect_get_address_to_pay()
            .returning(|_, _| Ok(String::from("1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2")));
        bitcoin_client.expect_generate_link_to_pay().returning(|_,_,_| String::from("bitcoin:1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2?amount=0.01&message=Payment in relation to bill some bill"));
        BillService::new(
            Client::new(
                sender,
                Arc::new(MockBillStoreApi::new()),
                Arc::new(MockBillChainStoreApi::new()),
                Arc::new(MockCompanyStoreApi::new()),
                Arc::new(MockCompanyChainStoreApi::new()),
                Arc::new(MockIdentityStoreApi::new()),
                Arc::new(MockFileUploadStoreApi::new()),
            ),
            Arc::new(mock_storage),
            Arc::new(mock_chain_storage),
            Arc::new(mock_identity_storage),
            Arc::new(mock_file_upload_storage),
            Arc::new(bitcoin_client),
            Arc::new(mock_notification_storage),
            Arc::new(mock_identity_chain_storage),
            Arc::new(mock_company_chain_storage),
            Arc::new(mock_contact_storage),
        )
    }

    fn get_storages() -> (
        MockBillStoreApi,
        MockBillChainStoreApi,
        MockIdentityStoreApi,
        MockFileUploadStoreApi,
        MockIdentityChainStoreApi,
        MockCompanyChainStoreApi,
        MockContactStoreApi,
    ) {
        let mut identity_chain_store = MockIdentityChainStoreApi::new();
        let mut company_chain_store = MockCompanyChainStoreApi::new();
        let mut contact_store = MockContactStoreApi::new();
        contact_store
            .expect_get()
            .returning(|_| Ok(Some(get_baseline_contact())));
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
        company_chain_store
            .expect_get_latest_block()
            .returning(|_| Ok(get_valid_company_block()));
        identity_chain_store
            .expect_add_block()
            .returning(|_| Ok(()));
        company_chain_store
            .expect_add_block()
            .returning(|_, _| Ok(()));
        (
            MockBillStoreApi::new(),
            MockBillChainStoreApi::new(),
            MockIdentityStoreApi::new(),
            MockFileUploadStoreApi::new(),
            identity_chain_store,
            company_chain_store,
            contact_store,
        )
    }

    #[tokio::test]
    async fn issue_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            mut file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
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
        storage.expect_save_keys().returning(|_, _| Ok(()));
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get_key_pair()
            .returning(|| Ok(get_baseline_identity().key_pair));

        let mut notification_service = MockNotificationServiceApi::new();

        // should send a bill is signed event
        notification_service
            .expect_send_bill_is_signed_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let drawer = get_baseline_identity();
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
        let (
            storage,
            chain_storage,
            identity_storage,
            mut file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
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
        let (
            storage,
            chain_storage,
            identity_storage,
            mut file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        assert!(service
            .encrypt_and_save_uploaded_file("file_name", &[], "test", TEST_PUB_KEY_SECP)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn open_decrypt_propagates_read_file_error() {
        let (
            storage,
            chain_storage,
            identity_storage,
            mut file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        assert!(service
            .open_and_decrypt_attached_file("test", "test", TEST_PRIVATE_KEY_SECP)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn get_bill_keys_calls_storage() {
        let (
            mut storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
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
        let (
            mut storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        storage.expect_get_keys().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        assert!(service.get_bill_keys("test").await.is_err());
    }

    #[tokio::test]
    async fn get_bills_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();

        let mut notification_service = MockNotificationServiceApi::new();

        identity_storage
            .expect_get()
            .returning(|| Ok(get_baseline_identity().identity));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(|_| Ok(get_genesis_chain(None)));
        storage
            .expect_get_ids()
            .returning(|| Ok(vec!["some id".to_string()]));

        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some id"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_bills().await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].id, "some id".to_string());
    }

    #[tokio::test]
    async fn get_bills_req_to_pay() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity);

        let mut notification_service = MockNotificationServiceApi::new();

        identity_storage
            .expect_get()
            .returning(|| Ok(get_baseline_identity().identity));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_request_to_pay(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestToPayBlockData {
                    requester: IdentityPublicData::new(get_baseline_identity().identity).into(),
                    currency_code: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: "some_address".to_string(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        storage
            .expect_get_ids()
            .returning(|| Ok(vec!["1234".to_string()]));
        storage.expect_is_paid().returning(|_| Ok(true));

        notification_service
            .expect_get_active_bill_notification()
            .with(eq("1234"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_bills().await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].id, "1234".to_string());
        assert!(returned_bills[0].paid);
    }

    #[tokio::test]
    async fn get_bills_empty_for_no_bills() {
        let (
            mut storage,
            chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        storage.expect_get_ids().returning(|| Ok(vec![]));
        identity_storage
            .expect_get()
            .returning(|| Ok(get_baseline_identity().identity));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_bills().await;
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_full_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some id"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_full_bill("some id", 1731593928).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(!res.as_ref().unwrap().waited_for_payment);
        assert!(!res.as_ref().unwrap().paid);
    }

    #[tokio::test]
    async fn get_full_bill_waiting_for_offer_to_sell() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_offer_to_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: IdentityPublicData::new(get_baseline_identity().identity).into(),
                    buyer: IdentityPublicData::new_only_node_id(bill.drawee.node_id.clone()).into(),
                    currency_code: "sat".to_string(),
                    amount: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: "some_address".to_string(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some id"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_full_bill("some id", 1731593928).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(res.as_ref().unwrap().waited_for_payment);
    }

    #[tokio::test]
    async fn get_full_bill_req_to_pay() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_is_paid().returning(|_| Ok(true));
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_request_to_pay(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestToPayBlockData {
                    requester: IdentityPublicData::new(get_baseline_identity().identity).into(),
                    currency_code: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: "some_address".to_string(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        notification_service
            .expect_get_active_bill_notification()
            .with(eq("some id"))
            .returning(|_| None);

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_full_bill("some id", 1731593928).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(res.as_ref().unwrap().paid);
        assert!(!res.as_ref().unwrap().waited_for_payment);
    }

    #[tokio::test]
    async fn accept_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.accept_bill("some id", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::Accept);
    }

    #[tokio::test]
    async fn accept_bill_fails_if_drawee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.accept_bill("some id", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn accept_bill_fails_if_already_accepted() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let keys = identity.key_pair.clone();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let mut chain = get_genesis_chain(Some(bill.clone()));
        chain.blocks_mut().push(
            BillBlock::new(
                "some id".to_string(),
                123456,
                "prevhash".to_string(),
                "hash".to_string(),
                BillOpCode::Accept,
                &keys,
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593928,
            )
            .unwrap(),
        );
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(chain.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.accept_bill("some id", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_pay_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.request_pay("some id", "sat", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::RequestToPay);
    }

    #[tokio::test]
    async fn request_pay_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.request_pay("some id", "sat", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn request_acceptance_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.request_acceptance("some id", 1731593928).await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::RequestToAccept);
    }

    #[tokio::test]
    async fn request_acceptance_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.request_acceptance("some id", 1731593928).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .mint_bitcredit_bill(
                "some id",
                5000,
                "sat",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::Mint);
    }

    #[tokio::test]
    async fn mint_bitcredit_bill_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .mint_bitcredit_bill(
                "some id",
                5000,
                "sat",
                IdentityPublicData::new_empty(),
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn offer_to_sell_bitcredit_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_offer_to_sell_event()
            .returning(|_, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .offer_to_sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::OfferToSell);
    }

    #[tokio::test]
    async fn offer_to_sell_bitcredit_bill_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .offer_to_sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let buyer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let buyer_clone = buyer.clone();
        chain_storage.expect_get_chain().returning(move |_| {
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell = BillBlock::create_block_for_offer_to_sell(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: bill.payee.clone().into(),
                    buyer: buyer_clone.clone().into(),
                    currency_code: "sat".to_owned(),
                    amount: 15000,
                    payment_address: "1234paymentaddress".to_owned(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: "some address".to_owned(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(offer_to_sell);
            Ok(chain)
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                buyer,
                15000,
                "sat",
                "1234paymentaddress",
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 3);
        assert!(res.as_ref().unwrap().blocks()[1].op_code == BillOpCode::OfferToSell);
        assert!(res.as_ref().unwrap().blocks()[2].op_code == BillOpCode::Sell);
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_fails_if_sell_data_is_invalid() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let buyer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        chain_storage.expect_get_chain().returning(move |_| {
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell = BillBlock::create_block_for_offer_to_sell(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: bill.payee.clone().into(),
                    buyer: bill.payee.clone().into(), // buyer is seller, which is invalid
                    currency_code: "sat".to_owned(),
                    amount: 10000, // different amount
                    payment_address: "1234paymentaddress".to_owned(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: "some address".to_owned(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(offer_to_sell);
            Ok(chain)
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Sold event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                buyer,
                15000,
                "sat",
                "1234paymentaddress",
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_fails_if_not_offer_to_sell_waiting_for_payment() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                "1234paymentaddress",
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sell_bitcredit_bill_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                "1234paymentaddress",
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
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
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .endorse_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::Endorse);
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_fails_if_waiting_for_offer_to_sell() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_offer_to_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: IdentityPublicData::new(get_baseline_identity().identity).into(),
                    buyer: IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key())
                        .into(),
                    currency_code: "sat".to_string(),
                    amount: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: "some_address".to_string(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .endorse_bitcredit_bill(
                "1234",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                1731593928,
            )
            .await;
        assert!(res.is_err());
        match res {
            Ok(_) => panic!("expected an error"),
            Err(e) => match e {
                Error::BillIsOfferedToSellAndWaitingForPayment => (),
                _ => panic!("expected a different error"),
            },
        };
    }

    #[tokio::test]
    async fn endorse_bitcredit_bill_fails_if_payee_not_caller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service
            .endorse_bitcredit_bill("some id", IdentityPublicData::new_empty(), 1731593928)
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_combined_bitcoin_key_for_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();

        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(identity.key_pair.get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_combined_bitcoin_key_for_bill("some id").await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn get_combined_bitcoin_key_for_bill_err() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();

        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity.clone()));

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.get_combined_bitcoin_key_for_bill("some id").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn check_bills_payment_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();

        let identity = get_baseline_identity();
        let bill = get_baseline_bill("1234");
        storage
            .expect_get_bill_ids_waiting_for_payment()
            .returning(|| Ok(vec!["1234".to_string()]));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_set_to_paid().returning(|_, _| Ok(()));
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
        identity_storage
            .expect_get()
            .returning(move || Ok(identity.identity.clone()));

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        );

        let res = service.check_bills_payment().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_offer_to_sell_payment_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
        ) = get_storages();

        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity);

        storage
            .expect_get_bill_ids_waiting_for_sell_payment()
            .returning(|| Ok(vec!["1234".to_string()]));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let buyer_node_id = BcrKeys::new().get_public_key();
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_offer_to_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: IdentityPublicData::new(get_baseline_identity().identity).into(),
                    buyer: IdentityPublicData::new_only_node_id(buyer_node_id.clone()).into(),
                    currency_code: "sat".to_string(),
                    amount: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: "some_address".to_string(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get()
            .returning(|| Ok(get_baseline_identity().identity.clone()));
        identity_storage
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity().clone()));

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
        );

        let res = service.check_bills_offer_to_sell_payment().await;
        assert!(res.is_ok());
    }
}
