use super::company_service::CompanyKeys;
use super::contact_service::{ContactType, IdentityPublicData, LightIdentityPublicData};
use super::identity_service::{Identity, IdentityWithAll};
use super::notification_service::{self, ActionType, Notification, NotificationServiceApi};
use crate::blockchain::bill::block::{
    BillAcceptBlockData, BillEndorseBlockData, BillIdentityBlockData, BillIssueBlockData,
    BillMintBlockData, BillOfferToSellBlockData, BillRecourseBlockData, BillRejectBlockData,
    BillRequestRecourseBlockData, BillRequestToAcceptBlockData, BillRequestToPayBlockData,
    BillSellBlockData, BillSignatoryBlockData,
};
use crate::blockchain::bill::{
    BillBlock, BillBlockchain, BillBlockchainToReturn, BillOpCode, OfferToSellWaitingForPayment,
    RecourseWaitingForPayment,
};
use crate::blockchain::company::{CompanyBlock, CompanySignCompanyBillBlockData};
use crate::blockchain::identity::{
    IdentityBlock, IdentitySignCompanyBillBlockData, IdentitySignPersonBillBlockData,
};
use crate::blockchain::{self, Block, Blockchain};
use crate::constants::{
    ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS,
};
use crate::external::bitcoin::BitcoinClientApi;
use crate::persistence::bill::BillChainStoreApi;
use crate::persistence::company::{CompanyChainStoreApi, CompanyStoreApi};
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::{IdentityChainStoreApi, IdentityStoreApi};
use crate::persistence::ContactStoreApi;
use crate::service::company_service::Company;
use crate::util::BcrKeys;
use crate::web::data::{
    BillCombinedBitcoinKey, BillsFilterRole, Endorsement, File, LightSignedBy, PastEndorsee,
};
use crate::web::ErrorResponse;
use crate::{dht, external, persistence, util};
use crate::{
    dht::{Client, GossipsubEvent, GossipsubEventId},
    persistence::bill::BillStoreApi,
};
use crate::{error, CONFIG};
use async_trait::async_trait;
use borsh::to_vec;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use futures::future::try_join_all;
use log::info;
#[cfg(test)]
use mockall::automock;
use rocket::http::ContentType;
use rocket::Response;
use rocket::{http::Status, response::Responder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;
use thiserror::Error;
use utoipa::ToSchema;

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// errors that currently return early http status code Status::NotFound
    #[error("not found")]
    NotFound,

    /// errors stemming from trying to do invalid operations
    #[error("invalid operation")]
    InvalidOperation,

    /// error returned if a bill was already accepted and is attempted to be accepted again
    #[error("Bill was already accepted")]
    BillAlreadyAccepted,

    /// error returned if the caller of an operation is not the drawee, but would have to be for it
    /// to be valid, e.g. accepting a  bill
    #[error("Caller is not drawee")]
    CallerIsNotDrawee,

    /// error returned if the caller of an operation is not the holder, but would have to be for it
    /// to be valid, e.g. requesting payment
    #[error("Caller is not holder")]
    CallerIsNotHolder,

    /// error returned if the caller of a reject operation is not the recoursee
    #[error("Caller is not the recoursee and can't reject")]
    CallerIsNotRecoursee,

    /// error returned if the caller of a reject buy operation is not the buyer
    #[error("Caller is not the buyer and can't reject to buy")]
    CallerIsNotBuyer,

    /// error returned if the caller of a reject operation trys to reject a request that is already
    /// expired
    #[error("The request already expired")]
    RequestAlreadyExpired,

    /// error returned if the operation was already rejected
    #[error("The request was already rejected")]
    RequestAlreadyRejected,

    /// error returned if the bill was already paid and hence can't be rejected to be paid
    #[error("The bill was already paid")]
    BillAlreadyPaid,

    /// error returned if the bill was not requested to accept, e.g. when rejecting to accept
    #[error("Bill was not requested to accept")]
    BillWasNotRequestedToAccept,

    /// error returned if the bill was not requested to pay, e.g. when rejecting to pay
    #[error("Bill was not requested to pay")]
    BillWasNotRequestedToPay,

    /// error returned if the bill was not offered to sell, e.g. when rejecting to buy
    #[error("Bill was not offered to sell")]
    BillWasNotOfferedToSell,

    /// error returned someone wants to request acceptance recourse, but the request to accept did
    /// not expire and was not rejected
    #[error("Bill request to accept did not expire and was not rejected")]
    BillRequestToAcceptDidNotExpireAndWasNotRejected,

    /// error returned someone wants to request payment recourse, but the request to pay did
    /// not expire and was not rejected
    #[error("Bill request to pay did not expire and was not rejected")]
    BillRequestToPayDidNotExpireAndWasNotRejected,

    /// error returned if the given recoursee is not a past holder of the bill
    #[error("The given recoursee is not a past holder of the bill")]
    RecourseeNotPastHolder,

    /// error returned if the bill was not requester to recourse, e.g. when rejecting to pay for
    /// recourse
    #[error("Bill was not requested to recourse")]
    BillWasNotRequestedToRecourse,

    /// error returned if the bill is not requested to recourse and is waiting for payment
    #[error("Bill is not waiting for recourse payment")]
    BillIsNotRequestedToRecourseAndWaitingForPayment,

    /// error returned if the bill is not currently an offer to sell waiting for payment
    #[error("Bill is not offer to sell waiting for payment")]
    BillIsNotOfferToSellWaitingForPayment,

    /// error returned if the selling data of selling a bill does not match the waited for offer to
    /// sell
    #[error("Sell data does not match offer to sell")]
    BillSellDataInvalid,

    /// error returned if the selling data of recoursing a bill does not match the request to
    /// recourse
    #[error("Recourse data does not match request to recourse")]
    BillRecourseDataInvalid,

    /// error returned if the bill is offered to sell and waiting for payment
    #[error("Bill is offered to sell and waiting for payment")]
    BillIsOfferedToSellAndWaitingForPayment,

    /// error returned if the bill is in recourse and waiting for payment
    #[error("Bill is in recourse and waiting for payment")]
    BillIsInRecourseAndWaitingForPayment,

    /// error returned if the given file upload id is not a temp file we have
    #[error("No file found for file upload id")]
    NoFileForFileUploadId,

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
            Error::RequestAlreadyExpired
            | Error::BillAlreadyAccepted
            | Error::BillWasNotOfferedToSell
            | Error::BillWasNotRequestedToPay
            | Error::BillWasNotRequestedToAccept
            | Error::BillWasNotRequestedToRecourse
            | Error::BillIsNotOfferToSellWaitingForPayment
            | Error::BillIsOfferedToSellAndWaitingForPayment
            | Error::BillIsInRecourseAndWaitingForPayment
            | Error::BillRequestToAcceptDidNotExpireAndWasNotRejected
            | Error::BillRequestToPayDidNotExpireAndWasNotRejected
            | Error::BillIsNotRequestedToRecourseAndWaitingForPayment
            | Error::BillSellDataInvalid
            | Error::BillAlreadyPaid
            | Error::BillRecourseDataInvalid
            | Error::RecourseeNotPastHolder
            | Error::CallerIsNotDrawee
            | Error::CallerIsNotBuyer
            | Error::CallerIsNotRecoursee
            | Error::RequestAlreadyRejected
            | Error::CallerIsNotHolder
            | Error::NoFileForFileUploadId
            | Error::InvalidOperation => {
                let body =
                    ErrorResponse::new("bad_request", self.to_string(), 400).to_json_string();
                Response::build()
                    .status(Status::BadRequest)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
            }
            Error::Io(e) => {
                error!("{e}");
                Status::InternalServerError.respond_to(req)
            }
            Error::NotFound => {
                let body =
                    ErrorResponse::new("not_found", "not found".to_string(), 404).to_json_string();
                Response::build()
                    .status(Status::NotFound)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), Cursor::new(body))
                    .ok()
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

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BillServiceApi: Send + Sync {
    /// Get bill balances
    async fn get_bill_balances(
        &self,
        currency: &str,
        current_identity_node_id: &str,
    ) -> Result<BillsBalanceOverview>;

    /// Search for bills
    async fn search_bills(
        &self,
        currency: &str,
        search_term: &Option<String>,
        date_range_from: Option<u64>,
        date_range_to: Option<u64>,
        role: &BillsFilterRole,
        current_identity_node_id: &str,
    ) -> Result<Vec<LightBitcreditBillToReturn>>;

    /// Gets all bills
    async fn get_bills(&self, current_identity_node_id: &str)
        -> Result<Vec<BitcreditBillToReturn>>;

    /// Gets all bills from all identities
    async fn get_bills_from_all_identities(&self) -> Result<Vec<BitcreditBillToReturn>>;

    /// Gets the combined bitcoin private key for a given bill
    async fn get_combined_bitcoin_key_for_bill(
        &self,
        bill_id: &str,
        caller_public_data: &IdentityPublicData,
        caller_keys: &BcrKeys,
    ) -> Result<BillCombinedBitcoinKey>;

    /// Gets the detail for the given bill id
    async fn get_detail(
        &self,
        bill_id: &str,
        local_identity: &Identity,
        current_identity_node_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn>;

    /// Gets the bill for the given bill id
    async fn get_bill(&self, bill_id: &str) -> Result<BitcreditBill>;

    /// Try to get the given bill chain from the dht and sync the blocks, if found
    async fn find_and_sync_with_bill_in_dht(&self, bill_id: &str) -> Result<()>;

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
    #[allow(clippy::too_many_arguments)]
    async fn issue_new_bill(
        &self,
        country_of_issuing: String,
        city_of_issuing: String,
        issue_date: String,
        maturity_date: String,
        drawee: IdentityPublicData,
        payee: IdentityPublicData,
        sum: u64,
        currency: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        file_upload_id: Option<String>,
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
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
    async fn accept_bill(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// request pay for a bill
    async fn request_pay(
        &self,
        bill_id: &str,
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// request acceptance for a bill
    async fn request_acceptance(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// request recourse for a bill
    async fn request_recourse(
        &self,
        bill_id: &str,
        recoursee: &IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        recourse_reason: RecourseReason,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// recourse bitcredit bill
    async fn recourse_bitcredit_bill(
        &self,
        bill_id: &str,
        recoursee: IdentityPublicData,
        sum: u64,
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// mint bitcredit bill
    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        sum: u64,
        currency: &str,
        mintnode: IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// offer to sell bitcredit bill
    async fn offer_to_sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        sum: u64,
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// sell bitcredit bill
    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        sum: u64,
        currency: &str,
        payment_address: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// endorse bitcredit bill
    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// reject acceptance for a bill
    async fn reject_acceptance(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// reject payment for a bill
    async fn reject_payment(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// reject buying a bill
    async fn reject_buying(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// reject payment for recourse of a bill
    async fn reject_payment_for_recourse(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain>;

    /// Check payment status of bills that are requested to pay and not expired and not paid yet, updating their
    /// paid status if they were paid
    async fn check_bills_payment(&self) -> Result<()>;

    /// Check payment status of bills that are waiting for a payment on an OfferToSell block, which
    /// haven't been expired, adding a Sell block if they were paid
    async fn check_bills_offer_to_sell_payment(&self) -> Result<()>;

    /// Check payment status of bills that are waiting for a payment on an RequestRecourse block, which
    /// haven't been expired, adding a Recourse block if they were paid
    async fn check_bills_in_recourse_payment(&self) -> Result<()>;

    /// Check if actions expected on bills in certain states have expired and execute the necessary
    /// steps after timeout.
    async fn check_bills_timeouts(&self, now: u64) -> Result<()>;

    /// Returns previous endorseers of the bill to select from for Recourse
    async fn get_past_endorsees(
        &self,
        bill_id: &str,
        current_identity_node_id: &str,
    ) -> Result<Vec<PastEndorsee>>;

    /// Returns all endorsements of the bill
    async fn get_endorsements(
        &self,
        bill_id: &str,
        current_identity_node_id: &str,
    ) -> Result<Vec<Endorsement>>;
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
    company_store: Arc<dyn CompanyStoreApi>,
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
        company_store: Arc<dyn CompanyStoreApi>,
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
            company_store,
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

    async fn add_identity_and_company_chain_blocks_for_signed_bill_action(
        &self,
        signer_public_data: &IdentityPublicData,
        bill_id: &str,
        block: &BillBlock,
        identity_keys: &BcrKeys,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<()> {
        match signer_public_data.t {
            ContactType::Person => {
                self.add_block_to_identity_chain_for_signed_bill_action(
                    bill_id,
                    block,
                    identity_keys,
                    timestamp,
                )
                .await?;
            }
            ContactType::Company => {
                self.add_block_to_company_chain_for_signed_bill_action(
                    &signer_public_data.node_id, // company id
                    bill_id,
                    block,
                    identity_keys,
                    &CompanyKeys {
                        private_key: signer_keys.get_private_key_string(),
                        public_key: signer_keys.get_public_key(),
                    },
                    timestamp,
                )
                .await?;

                self.add_block_to_identity_chain_for_signed_company_bill_action(
                    &signer_public_data.node_id, // company id
                    bill_id,
                    block,
                    identity_keys,
                    timestamp,
                )
                .await?;
            }
        };
        Ok(())
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

    async fn add_block_to_identity_chain_for_signed_company_bill_action(
        &self,
        company_id: &str,
        bill_id: &str,
        block: &BillBlock,
        keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<()> {
        let previous_block = self.identity_blockchain_store.get_latest_block().await?;
        let new_block = IdentityBlock::create_block_for_sign_company_bill(
            &previous_block,
            &IdentitySignCompanyBillBlockData {
                bill_id: bill_id.to_owned(),
                block_id: block.id,
                block_hash: block.hash.to_owned(),
                company_id: company_id.to_owned(),
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

    /// If it's our identity, we take the fields from there, otherwise we check contacts,
    /// companies, or leave them empty
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
                } else if let Ok(company) = self.company_store.get(other_node_id).await {
                    (
                        Some(company.email.clone()),
                        identity.nostr_relay.clone(), // if it's a local company, we take our relay
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

        // check endorsing blocks
        let last_version_block_endorse = if let Some(endorse_block_encrypted) =
            chain.get_last_version_block_with_op_code(BillOpCode::Endorse)
        {
            Some((
                endorse_block_encrypted
                    .get_decrypted_block_bytes::<BillEndorseBlockData>(bill_keys)?,
                endorse_block_encrypted.id,
            ))
        } else {
            None
        };
        let last_version_block_mint = if let Some(mint_block_encrypted) =
            chain.get_last_version_block_with_op_code(BillOpCode::Mint)
        {
            Some((
                mint_block_encrypted.get_decrypted_block_bytes::<BillMintBlockData>(bill_keys)?,
                mint_block_encrypted.id,
            ))
        } else {
            None
        };
        let last_version_block_sell = if let Some(sell_block_encrypted) =
            chain.get_last_version_block_with_op_code(BillOpCode::Sell)
        {
            Some((
                sell_block_encrypted.get_decrypted_block_bytes::<BillSellBlockData>(bill_keys)?,
                sell_block_encrypted.id,
            ))
        } else {
            None
        };

        // If the last block is endorse, the endorsee is the holder
        // If the last block is mint, the mint is the holder
        // If the last block is sell, the buyer is the holder
        let last_endorsee = match (
            last_version_block_endorse,
            last_version_block_mint,
            last_version_block_sell,
        ) {
            (None, None, None) => None,
            (Some((endorse_block, _)), None, None) => Some(endorse_block.endorsee),
            (None, Some((mint_block, _)), None) => Some(mint_block.endorsee),
            (None, None, Some((sell_block, _))) => Some(sell_block.buyer),
            (Some((endorse_block, endorse_block_id)), Some((mint_block, mint_block_id)), None) => {
                if endorse_block_id > mint_block_id {
                    Some(endorse_block.endorsee)
                } else {
                    Some(mint_block.endorsee)
                }
            }
            (Some((endorse_block, endorse_block_id)), None, Some((sell_block, sell_block_id))) => {
                if endorse_block_id > sell_block_id {
                    Some(endorse_block.endorsee)
                } else {
                    Some(sell_block.buyer)
                }
            }
            (None, Some((mint_block, mint_block_id)), Some((sell_block, sell_block_id))) => {
                if sell_block_id > mint_block_id {
                    Some(sell_block.buyer)
                } else {
                    Some(mint_block.endorsee)
                }
            }
            (
                Some((endorse_block, endorse_block_id)),
                Some((mint_block, mint_block_id)),
                Some((sell_block, sell_block_id)),
            ) => {
                if endorse_block_id > mint_block_id && endorse_block_id > sell_block_id {
                    Some(endorse_block.endorsee)
                } else if mint_block_id > sell_block_id {
                    Some(mint_block.endorsee)
                } else {
                    Some(sell_block.buyer)
                }
            }
        };

        let payee = bill_first_version.payee;

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
            country_of_issuing: bill_first_version.country_of_issuing,
            city_of_issuing: bill_first_version.city_of_issuing,
            drawee: drawee_contact,
            drawer: drawer_contact,
            payee: payee_contact,
            endorsee: endorsee_contact,
            currency: bill_first_version.currency,
            sum: bill_first_version.sum,
            maturity_date: bill_first_version.maturity_date,
            issue_date: bill_first_version.issue_date,
            country_of_payment: bill_first_version.country_of_payment,
            city_of_payment: bill_first_version.city_of_payment,
            language: bill_first_version.language,
            files: bill_first_version.files,
        })
    }

    fn get_bill_signing_keys(
        &self,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        signatory_identity: &IdentityWithAll,
    ) -> BillSigningKeys {
        let (signatory_keys, company_keys, signatory_identity) = match signer_public_data.t {
            ContactType::Person => (signer_keys.clone(), None, None),
            ContactType::Company => (
                signatory_identity.key_pair.clone(),
                Some(signer_keys.clone()),
                Some(signatory_identity.identity.clone().into()),
            ),
        };
        BillSigningKeys {
            signatory_keys,
            company_keys,
            signatory_identity,
        }
    }

    async fn get_full_bill(
        &self,
        bill_id: &str,
        local_identity: &Identity,
        current_identity_node_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, local_identity)
            .await?;
        let first_version_bill = chain.get_first_version_bill(&bill_keys)?;
        let time_of_drawing = first_version_bill.signing_timestamp;

        // handle expensive deserialization and decryption logic in parallel on a blocking thread
        // pool as not to block the task queue
        let chain_clone = chain.clone();
        let keys_clone = bill_keys.clone();
        let bill_participants_handle =
            tokio::task::spawn_blocking(move || chain_clone.get_all_nodes_from_bill(&keys_clone));
        let chain_clone = chain.clone();
        let keys_clone = bill_keys.clone();
        let chain_to_return_handle = tokio::task::spawn_blocking(move || {
            BillBlockchainToReturn::new(chain_clone, &keys_clone)
        });
        let (bill_participants_res, chain_to_return_res) =
            tokio::try_join!(bill_participants_handle, chain_to_return_handle).map_err(|e| {
                error!("couldn't get data from bill chain blocks {bill_id}: {e}");
                Error::Blockchain(blockchain::Error::BlockchainParse)
            })?;
        let bill_participants = bill_participants_res?;
        let chain_to_return = chain_to_return_res?;

        let endorsements_count = chain.get_endorsements_count();
        let mut in_recourse = false;
        let mut link_to_pay_recourse = "".to_string();
        let mut link_for_buy = "".to_string();
        let endorsed = chain.block_with_operation_code_exists(BillOpCode::Endorse);
        let accepted = chain.block_with_operation_code_exists(BillOpCode::Accept);
        let last_offer_to_sell_block_waiting_for_payment =
            chain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, current_timestamp)?;
        let last_req_to_recourse_block_waiting_for_payment = chain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, current_timestamp)?;
        let mut waiting_for_payment = false;
        let mut buyer = None;
        let mut seller = None;
        if let OfferToSellWaitingForPayment::Yes(payment_info) =
            last_offer_to_sell_block_waiting_for_payment
        {
            waiting_for_payment = true;
            buyer = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.buyer.clone(),
                    local_identity,
                )
                .await,
            );
            seller = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.seller.clone(),
                    local_identity,
                )
                .await,
            );

            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &payment_info.seller.node_id)?;

            if current_identity_node_id
                .to_string()
                .eq(&payment_info.buyer.node_id)
                || current_identity_node_id
                    .to_string()
                    .eq(&payment_info.seller.node_id)
            {
                let message: String = format!("Payment in relation to a bill {}", &bill.id);
                link_for_buy = self.bitcoin_client.generate_link_to_pay(
                    &address_to_pay,
                    payment_info.sum,
                    &message,
                );
            }
        }
        let mut recourser = None;
        let mut recoursee = None;
        if let RecourseWaitingForPayment::Yes(payment_info) =
            last_req_to_recourse_block_waiting_for_payment
        {
            in_recourse = true;
            recourser = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.recourser.clone(),
                    local_identity,
                )
                .await,
            );
            recoursee = Some(
                self.extend_bill_chain_identity_data_from_contacts_or_identity(
                    payment_info.recoursee.clone(),
                    local_identity,
                )
                .await,
            );

            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &payment_info.recourser.node_id)?;

            if current_identity_node_id
                .to_string()
                .eq(&payment_info.recoursee.node_id)
                || current_identity_node_id
                    .to_string()
                    .eq(&payment_info.recourser.node_id)
            {
                let message: String = format!("Payment in relation to a bill {}", &bill.id);
                link_to_pay_recourse = self.bitcoin_client.generate_link_to_pay(
                    &address_to_pay,
                    payment_info.sum,
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
        let mempool_link_for_address_to_pay = self
            .bitcoin_client
            .get_mempool_link_for_address(&address_to_pay);
        let mut paid = false;
        if requested_to_pay {
            paid = self.store.is_paid(&bill.id).await?;
        }
        let message: String = format!("Payment in relation to a bill {}", bill.id.clone());
        let link_to_pay =
            self.bitcoin_client
                .generate_link_to_pay(&address_to_pay, bill.sum, &message);

        let active_notification = self
            .notification_service
            .get_active_bill_notification(&bill.id)
            .await;

        Ok(BitcreditBillToReturn {
            id: bill.id,
            time_of_drawing,
            time_of_maturity: util::date::date_string_to_i64_timestamp(&bill.maturity_date, None)
                .unwrap_or(0) as u64,
            country_of_issuing: bill.country_of_issuing,
            city_of_issuing: bill.city_of_issuing,
            drawee: bill.drawee,
            drawer: bill.drawer,
            payee: bill.payee,
            endorsee: bill.endorsee,
            currency: bill.currency,
            sum: util::currency::sum_to_string(bill.sum),
            maturity_date: bill.maturity_date,
            issue_date: bill.issue_date,
            country_of_payment: bill.country_of_payment,
            city_of_payment: bill.city_of_payment,
            language: bill.language,
            accepted,
            endorsed,
            requested_to_pay,
            requested_to_accept,
            waiting_for_payment,
            buyer,
            seller,
            paid,
            link_for_buy,
            link_to_pay,
            in_recourse,
            recourser,
            recoursee,
            link_to_pay_recourse,
            address_to_pay,
            mempool_link_for_address_to_pay,
            chain_of_blocks: chain_to_return,
            files: bill.files,
            active_notification,
            bill_participants,
            endorsements_count,
        })
    }

    async fn check_bill_payment(&self, bill_id: &str, identity: &Identity) -> Result<()> {
        info!("Checking bill payment for {bill_id}");
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, identity)
            .await?;

        let holder_public_key = match bill.endorsee {
            None => &bill.payee.node_id,
            Some(ref endorsee) => &endorsee.node_id,
        };
        let address_to_pay = self
            .bitcoin_client
            .get_address_to_pay(&bill_keys.public_key, holder_public_key)?;
        if let Ok((paid, sum)) = self
            .bitcoin_client
            .check_if_paid(&address_to_pay, bill.sum)
            .await
        {
            if paid && sum > 0 {
                self.store.set_to_paid(bill_id, &address_to_pay).await?;
            }
        }
        Ok(())
    }

    async fn check_bill_in_recourse_payment(
        &self,
        bill_id: &str,
        identity: &IdentityWithAll,
        now: u64,
    ) -> Result<()> {
        info!("Checking bill recourse payment for {bill_id}");
        let bill_keys = self.store.get_keys(bill_id).await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        if let Ok(RecourseWaitingForPayment::Yes(payment_info)) =
            chain.is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, now)
        {
            // calculate payment address
            let payment_address = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &payment_info.recourser.node_id)?;
            // check if paid
            if let Ok((paid, sum)) = self
                .bitcoin_client
                .check_if_paid(&payment_address, payment_info.sum)
                .await
            {
                if paid && sum > 0 {
                    // If we are the recourser and a bill issuer and it's paid, we add a Recourse block
                    if payment_info.recourser.node_id == identity.identity.node_id {
                        if let Some(signer_identity) =
                            IdentityPublicData::new(identity.identity.clone())
                        {
                            let chain = self
                                .recourse_bitcredit_bill(
                                    bill_id,
                                    self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                        payment_info.recoursee.clone(),
                                        &identity.identity,
                                    )
                                    .await,
                                    payment_info.sum,
                                    &payment_info.currency,
                                    &signer_identity,
                                    &identity.key_pair,
                                    now,
                                )
                                .await?;

                            if let Err(e) = self
                                .propagate_block(bill_id, chain.get_latest_block())
                                .await
                            {
                                error!("Error propagating block: {e}");
                            }

                            if let Err(e) = self
                                .propagate_bill_for_node(bill_id, &payment_info.recoursee.node_id)
                                .await
                            {
                                error!("Error propagating bill for node on DHT: {e}");
                            }
                        }
                        return Ok(()); // return early
                    }

                    let local_companies: HashMap<String, (Company, CompanyKeys)> =
                        self.company_store.get_all().await?;
                    // If a local company is the recourser, create the recourse block as that company
                    if let Some(recourser_company) =
                        local_companies.get(&payment_info.recourser.node_id)
                    {
                        if recourser_company
                            .0
                            .signatories
                            .iter()
                            .any(|s| s == &identity.identity.node_id)
                        {
                            let chain = self
                                .recourse_bitcredit_bill(
                                    bill_id,
                                    self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                        payment_info.recoursee.clone(),
                                        &identity.identity,
                                    )
                                    .await,
                                    payment_info.sum,
                                    &payment_info.currency,
                                    // signer identity (company)
                                    &IdentityPublicData::from(recourser_company.0.clone()),
                                    // signer keys (company keys)
                                    &BcrKeys::from_private_key(&recourser_company.1.private_key)?,
                                    now,
                                )
                                .await?;

                            if let Err(e) = self
                                .propagate_block(bill_id, chain.get_latest_block())
                                .await
                            {
                                error!("Error propagating block: {e}");
                            }

                            if let Err(e) = self
                                .propagate_bill_for_node(bill_id, &payment_info.recoursee.node_id)
                                .await
                            {
                                error!("Error propagating bill for node on DHT: {e}");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_bill_offer_to_sell_payment(
        &self,
        bill_id: &str,
        identity: &IdentityWithAll,
        now: u64,
    ) -> Result<()> {
        info!("Checking bill offer to sell payment for {bill_id}");
        let bill_keys = self.store.get_keys(bill_id).await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        if let Ok(OfferToSellWaitingForPayment::Yes(payment_info)) =
            chain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, now)
        {
            // check if paid
            if let Ok((paid, sum)) = self
                .bitcoin_client
                .check_if_paid(&payment_info.payment_address, payment_info.sum)
                .await
            {
                if paid && sum > 0 {
                    // If we are the seller and a bill issuer and it's paid, we add a Sell block
                    if payment_info.seller.node_id == identity.identity.node_id {
                        if let Some(signer_identity) =
                            IdentityPublicData::new(identity.identity.clone())
                        {
                            let chain = self
                                .sell_bitcredit_bill(
                                    bill_id,
                                    self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                        payment_info.buyer.clone(),
                                        &identity.identity,
                                    )
                                    .await,
                                    payment_info.sum,
                                    &payment_info.currency,
                                    &payment_info.payment_address,
                                    &signer_identity,
                                    &identity.key_pair,
                                    now,
                                )
                                .await?;

                            if let Err(e) = self
                                .propagate_block(bill_id, chain.get_latest_block())
                                .await
                            {
                                error!("Error propagating block: {e}");
                            }

                            if let Err(e) = self
                                .propagate_bill_for_node(bill_id, &payment_info.buyer.node_id)
                                .await
                            {
                                error!("Error propagating bill for node on DHT: {e}");
                            }
                        }
                        return Ok(()); // return early
                    }

                    let local_companies: HashMap<String, (Company, CompanyKeys)> =
                        self.company_store.get_all().await?;
                    // If a local company is the seller, create the sell block as that company
                    if let Some(seller_company) = local_companies.get(&payment_info.seller.node_id)
                    {
                        if seller_company
                            .0
                            .signatories
                            .iter()
                            .any(|s| s == &identity.identity.node_id)
                        {
                            let chain = self
                                .sell_bitcredit_bill(
                                    bill_id,
                                    self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                        payment_info.buyer.clone(),
                                        &identity.identity,
                                    )
                                    .await,
                                    payment_info.sum,
                                    &payment_info.currency,
                                    &payment_info.payment_address,
                                    // signer identity (company)
                                    &IdentityPublicData::from(seller_company.0.clone()),
                                    // signer keys (company keys)
                                    &BcrKeys::from_private_key(&seller_company.1.private_key)?,
                                    now,
                                )
                                .await?;

                            if let Err(e) = self
                                .propagate_block(bill_id, chain.get_latest_block())
                                .await
                            {
                                error!("Error propagating block: {e}");
                            }

                            if let Err(e) = self
                                .propagate_bill_for_node(bill_id, &payment_info.buyer.node_id)
                                .await
                            {
                                error!("Error propagating bill for node on DHT: {e}");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_bill_timeouts(&self, bill_id: &str, now: u64) -> Result<()> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let latest_ts = chain.get_latest_block().timestamp;

        if let Some(action) = match chain.get_latest_block().op_code {
            BillOpCode::RequestToPay | BillOpCode::OfferToSell
                if (latest_ts + PAYMENT_DEADLINE_SECONDS <= now) =>
            {
                Some(ActionType::PayBill)
            }
            BillOpCode::RequestToAccept if (latest_ts + ACCEPT_DEADLINE_SECONDS <= now) => {
                Some(ActionType::AcceptBill)
            }
            BillOpCode::RequestRecourse if (latest_ts + RECOURSE_DEADLINE_SECONDS <= now) => {
                Some(ActionType::RecourseBill)
            }
            _ => None,
        } {
            // did we already send the notification
            let sent = self
                .notification_service
                .check_bill_notification_sent(
                    bill_id,
                    chain.block_height() as i32,
                    action.to_owned(),
                )
                .await?;

            if !sent {
                let identity = self.identity_store.get().await?;
                let current_identity = IdentityPublicData::new(identity.clone());
                let participants = chain.get_all_nodes_from_bill(&bill_keys)?;
                let mut recipient_options = vec![current_identity];
                let bill = self
                    .get_last_version_bill(&chain, &bill_keys, &identity)
                    .await?;

                for node_id in participants {
                    let contact: Option<IdentityPublicData> =
                        self.contact_store.get(&node_id).await?.map(|c| c.into());
                    recipient_options.push(contact);
                }

                let recipients = recipient_options
                    .into_iter()
                    .flatten()
                    .collect::<Vec<IdentityPublicData>>();

                self.notification_service
                    .send_request_to_action_timed_out_event(
                        bill_id,
                        Some(bill.sum),
                        action.to_owned(),
                        recipients,
                    )
                    .await?;

                // remember we have sent the notification
                self.notification_service
                    .mark_bill_notification_sent(bill_id, chain.block_height() as i32, action)
                    .await?;
            }
        }
        Ok(())
    }

    fn get_past_endorsees_for_bill(
        &self,
        chain: &BillBlockchain,
        bill_keys: &BillKeys,
        current_identity_node_id: &str,
    ) -> Result<Vec<PastEndorsee>> {
        let mut result: HashMap<String, PastEndorsee> = HashMap::new();

        let mut found_last_endorsing_block_for_node = false;
        for block in chain.blocks().iter().rev() {
            // we ignore recourse blocks, since we're only interested in previous endorsees before
            // recourse
            if block.op_code == BillOpCode::Recourse {
                continue;
            }
            if let Ok(Some(holder_from_block)) = block.get_holder_from_block(bill_keys) {
                // first, we search for the last non-recourse block in which we became holder
                if holder_from_block.holder.node_id == *current_identity_node_id
                    && !found_last_endorsing_block_for_node
                {
                    found_last_endorsing_block_for_node = true;
                }

                // we add the holders after ourselves, if they're not in the list already
                if found_last_endorsing_block_for_node
                    && holder_from_block.holder.node_id != *current_identity_node_id
                {
                    result
                        .entry(holder_from_block.holder.node_id.clone())
                        .or_insert(PastEndorsee {
                            pay_to_the_order_of: holder_from_block.holder.clone().into(),
                            signed: LightSignedBy {
                                data: holder_from_block.signer.clone().into(),
                                signatory: holder_from_block.signatory.map(|s| {
                                    LightIdentityPublicData {
                                        t: ContactType::Person,
                                        name: s.name,
                                        node_id: s.node_id,
                                    }
                                }),
                            },
                            signing_timestamp: block.timestamp,
                            signing_address: holder_from_block.signer.postal_address,
                        });
                }
            }
        }

        let first_version_bill = chain.get_first_version_bill(bill_keys)?;
        // If the drawer is not the drawee, the drawer is the first holder, if the drawer is the
        // payee, they are already in the list
        if first_version_bill.drawer.node_id != first_version_bill.drawee.node_id {
            result
                .entry(first_version_bill.drawer.node_id.clone())
                .or_insert(PastEndorsee {
                    pay_to_the_order_of: first_version_bill.drawer.clone().into(),
                    signed: LightSignedBy {
                        data: first_version_bill.drawer.clone().into(),
                        signatory: first_version_bill
                            .signatory
                            .map(|s| LightIdentityPublicData {
                                t: ContactType::Person,
                                name: s.name,
                                node_id: s.node_id,
                            }),
                    },
                    signing_timestamp: first_version_bill.signing_timestamp,
                    signing_address: first_version_bill.drawer.postal_address,
                });
        }

        // remove ourselves from the list
        result.remove(current_identity_node_id);

        // sort by signing timestamp descending
        let mut list: Vec<PastEndorsee> = result.into_values().collect();
        list.sort_by(|a, b| b.signing_timestamp.cmp(&a.signing_timestamp));

        Ok(list)
    }

    /// Implementation of reject actions for bills, with individual validation rules, blocks and
    /// notifications
    async fn reject(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
        op: BillOpCode,
    ) -> Result<BillBlockchain> {
        match op {
            BillOpCode::RejectToAccept
            | BillOpCode::RejectToBuy
            | BillOpCode::RejectToPayRecourse
            | BillOpCode::RejectToPay => (),
            _ => return Err(Error::InvalidOperation),
        };
        // data fetching
        let identity = self.identity_store.get_full().await?;
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;
        let signer_node_id = signer_public_data.node_id.clone();
        let waiting_for_payment =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?;
        let waiting_for_recourse = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?;
        let last_block = blockchain.get_latest_block();

        // validation
        // If the operation was already rejected, we can't reject again
        if op == *last_block.op_code() {
            return Err(Error::RequestAlreadyRejected);
        }
        match op {
            BillOpCode::RejectToAccept => {
                // not waiting for last offer to sell
                if let OfferToSellWaitingForPayment::Yes(_) = waiting_for_payment {
                    return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
                }
                // not in recourse
                if let RecourseWaitingForPayment::Yes(_) = waiting_for_recourse {
                    return Err(Error::BillIsInRecourseAndWaitingForPayment);
                }
                // caller has to be the drawee
                if signer_node_id != bill.drawee.node_id {
                    return Err(Error::CallerIsNotDrawee);
                }
                // there is not allowed to be an accept block
                if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                    return Err(Error::BillAlreadyAccepted);
                }
            }
            BillOpCode::RejectToBuy => {
                if let RecourseWaitingForPayment::Yes(_) = waiting_for_recourse {
                    return Err(Error::BillIsInRecourseAndWaitingForPayment);
                }
                // there has to be a offer to sell block that is not expired
                if let OfferToSellWaitingForPayment::Yes(payment_info) = waiting_for_payment {
                    // caller has to be buyer of the offer to sell
                    if signer_node_id != payment_info.buyer.node_id {
                        return Err(Error::CallerIsNotBuyer);
                    }
                } else {
                    return Err(Error::BillWasNotOfferedToSell);
                }
            }
            BillOpCode::RejectToPay => {
                if let RecourseWaitingForPayment::Yes(_) = waiting_for_recourse {
                    return Err(Error::BillIsInRecourseAndWaitingForPayment);
                }
                // not waiting for last offer to sell
                if let OfferToSellWaitingForPayment::Yes(_) = waiting_for_payment {
                    return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
                }
                // caller has to be the drawee
                if signer_node_id != bill.drawee.node_id {
                    return Err(Error::CallerIsNotDrawee);
                }
                // bill is not paid already
                if let Ok(true) = self.store.is_paid(bill_id).await {
                    return Err(Error::BillAlreadyPaid);
                }
                // there has to be a request to pay block that is not expired
                if let Some(req_to_pay) =
                    blockchain.get_last_version_block_with_op_code(BillOpCode::RequestToPay)
                {
                    if req_to_pay.timestamp + PAYMENT_DEADLINE_SECONDS < timestamp {
                        return Err(Error::RequestAlreadyExpired);
                    }
                } else {
                    return Err(Error::BillWasNotRequestedToPay);
                }
            }
            BillOpCode::RejectToPayRecourse => {
                // not waiting for last offer to sell
                if let OfferToSellWaitingForPayment::Yes(_) = waiting_for_payment {
                    return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
                }
                // there has to be a request to recourse that is not expired
                if let Some(req_to_recourse) =
                    blockchain.get_last_version_block_with_op_code(BillOpCode::RequestRecourse)
                {
                    // has to be the last block
                    if blockchain.get_latest_block().id != req_to_recourse.id {
                        return Err(Error::BillWasNotRequestedToRecourse);
                    }
                    if req_to_recourse.timestamp + RECOURSE_DEADLINE_SECONDS < timestamp {
                        return Err(Error::RequestAlreadyExpired);
                    }
                    // caller has to be recoursee of the request to recourse block
                    let block_data: BillRequestRecourseBlockData =
                        req_to_recourse.get_decrypted_block_bytes(&bill_keys)?;
                    if signer_node_id != block_data.recoursee.node_id {
                        return Err(Error::CallerIsNotRecoursee);
                    }
                } else {
                    return Err(Error::BillWasNotRequestedToRecourse);
                }
            }
            _ => return Err(Error::InvalidOperation),
        };

        // block creation
        let signing_keys = self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
        let previous_block = blockchain.get_latest_block();
        let block_data = BillRejectBlockData {
            rejecter: signer_public_data.clone().into(),
            signatory: signing_keys.signatory_identity,
            signing_timestamp: timestamp,
            signing_address: signer_public_data.postal_address.clone(),
        };
        let block = match op {
            BillOpCode::RejectToAccept => BillBlock::create_block_for_reject_to_accept(
                bill_id.to_owned(),
                previous_block,
                &block_data,
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?,
            BillOpCode::RejectToBuy => BillBlock::create_block_for_reject_to_buy(
                bill_id.to_owned(),
                previous_block,
                &block_data,
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?,
            BillOpCode::RejectToPay => BillBlock::create_block_for_reject_to_pay(
                bill_id.to_owned(),
                previous_block,
                &block_data,
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?,
            BillOpCode::RejectToPayRecourse => BillBlock::create_block_for_reject_to_pay_recourse(
                bill_id.to_owned(),
                previous_block,
                &block_data,
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?,
            _ => return Err(Error::InvalidOperation),
        };

        self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
            .await?;

        self.add_identity_and_company_chain_blocks_for_signed_bill_action(
            signer_public_data,
            bill_id,
            &block,
            &identity.key_pair,
            signer_keys,
            timestamp,
        )
        .await?;

        // notifications
        let mut recipients = vec![];
        if let Some(self_identity) = IdentityPublicData::new(identity.identity) {
            recipients.push(self_identity);
        }
        for node_id in blockchain.get_all_nodes_from_bill(&bill_keys)? {
            if let Some(contact) = self.contact_store.get(&node_id).await?.map(|c| c.into()) {
                recipients.push(contact);
            }
        }

        match op {
            BillOpCode::RejectToAccept => {
                self.notification_service
                    .send_request_to_action_rejected_event(
                        bill_id,
                        Some(bill.sum),
                        ActionType::AcceptBill,
                        recipients,
                    )
                    .await?;
            }
            BillOpCode::RejectToBuy => {
                self.notification_service
                    .send_request_to_action_rejected_event(
                        bill_id,
                        Some(bill.sum),
                        ActionType::BuyBill,
                        recipients,
                    )
                    .await?;
            }
            BillOpCode::RejectToPay => {
                self.notification_service
                    .send_request_to_action_rejected_event(
                        bill_id,
                        Some(bill.sum),
                        ActionType::PayBill,
                        recipients,
                    )
                    .await?;
            }
            BillOpCode::RejectToPayRecourse => {
                self.notification_service
                    .send_request_to_action_rejected_event(
                        bill_id,
                        Some(bill.sum),
                        ActionType::RecourseBill,
                        recipients,
                    )
                    .await?;
            }
            _ => (),
        };

        Ok(blockchain)
    }
}

#[async_trait]
impl BillServiceApi for BillService {
    async fn get_bill_balances(
        &self,
        _currency: &str,
        current_identity_node_id: &str,
    ) -> Result<BillsBalanceOverview> {
        let bills = self.get_bills(current_identity_node_id).await?;

        let mut payer_sum = 0;
        let mut payee_sum = 0;
        let mut contingent_sum = 0;

        for bill in bills {
            if let Ok(sum) = util::currency::parse_sum(&bill.sum) {
                if let Some(bill_role) = bill.get_bill_role_for_node_id(current_identity_node_id) {
                    match bill_role {
                        BillRole::Payee => payee_sum += sum,
                        BillRole::Payer => payer_sum += sum,
                        BillRole::Contingent => contingent_sum += sum,
                    };
                }
            }
        }

        Ok(BillsBalanceOverview {
            payee: BillsBalance {
                sum: util::currency::sum_to_string(payee_sum),
            },
            payer: BillsBalance {
                sum: util::currency::sum_to_string(payer_sum),
            },
            contingent: BillsBalance {
                sum: util::currency::sum_to_string(contingent_sum),
            },
        })
    }

    async fn search_bills(
        &self,
        _currency: &str,
        search_term: &Option<String>,
        date_range_from: Option<u64>,
        date_range_to: Option<u64>,
        role: &BillsFilterRole,
        current_identity_node_id: &str,
    ) -> Result<Vec<LightBitcreditBillToReturn>> {
        let bills = self.get_bills(current_identity_node_id).await?;
        let mut result = vec![];

        // for now we do the search here - with the quick-fetch table, we can search in surrealDB
        // directly
        for bill in bills {
            // if the bill wasn't issued between from and to, we kick them out
            if let Some(issue_date_ts) =
                util::date::date_string_to_i64_timestamp(&bill.issue_date, None)
            {
                if let Some(from) = date_range_from {
                    if from > issue_date_ts as u64 {
                        continue;
                    }
                }
                if let Some(to) = date_range_to {
                    if to < issue_date_ts as u64 {
                        continue;
                    }
                }
            }

            let bill_role = match bill.get_bill_role_for_node_id(current_identity_node_id) {
                Some(bill_role) => bill_role,
                None => continue, // node is not in bill - don't add
            };

            match role {
                BillsFilterRole::All => (), // we take all
                BillsFilterRole::Payer => {
                    if bill_role != BillRole::Payer {
                        // payer selected, but node not payer
                        continue;
                    }
                }
                BillsFilterRole::Payee => {
                    if bill_role != BillRole::Payee {
                        // payee selected, but node not payee
                        continue;
                    }
                }
                BillsFilterRole::Contingent => {
                    if bill_role != BillRole::Contingent {
                        // contingent selected, but node not
                        // contingent
                        continue;
                    }
                }
            };

            if let Some(st) = search_term {
                if !bill.search_bill_for_search_term(st) {
                    continue;
                }
            }

            result.push(bill.into());
        }

        Ok(result)
    }

    async fn get_bills_from_all_identities(&self) -> Result<Vec<BitcreditBillToReturn>> {
        let bill_ids = self.store.get_ids().await?;
        let identity = self.identity_store.get().await?;
        let current_timestamp = util::date::now().timestamp() as u64;

        let tasks = bill_ids.iter().map(|id| {
            let identity_clone = identity.clone();
            async move {
                self.get_full_bill(
                    id,
                    &identity_clone,
                    &identity_clone.node_id,
                    current_timestamp,
                )
                .await
            }
        });
        let bills = try_join_all(tasks).await?;

        Ok(bills)
    }

    async fn get_bills(
        &self,
        current_identity_node_id: &str,
    ) -> Result<Vec<BitcreditBillToReturn>> {
        let bill_ids = self.store.get_ids().await?;
        let identity = self.identity_store.get().await?;
        let current_timestamp = util::date::now().timestamp() as u64;

        let tasks = bill_ids.iter().map(|id| {
            let identity_clone = identity.clone();
            async move {
                self.get_full_bill(
                    id,
                    &identity_clone,
                    current_identity_node_id,
                    current_timestamp,
                )
                .await
            }
        });
        let bills = try_join_all(tasks).await?;

        Ok(bills
            .into_iter()
            .filter(|b| {
                b.bill_participants
                    .iter()
                    .any(|p| p == current_identity_node_id)
            })
            .collect())
    }

    async fn get_combined_bitcoin_key_for_bill(
        &self,
        bill_id: &str,
        caller_public_data: &IdentityPublicData,
        caller_keys: &BcrKeys,
    ) -> Result<BillCombinedBitcoinKey> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        // if caller is not part of the bill, they can't access it
        if !chain
            .get_all_nodes_from_bill(&bill_keys)?
            .iter()
            .any(|p| p == &caller_public_data.node_id)
        {
            return Err(Error::NotFound);
        }

        let private_key = self.bitcoin_client.get_combined_private_key(
            &caller_keys.get_bitcoin_private_key(CONFIG.bitcoin_network()),
            &BcrKeys::from_private_key(&bill_keys.private_key)?
                .get_bitcoin_private_key(CONFIG.bitcoin_network()),
        )?;
        return Ok(BillCombinedBitcoinKey { private_key });
    }

    async fn get_detail(
        &self,
        bill_id: &str,
        identity: &Identity,
        current_identity_node_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillToReturn> {
        if !self.store.exists(bill_id).await {
            return Err(Error::NotFound);
        }
        let res = self
            .get_full_bill(
                bill_id,
                identity,
                current_identity_node_id,
                current_timestamp,
            )
            .await?;
        // if currently active identity is not part of the bill, we can't access it
        if !res
            .bill_participants
            .iter()
            .any(|p| p == current_identity_node_id)
        {
            return Err(Error::NotFound);
        }
        Ok(res)
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

    async fn find_and_sync_with_bill_in_dht(&self, bill_id: &str) -> Result<()> {
        if !self.store.exists(bill_id).await {
            return Err(Error::NotFound);
        }
        let mut dht_client = self.client.clone();
        dht_client.receive_updates_for_bill_topic(bill_id).await?;
        Ok(())
    }

    async fn get_bill_keys(&self, bill_id: &str) -> Result<BillKeys> {
        if !self.store.exists(bill_id).await {
            return Err(Error::NotFound);
        }
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
        country_of_issuing: String,
        city_of_issuing: String,
        issue_date: String,
        maturity_date: String,
        drawee: IdentityPublicData,
        payee: IdentityPublicData,
        sum: u64,
        currency: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        file_upload_id: Option<String>,
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
        timestamp: u64,
    ) -> Result<BitcreditBill> {
        let identity = self.identity_store.get_full().await?;
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

        let mut bill_files: Vec<File> = vec![];
        if let Some(ref upload_id) = file_upload_id {
            let files = self
                .file_upload_store
                .read_temp_upload_files(upload_id)
                .await
                .map_err(|_| Error::NoFileForFileUploadId)?;
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
            country_of_issuing,
            city_of_issuing,
            currency,
            sum,
            maturity_date,
            issue_date,
            country_of_payment,
            city_of_payment,
            language,
            drawee,
            drawer: drawer_public_data.clone(),
            payee,
            endorsee: None,
            files: bill_files,
        };

        let signing_keys = self.get_bill_signing_keys(&drawer_public_data, &drawer_keys, &identity);
        let chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill.clone(), signing_keys.signatory_identity, timestamp),
            signing_keys.signatory_keys,
            signing_keys.company_keys,
            keys.clone(),
            timestamp,
        )?;

        let block = chain.get_first_block();
        self.blockchain_store.add_block(&bill.id, block).await?;

        self.add_identity_and_company_chain_blocks_for_signed_bill_action(
            &drawer_public_data,
            &bill_id,
            block,
            &identity.key_pair,
            &drawer_keys,
            timestamp,
        )
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

    async fn accept_bill(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        let accepted = blockchain.block_with_operation_code_exists(BillOpCode::Accept);

        if accepted {
            return Err(Error::BillAlreadyAccepted);
        }

        if !bill.drawee.node_id.eq(&signer_public_data.node_id) {
            return Err(Error::CallerIsNotDrawee);
        }

        let signing_keys = self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
        let previous_block = blockchain.get_latest_block();
        let block = BillBlock::create_block_for_accept(
            bill_id.to_owned(),
            previous_block,
            &BillAcceptBlockData {
                accepter: signer_public_data.clone().into(),
                signatory: signing_keys.signatory_identity,
                signing_timestamp: timestamp,
                signing_address: signer_public_data.postal_address.clone(),
            },
            &signing_keys.signatory_keys,
            signing_keys.company_keys.as_ref(), // company keys
            &BcrKeys::from_private_key(&bill_keys.private_key)?,
            timestamp,
        )?;
        self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
            .await?;

        self.add_identity_and_company_chain_blocks_for_signed_bill_action(
            signer_public_data,
            bill_id,
            &block,
            &identity.key_pair,
            signer_keys,
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
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (signer_public_data.node_id.eq(&bill.payee.node_id)
            && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(signer_public_data.node_id.clone()).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_request_to_pay(
                bill_id.to_owned(),
                previous_block,
                &BillRequestToPayBlockData {
                    requester: signer_public_data.clone().into(),
                    currency: currency.to_owned(),
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
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
        Err(Error::CallerIsNotHolder)
    }

    async fn request_acceptance(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (signer_public_data.node_id.eq(&bill.payee.node_id)
            && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(signer_public_data.clone().node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_request_to_accept(
                bill_id.to_owned(),
                previous_block,
                &BillRequestToAcceptBlockData {
                    requester: signer_public_data.clone().into(),
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
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
        Err(Error::CallerIsNotHolder)
    }

    async fn request_recourse(
        &self,
        bill_id: &str,
        recoursee: &IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        recourse_reason: RecourseReason,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        // data fetching
        let identity = self.identity_store.get_full().await?;
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let past_holders =
            self.get_past_endorsees_for_bill(&blockchain, &bill_keys, &signer_public_data.node_id)?;

        // validation
        if !past_holders
            .iter()
            .any(|h| h.pay_to_the_order_of.node_id == recoursee.node_id)
        {
            return Err(Error::RecourseeNotPastHolder);
        }

        // if the bill is offered for selling and waiting, we have to wait
        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        // if the bill is currently in recourse and the recourse request has not expired
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        let holder_node_id = match bill.endorsee {
            None => &bill.payee.node_id,
            Some(ref endorsee) => &endorsee.node_id,
        };

        // the caller has to be the bill holder
        if signer_public_data.node_id != *holder_node_id {
            return Err(Error::CallerIsNotHolder);
        }

        match recourse_reason {
            RecourseReason::Accept => {
                if let Some(req_to_accept) =
                    blockchain.get_last_version_block_with_op_code(BillOpCode::RejectToAccept)
                {
                    // only if the request to accept expired or was rejected
                    if (req_to_accept.timestamp + ACCEPT_DEADLINE_SECONDS >= timestamp)
                        && !blockchain.block_with_operation_code_exists(BillOpCode::RejectToAccept)
                    {
                        return Err(Error::BillRequestToAcceptDidNotExpireAndWasNotRejected);
                    }
                } else {
                    return Err(Error::BillWasNotRequestedToAccept);
                }
            }
            RecourseReason::Pay(_, _) => {
                if let Some(req_to_pay) =
                    blockchain.get_last_version_block_with_op_code(BillOpCode::RejectToPay)
                {
                    // only if the bill is not paid already
                    if let Ok(true) = self.store.is_paid(bill_id).await {
                        return Err(Error::BillAlreadyPaid);
                    }
                    // only if the request to pay expired or was rejected
                    if (req_to_pay.timestamp + PAYMENT_DEADLINE_SECONDS >= timestamp)
                        && !blockchain.block_with_operation_code_exists(BillOpCode::RejectToPay)
                    {
                        return Err(Error::BillRequestToPayDidNotExpireAndWasNotRejected);
                    }
                } else {
                    return Err(Error::BillWasNotRequestedToPay);
                }
            }
        };

        let (sum, currency) = match recourse_reason {
            RecourseReason::Accept => (bill.sum, bill.currency.clone()),
            RecourseReason::Pay(sum, ref currency) => (sum, currency.to_owned()),
        };

        // block creation
        let signing_keys = self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
        let previous_block = blockchain.get_latest_block();
        let block = BillBlock::create_block_for_request_recourse(
            bill_id.to_owned(),
            previous_block,
            &BillRequestRecourseBlockData {
                recourser: signer_public_data.clone().into(),
                recoursee: recoursee.clone().into(),
                sum,
                currency: currency.to_owned(),
                signatory: signing_keys.signatory_identity,
                signing_timestamp: timestamp,
                signing_address: signer_public_data.postal_address.clone(),
            },
            &signing_keys.signatory_keys,
            signing_keys.company_keys.as_ref(),
            &BcrKeys::from_private_key(&bill_keys.private_key)?,
            timestamp,
        )?;
        self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
            .await?;

        self.add_identity_and_company_chain_blocks_for_signed_bill_action(
            signer_public_data,
            bill_id,
            &block,
            &identity.key_pair,
            signer_keys,
            timestamp,
        )
        .await?;

        let action_type = match recourse_reason {
            RecourseReason::Accept => ActionType::AcceptBill,
            RecourseReason::Pay(_, _) => ActionType::PayBill,
        };
        self.notification_service
            .send_recourse_action_event(bill_id, Some(sum), action_type, recoursee)
            .await?;

        Ok(blockchain)
    }

    async fn recourse_bitcredit_bill(
        &self,
        bill_id: &str,
        recoursee: IdentityPublicData,
        sum: u64,
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if let RecourseWaitingForPayment::Yes(payment_info) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            if payment_info.sum != sum
                || payment_info.currency != currency
                || payment_info.recoursee.node_id != recoursee.node_id
                || payment_info.recourser.node_id != signer_public_data.node_id
            {
                return Err(Error::BillRecourseDataInvalid);
            }

            let holder_node_id = match bill.endorsee {
                None => &bill.payee.node_id,
                Some(ref endorsee) => &endorsee.node_id,
            };

            // the caller has to be the bill holder
            if signer_public_data.node_id != *holder_node_id {
                return Err(Error::CallerIsNotHolder);
            }

            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_recourse(
                bill_id.to_owned(),
                previous_block,
                &BillRecourseBlockData {
                    recourser: signer_public_data.clone().into(),
                    recoursee: recoursee.clone().into(),
                    sum,
                    currency: currency.to_owned(),
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_bill_recourse_paid_event(bill_id, Some(payment_info.sum), &recoursee)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::BillIsNotRequestedToRecourseAndWaitingForPayment)
    }

    async fn mint_bitcredit_bill(
        &self,
        bill_id: &str,
        sum: u64,
        currency: &str,
        mintnode: IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (signer_public_data.node_id.eq(&bill.payee.node_id)
            && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(signer_public_data.clone().node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_mint(
                bill_id.to_owned(),
                previous_block,
                &BillMintBlockData {
                    endorser: signer_public_data.clone().into(),
                    endorsee: mintnode.into(),
                    currency: currency.to_owned(),
                    sum,
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
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
        Err(Error::CallerIsNotHolder)
    }

    async fn offer_to_sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        sum: u64,
        currency: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (signer_public_data.node_id.eq(&bill.payee.node_id)
            && !blockchain.has_been_endorsed_or_sold())
            || (Some(signer_public_data.clone().node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            // The address to pay is the seller's address combined with the bill's address
            let address_to_pay = self
                .bitcoin_client
                .get_address_to_pay(&bill_keys.public_key, &signer_public_data.node_id)?;
            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_offer_to_sell(
                bill_id.to_owned(),
                previous_block,
                &BillOfferToSellBlockData {
                    seller: signer_public_data.clone().into(),
                    buyer: buyer.clone().into(),
                    currency: currency.to_owned(),
                    sum,
                    payment_address: address_to_pay,
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
                timestamp,
            )
            .await?;

            self.notification_service
                .send_offer_to_sell_event(bill_id, Some(sum), &buyer)
                .await?;

            return Ok(blockchain);
        }
        Err(Error::CallerIsNotHolder)
    }

    async fn sell_bitcredit_bill(
        &self,
        bill_id: &str,
        buyer: IdentityPublicData,
        sum: u64,
        currency: &str,
        payment_address: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        if let Ok(OfferToSellWaitingForPayment::Yes(payment_info)) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)
        {
            if payment_info.sum != sum
                || payment_info.currency != currency
                || payment_info.payment_address != payment_address
                || payment_info.buyer.node_id != buyer.node_id
                || payment_info.seller.node_id != signer_public_data.node_id
            {
                return Err(Error::BillSellDataInvalid);
            }

            if (signer_public_data.node_id.eq(&bill.payee.node_id)
                && !blockchain.has_been_endorsed_or_sold())
                || (Some(signer_public_data.clone().node_id).eq(&bill.endorsee.map(|e| e.node_id)))
            {
                let signing_keys =
                    self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
                let previous_block = blockchain.get_latest_block();
                let block = BillBlock::create_block_for_sell(
                    bill_id.to_owned(),
                    previous_block,
                    &BillSellBlockData {
                        seller: signer_public_data.clone().into(),
                        buyer: buyer.clone().into(),
                        currency: currency.to_owned(),
                        sum,
                        payment_address: payment_address.to_owned(),
                        signatory: signing_keys.signatory_identity,
                        signing_timestamp: timestamp,
                        signing_address: signer_public_data.postal_address.clone(),
                    },
                    &signing_keys.signatory_keys,
                    signing_keys.company_keys.as_ref(),
                    &BcrKeys::from_private_key(&bill_keys.private_key)?,
                    timestamp,
                )?;
                self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                    .await?;

                self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                    signer_public_data,
                    bill_id,
                    &block,
                    &identity.key_pair,
                    signer_keys,
                    timestamp,
                )
                .await?;

                self.notification_service
                    .send_bill_is_sold_event(bill_id, Some(payment_info.sum), &buyer)
                    .await?;

                return Ok(blockchain);
            } else {
                return Err(Error::CallerIsNotHolder);
            }
        }
        Err(Error::BillIsNotOfferToSellWaitingForPayment)
    }

    async fn endorse_bitcredit_bill(
        &self,
        bill_id: &str,
        endorsee: IdentityPublicData,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        let identity = self.identity_store.get_full().await?;

        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(&bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }

        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        if (signer_public_data.node_id.eq(&bill.payee.node_id)
            && !blockchain.has_been_endorsed_sold_or_minted())
            || (Some(signer_public_data.clone().node_id).eq(&bill.endorsee.map(|e| e.node_id)))
        {
            let signing_keys =
                self.get_bill_signing_keys(signer_public_data, signer_keys, &identity);
            let previous_block = blockchain.get_latest_block();
            let block = BillBlock::create_block_for_endorse(
                bill_id.to_owned(),
                previous_block,
                &BillEndorseBlockData {
                    endorser: signer_public_data.clone().into(),
                    endorsee: endorsee.into(),
                    signatory: signing_keys.signatory_identity,
                    signing_timestamp: timestamp,
                    signing_address: signer_public_data.postal_address.clone(),
                },
                &signing_keys.signatory_keys,
                signing_keys.company_keys.as_ref(),
                &BcrKeys::from_private_key(&bill_keys.private_key)?,
                timestamp,
            )?;
            self.validate_and_add_block(bill_id, &mut blockchain, block.clone())
                .await?;

            self.add_identity_and_company_chain_blocks_for_signed_bill_action(
                signer_public_data,
                bill_id,
                &block,
                &identity.key_pair,
                signer_keys,
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
        Err(Error::CallerIsNotHolder)
    }

    async fn reject_acceptance(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        self.reject(
            bill_id,
            signer_public_data,
            signer_keys,
            timestamp,
            BillOpCode::RejectToAccept,
        )
        .await
    }

    async fn reject_payment(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        self.reject(
            bill_id,
            signer_public_data,
            signer_keys,
            timestamp,
            BillOpCode::RejectToPay,
        )
        .await
    }

    async fn reject_buying(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        self.reject(
            bill_id,
            signer_public_data,
            signer_keys,
            timestamp,
            BillOpCode::RejectToBuy,
        )
        .await
    }

    async fn reject_payment_for_recourse(
        &self,
        bill_id: &str,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        self.reject(
            bill_id,
            signer_public_data,
            signer_keys,
            timestamp,
            BillOpCode::RejectToPayRecourse,
        )
        .await
    }

    async fn check_bills_payment(&self) -> Result<()> {
        let identity = self.identity_store.get().await?;
        let bill_ids_waiting_for_payment = self.store.get_bill_ids_waiting_for_payment().await?;

        for bill_id in bill_ids_waiting_for_payment {
            if let Err(e) = self.check_bill_payment(&bill_id, &identity).await {
                error!("Checking bill payment for {bill_id} failed: {e}");
            }
        }
        Ok(())
    }

    async fn check_bills_offer_to_sell_payment(&self) -> Result<()> {
        let identity = self.identity_store.get_full().await?;
        let bill_ids_waiting_for_offer_to_sell_payment =
            self.store.get_bill_ids_waiting_for_sell_payment().await?;
        let now = external::time::TimeApi::get_atomic_time().await.timestamp;

        for bill_id in bill_ids_waiting_for_offer_to_sell_payment {
            if let Err(e) = self
                .check_bill_offer_to_sell_payment(&bill_id, &identity, now)
                .await
            {
                error!("Checking offer to sell payment for {bill_id} failed: {e}");
            }
        }
        Ok(())
    }

    async fn check_bills_in_recourse_payment(&self) -> Result<()> {
        let identity = self.identity_store.get_full().await?;
        let bill_ids_waiting_for_recourse_payment = self
            .store
            .get_bill_ids_waiting_for_recourse_payment()
            .await?;
        let now = external::time::TimeApi::get_atomic_time().await.timestamp;

        for bill_id in bill_ids_waiting_for_recourse_payment {
            if let Err(e) = self
                .check_bill_in_recourse_payment(&bill_id, &identity, now)
                .await
            {
                error!("Checking recourse payment for {bill_id} failed: {e}");
            }
        }
        Ok(())
    }

    async fn check_bills_timeouts(&self, now: u64) -> Result<()> {
        let op_codes = HashSet::from([
            BillOpCode::RequestToPay,
            BillOpCode::OfferToSell,
            BillOpCode::RequestToAccept,
            BillOpCode::RequestRecourse,
        ]);

        let bill_ids_to_check = self
            .store
            .get_bill_ids_with_op_codes_since(op_codes, 0)
            .await?;

        for bill_id in bill_ids_to_check {
            if let Err(e) = self.check_bill_timeouts(&bill_id, now).await {
                error!("Checking bill timeouts for {bill_id} failed: {e}");
            }
        }

        Ok(())
    }

    async fn get_past_endorsees(
        &self,
        bill_id: &str,
        current_identity_node_id: &str,
    ) -> Result<Vec<PastEndorsee>> {
        if !self.store.exists(bill_id).await {
            return Err(Error::NotFound);
        }

        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        let bill_participants = chain.get_all_nodes_from_bill(&bill_keys)?;
        // active identity is not part of the bill
        if !bill_participants
            .iter()
            .any(|p| p == current_identity_node_id)
        {
            return Err(Error::NotFound);
        }

        self.get_past_endorsees_for_bill(&chain, &bill_keys, current_identity_node_id)
    }

    async fn get_endorsements(
        &self,
        bill_id: &str,
        current_identity_node_id: &str,
    ) -> Result<Vec<Endorsement>> {
        if !self.store.exists(bill_id).await {
            return Err(Error::NotFound);
        }

        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;

        let bill_participants = chain.get_all_nodes_from_bill(&bill_keys)?;
        // active identity is not part of the bill
        if !bill_participants
            .iter()
            .any(|p| p == current_identity_node_id)
        {
            return Err(Error::NotFound);
        }

        let mut result: Vec<Endorsement> = vec![];
        // iterate from the back to the front, collecting all endorsement blocks
        for block in chain.blocks().iter().rev() {
            // we ignore issue blocks, since we are only interested in endorsements
            if block.op_code == BillOpCode::Issue {
                continue;
            }
            if let Ok(Some(holder_from_block)) = block.get_holder_from_block(&bill_keys) {
                result.push(Endorsement {
                    pay_to_the_order_of: holder_from_block.holder.clone().into(),
                    signed: LightSignedBy {
                        data: holder_from_block.signer.clone().into(),
                        signatory: holder_from_block
                            .signatory
                            .map(|s| LightIdentityPublicData {
                                t: ContactType::Person,
                                name: s.name,
                                node_id: s.node_id,
                            }),
                    },
                    signing_timestamp: block.timestamp,
                    signing_address: holder_from_block.signer.postal_address,
                });
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub enum RecourseReason {
    Accept,
    Pay(u64, String), // sum and currency
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct BillsBalanceOverview {
    pub payee: BillsBalance,
    pub payer: BillsBalance,
    pub contingent: BillsBalance,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct BillsBalance {
    pub sum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillRole {
    Payee,
    Payer,
    Contingent,
}

#[derive(Debug, Clone)]
pub struct BillSigningKeys {
    pub signatory_keys: BcrKeys,
    pub company_keys: Option<BcrKeys>,
    pub signatory_identity: Option<BillSignatoryBlockData>,
}

impl From<Identity> for BillSignatoryBlockData {
    fn from(value: Identity) -> Self {
        Self {
            name: value.name,
            node_id: value.node_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct LightBitcreditBillToReturn {
    pub id: String,
    pub drawee: LightIdentityPublicData,
    pub drawer: LightIdentityPublicData,
    pub payee: LightIdentityPublicData,
    pub endorsee: Option<LightIdentityPublicData>,
    pub active_notification: Option<Notification>,
    pub sum: String,
    pub currency: String,
    pub issue_date: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
}

impl From<BitcreditBillToReturn> for LightBitcreditBillToReturn {
    fn from(value: BitcreditBillToReturn) -> Self {
        Self {
            id: value.id,
            drawee: value.drawee.into(),
            drawer: value.drawer.into(),
            payee: value.payee.into(),
            endorsee: value.endorsee.map(|v| v.into()),
            active_notification: value.active_notification,
            sum: value.sum,
            currency: value.currency,
            issue_date: value.issue_date,
            time_of_drawing: value.time_of_drawing,
            time_of_maturity: util::date::date_string_to_i64_timestamp(&value.maturity_date, None)
                .unwrap_or(0) as u64,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct BitcreditBillToReturn {
    pub id: String,
    pub time_of_drawing: u64,
    pub time_of_maturity: u64,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    /// The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    /// The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    /// The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
    pub currency: String,
    pub sum: String,
    pub maturity_date: String,
    pub issue_date: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub accepted: bool,
    pub endorsed: bool,
    pub requested_to_pay: bool,
    pub requested_to_accept: bool,
    pub paid: bool,
    pub waiting_for_payment: bool,
    pub buyer: Option<IdentityPublicData>,
    pub seller: Option<IdentityPublicData>,
    pub in_recourse: bool,
    pub recourser: Option<IdentityPublicData>,
    pub recoursee: Option<IdentityPublicData>,
    pub link_for_buy: String,
    pub link_to_pay: String,
    pub link_to_pay_recourse: String,
    pub address_to_pay: String,
    pub mempool_link_for_address_to_pay: String,
    pub chain_of_blocks: BillBlockchainToReturn,
    pub files: Vec<File>,
    /// The currently active notification for this bill if any
    pub active_notification: Option<Notification>,
    pub bill_participants: Vec<String>,
    pub endorsements_count: u64,
}

impl BitcreditBillToReturn {
    /// Returns the role of the given node_id in the bill, or None if the node_id is not a
    /// participant in the bill
    pub fn get_bill_role_for_node_id(&self, node_id: &str) -> Option<BillRole> {
        // Node id is not part of the bill
        if !self.bill_participants.iter().any(|bp| bp == node_id) {
            return None;
        }

        // Node id is the payer
        if self.drawee.node_id == *node_id {
            return Some(BillRole::Payer);
        }

        // Node id is payee / endorsee
        if self.payee.node_id == *node_id
            || self.endorsee.as_ref().map(|e| e.node_id.as_str()) == Some(node_id)
        {
            return Some(BillRole::Payee);
        }

        // Node id is part of the bill, but neither payer, nor payee - they are part of the risk
        // chain
        Some(BillRole::Contingent)
    }

    // Search in the participants for the search term
    pub fn search_bill_for_search_term(&self, search_term: &str) -> bool {
        let search_term_lc = search_term.to_lowercase();
        if self.payee.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if self.drawer.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if self.drawee.name.to_lowercase().contains(&search_term_lc) {
            return true;
        }

        if let Some(ref endorsee) = self.endorsee {
            if endorsee.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        if let Some(ref buyer) = self.buyer {
            if buyer.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        if let Some(ref seller) = self.seller {
            if seller.name.to_lowercase().contains(&search_term_lc) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
pub struct BitcreditEbillQuote {
    pub bill_id: String,
    pub quote_id: String,
    pub sum: u64,
    pub mint_node_id: String,
    pub mint_url: String,
    pub accepted: bool,
    pub token: String,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
pub struct BitcreditBill {
    pub id: String,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    // The party obliged to pay a Bill
    pub drawee: IdentityPublicData,
    // The party issuing a Bill
    pub drawer: IdentityPublicData,
    pub payee: IdentityPublicData,
    // The person to whom the Payee or an Endorsee endorses a bill
    pub endorsee: Option<IdentityPublicData>,
    pub currency: String,
    pub sum: u64,
    pub maturity_date: String,
    pub issue_date: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub files: Vec<File>,
}

#[cfg(test)]
impl BitcreditBill {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            id: "".to_string(),
            country_of_issuing: "".to_string(),
            city_of_issuing: "".to_string(),
            drawee: IdentityPublicData::new_empty(),
            drawer: IdentityPublicData::new_empty(),
            payee: IdentityPublicData::new_empty(),
            endorsee: None,
            currency: "".to_string(),
            sum: 0,
            maturity_date: "".to_string(),
            issue_date: "".to_string(),
            city_of_payment: "".to_string(),
            country_of_payment: "".to_string(),
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
            company_service::tests::{get_baseline_company_data, get_valid_company_block},
            identity_service::{Identity, IdentityWithAll},
            notification_service::MockNotificationServiceApi,
        },
        tests::tests::{TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP},
        web::data::PostalAddress,
    };
    use blockchain::{bill::block::BillIssueBlockData, identity::IdentityBlockchain};
    use core::str;
    use external::bitcoin::MockBitcoinClientApi;
    use futures::channel::mpsc;
    use mockall::predicate::{always, eq, function};
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
        identity.postal_address.country = Some("AT".to_owned());
        identity.postal_address.city = Some("Vienna".to_owned());
        identity.postal_address.address = Some("Hayekweg 5".to_owned());
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
        bill.drawee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
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
        mock_company_storage: MockCompanyStoreApi,
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
            mock_company_storage,
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
        mock_company_storage: MockCompanyStoreApi,
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
        bitcoin_client
            .expect_get_mempool_link_for_address()
            .returning(|_| {
                String::from(
                    "http://blockstream.info/testnet/address/1Jfn2nZcJ4T7bhE8FdMRz8T3P3YV4LsWn2",
                )
            });
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
            Arc::new(mock_company_storage),
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
        MockCompanyStoreApi,
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
            MockCompanyStoreApi::new(),
        )
    }

    #[tokio::test]
    async fn get_bill_balances_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let company_node_id = BcrKeys::new().get_public_key();

        let mut bill1 = get_baseline_bill("1234");
        bill1.sum = 1000;
        bill1.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let mut bill2 = get_baseline_bill("4321");
        bill2.sum = 2000;
        bill2.drawee = IdentityPublicData::new_only_node_id(company_node_id.clone());
        bill2.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let mut bill3 = get_baseline_bill("9999");
        bill3.sum = 20000;
        bill3.drawer = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        bill3.payee = IdentityPublicData::new_only_node_id(company_node_id.clone());
        bill3.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());

        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_get_ids().returning(|| {
            Ok(vec![
                String::from("1234"),
                String::from("4321"),
                String::from("9999"),
            ])
        });
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "1234")
            .returning(move |_| Ok(get_genesis_chain(Some(bill1.clone()))));
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "4321")
            .returning(move |_| Ok(get_genesis_chain(Some(bill2.clone()))));
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "9999")
            .returning(move |_| Ok(get_genesis_chain(Some(bill3.clone()))));
        let identity_clone = identity.clone();
        identity_storage
            .expect_get()
            .returning(move || Ok(identity_clone.identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        notification_service
            .expect_get_active_bill_notification()
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
            company_storage,
        );
        // for identity
        let res = service
            .get_bill_balances("sat", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().payer.sum, "1000".to_string());
        assert_eq!(res.as_ref().unwrap().payee.sum, "2000".to_string());
        assert_eq!(res.as_ref().unwrap().contingent.sum, "20000".to_string());

        // for company
        let res_comp = service.get_bill_balances("sat", &company_node_id).await;
        assert!(res_comp.is_ok());
        assert_eq!(res_comp.as_ref().unwrap().payer.sum, "2000".to_string());
        assert_eq!(res_comp.as_ref().unwrap().payee.sum, "20000".to_string());
        assert_eq!(res_comp.as_ref().unwrap().contingent.sum, "0".to_string());
    }

    #[tokio::test]
    async fn get_search_bill() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let company_node_id = BcrKeys::new().get_public_key();

        let mut bill1 = get_baseline_bill("1234");
        bill1.issue_date = "2020-05-01".to_string();
        bill1.sum = 1000;
        bill1.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let mut bill2 = get_baseline_bill("4321");
        bill2.issue_date = "2030-05-01".to_string();
        bill2.sum = 2000;
        bill2.drawee = IdentityPublicData::new_only_node_id(company_node_id.clone());
        bill2.payee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        bill2.payee.name = "hayek".to_string();
        let mut bill3 = get_baseline_bill("9999");
        bill3.issue_date = "2030-05-01".to_string();
        bill3.sum = 20000;
        bill3.drawer = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        bill3.payee = IdentityPublicData::new_only_node_id(company_node_id.clone());
        bill3.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());

        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_get_ids().returning(|| {
            Ok(vec![
                String::from("1234"),
                String::from("4321"),
                String::from("9999"),
            ])
        });
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "1234")
            .returning(move |_| Ok(get_genesis_chain(Some(bill1.clone()))));
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "4321")
            .returning(move |_| Ok(get_genesis_chain(Some(bill2.clone()))));
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "9999")
            .returning(move |_| Ok(get_genesis_chain(Some(bill3.clone()))));
        let identity_clone = identity.clone();
        identity_storage
            .expect_get()
            .returning(move || Ok(identity_clone.identity.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        notification_service
            .expect_get_active_bill_notification()
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
            company_storage,
        );
        let res_all_comp = service
            .search_bills(
                "sat",
                &None,
                None,
                None,
                &BillsFilterRole::All,
                &company_node_id,
            )
            .await;
        assert!(res_all_comp.is_ok());
        assert_eq!(res_all_comp.as_ref().unwrap().len(), 2);
        let res_all = service
            .search_bills(
                "sat",
                &None,
                None,
                None,
                &BillsFilterRole::All,
                &identity.identity.node_id,
            )
            .await;
        assert!(res_all.is_ok());
        assert_eq!(res_all.as_ref().unwrap().len(), 3);

        let res_term = service
            .search_bills(
                "sat",
                &Some(String::from("hayek")),
                None,
                None,
                &BillsFilterRole::All,
                &identity.identity.node_id,
            )
            .await;
        assert!(res_term.is_ok());
        assert_eq!(res_term.as_ref().unwrap().len(), 1);

        let from_ts = util::date::date_string_to_i64_timestamp("2030-05-01", None).unwrap();
        let to_ts = util::date::date_string_to_i64_timestamp("2030-05-30", None).unwrap();
        let res_fromto = service
            .search_bills(
                "sat",
                &None,
                Some(from_ts as u64),
                Some(to_ts as u64),
                &BillsFilterRole::All,
                &identity.identity.node_id,
            )
            .await;
        assert!(res_fromto.is_ok());
        assert_eq!(res_fromto.as_ref().unwrap().len(), 2);

        let res_role = service
            .search_bills(
                "sat",
                &None,
                None,
                None,
                &BillsFilterRole::Payer,
                &identity.identity.node_id,
            )
            .await;
        assert!(res_role.is_ok());
        assert_eq!(res_role.as_ref().unwrap().len(), 1);

        let res_comb = service
            .search_bills(
                "sat",
                &Some(String::from("hayek")),
                Some(from_ts as u64),
                Some(to_ts as u64),
                &BillsFilterRole::Payee,
                &identity.identity.node_id,
            )
            .await;
        assert!(res_comb.is_ok());
        assert_eq!(res_comb.as_ref().unwrap().len(), 1);
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
            company_storage,
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
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));

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
            company_storage,
        );

        let drawer = get_baseline_identity();
        let drawee = IdentityPublicData::new_empty();
        let payee = IdentityPublicData::new_empty();

        let bill = service
            .issue_new_bill(
                String::from("UK"),
                String::from("London"),
                String::from("2030-01-01"),
                String::from("2030-04-01"),
                drawee,
                payee,
                100,
                String::from("sat"),
                String::from("AT"),
                String::from("Vienna"),
                String::from("en-UK"),
                Some("1234".to_string()),
                IdentityPublicData::new(drawer.identity).unwrap(),
                drawer.key_pair,
                1731593928,
            )
            .await
            .unwrap();

        assert_eq!(bill.files.first().unwrap().name, expected_file_name);
    }

    #[tokio::test]
    async fn issue_bill_as_company() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            mut file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
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
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));

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
            company_storage,
        );

        let drawer = get_baseline_company_data();
        let drawee = IdentityPublicData::new_empty();
        let payee = IdentityPublicData::new_empty();

        let bill = service
            .issue_new_bill(
                String::from("UK"),
                String::from("London"),
                String::from("2030-01-01"),
                String::from("2030-04-01"),
                drawee,
                payee,
                100,
                String::from("sat"),
                String::from("AT"),
                String::from("Vienna"),
                String::from("en-UK"),
                Some("1234".to_string()),
                IdentityPublicData::from(drawer.1 .0), // public company data
                BcrKeys::from_private_key(&drawer.1 .1.private_key).unwrap(), // company keys
                1731593928,
            )
            .await
            .unwrap();

        assert_eq!(bill.files.first().unwrap().name, expected_file_name);
        assert_eq!(bill.drawer.node_id, drawer.0);
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
            company_storage,
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
            company_storage,
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
            company_storage,
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
            company_storage,
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
            company_storage,
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
            company_storage,
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
            company_storage,
        ) = get_storages();
        storage.expect_exists().returning(|_| true);
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
            company_storage,
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
            company_storage,
        ) = get_storages();
        storage.expect_exists().returning(|_| true);
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
            company_storage,
        );

        assert!(service.get_bill_keys("test").await.is_err());
    }

    #[tokio::test]
    async fn get_bills_from_all_identities_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let company_node_id = BcrKeys::new().get_public_key();
        let mut bill1 = get_baseline_bill("1234");
        bill1.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill1.drawer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill1.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let mut bill2 = get_baseline_bill("5555");
        bill2.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill2.drawer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill2.payee = IdentityPublicData::new_only_node_id(company_node_id.clone());

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
            .withf(|id| id == "1234")
            .returning(move |_| {
                let chain = get_genesis_chain(Some(bill1.clone()));
                Ok(chain)
            });
        chain_storage
            .expect_get_chain()
            .withf(|id| id == "5555")
            .returning(move |_| {
                let chain = get_genesis_chain(Some(bill2.clone()));
                Ok(chain)
            });
        storage
            .expect_get_ids()
            .returning(|| Ok(vec!["1234".to_string(), "5555".to_string()]));
        storage.expect_is_paid().returning(|_| Ok(true));

        notification_service
            .expect_get_active_bill_notification()
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
            company_storage,
        );

        let res_personal = service
            .get_bills(&get_baseline_identity().identity.node_id)
            .await;
        let res_company = service.get_bills(&company_node_id).await;
        let res_both = service.get_bills_from_all_identities().await;
        assert!(res_personal.is_ok());
        assert!(res_company.is_ok());
        assert!(res_both.is_ok());
        assert!(res_personal.as_ref().unwrap().len() == 1);
        assert!(res_company.as_ref().unwrap().len() == 1);
        assert!(res_both.as_ref().unwrap().len() == 2);
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
            company_storage,
        ) = get_storages();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

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
            let chain = get_genesis_chain(Some(bill.clone()));
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
            company_storage,
        );

        let res = service
            .get_bills(&get_baseline_identity().identity.node_id)
            .await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].id, "1234".to_string());
    }

    #[tokio::test]
    async fn get_bills_baseline_company() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let company_node_id = BcrKeys::new().get_public_key();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
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
            company_storage,
        );

        let res = service
            .get_bills(&get_baseline_identity().identity.node_id)
            .await;
        assert!(res.is_ok());
        let returned_bills = res.unwrap();
        assert!(returned_bills.len() == 1);
        assert_eq!(returned_bills[0].id, "some id".to_string());

        let res = service.get_bills(&company_node_id).await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 0);
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
            company_storage,
        ) = get_storages();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

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
                    requester: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    currency: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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
            company_storage,
        );

        let res = service
            .get_bills(&get_baseline_identity().identity.node_id)
            .await;
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
            company_storage,
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
            company_storage,
        );

        let res = service
            .get_bills(&get_baseline_identity().identity.node_id)
            .await;
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_detail_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
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
        storage.expect_exists().returning(|_| true);
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
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
            company_storage,
        );

        let res = service
            .get_detail(
                "some id",
                &identity.identity,
                &identity.identity.node_id,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(!res.as_ref().unwrap().waiting_for_payment);
        assert!(!res.as_ref().unwrap().paid);
    }

    #[tokio::test]
    async fn get_detail_bill_fails_for_non_participant() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_exists().returning(|_| true);
        chain_storage
            .expect_get_chain()
            .returning(move |_| Ok(get_genesis_chain(Some(bill.clone()))));
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
            company_storage,
        );

        let res = service
            .get_detail(
                "some id",
                &identity.identity,
                &BcrKeys::new().get_public_key(),
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_detail_waiting_for_offer_to_sell() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_exists().returning(|_| true);
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
                    seller: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    buyer: IdentityPublicData::new_only_node_id(bill.drawee.node_id.clone()).into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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
            company_storage,
        );

        let res = service
            .get_detail(
                "some id",
                &identity.identity,
                &identity.identity.node_id,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(res.as_ref().unwrap().waiting_for_payment);
    }

    #[tokio::test]
    async fn get_detail_bill_req_to_pay() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let mut notification_service = MockNotificationServiceApi::new();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let drawee_node_id = bill.drawee.node_id.clone();
        storage.expect_exists().returning(|_| true);
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
                    requester: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    currency: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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
            company_storage,
        );

        let res = service
            .get_detail(
                "some id",
                &identity.identity,
                &identity.identity.node_id,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().id, "some id".to_string());
        assert_eq!(res.as_ref().unwrap().drawee.node_id, drawee_node_id);
        assert!(res.as_ref().unwrap().paid);
        assert!(!res.as_ref().unwrap().waiting_for_payment);
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .accept_bill(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.unwrap().blocks()[1].op_code == BillOpCode::Accept);
    }

    #[tokio::test]
    async fn accept_bill_as_company() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let company = get_baseline_company_data();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(company.0.clone());

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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .accept_bill(
                "some id",
                &IdentityPublicData::from(company.1 .0),
                &BcrKeys::from_private_key(&company.1 .1.private_key).unwrap(),
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 2);
        assert!(res.as_ref().unwrap().blocks()[1].op_code == BillOpCode::Accept);
        // company is accepter
        assert!(
            res.as_ref().unwrap().blocks()[1]
                .get_nodes_from_block(&BillKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                })
                .unwrap()[0]
                == company.0
        );
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
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
            company_storage,
        );

        let res = service
            .accept_bill(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let keys = identity.key_pair.clone();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(identity.identity.node_id.clone());
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
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
            company_storage,
        );

        let res = service
            .accept_bill(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .request_pay(
                "some id",
                "sat",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .request_pay(
                "some id",
                "sat",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .request_acceptance(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .request_acceptance(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .mint_bitcredit_bill(
                "some id",
                5000,
                "sat",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .mint_bitcredit_bill(
                "some id",
                5000,
                "sat",
                IdentityPublicData::new_empty(),
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_offer_to_sell_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .offer_to_sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .offer_to_sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
                    currency: "sat".to_owned(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_owned(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                buyer,
                15000,
                "sat",
                "1234paymentaddress",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
                    currency: "sat".to_owned(),
                    sum: 10000, // different sum
                    payment_address: "1234paymentaddress".to_owned(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Sold event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                buyer,
                15000,
                "sat",
                "1234paymentaddress",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to sell event should be sent
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                "1234paymentaddress",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .sell_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                15000,
                "sat",
                "1234paymentaddress",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

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
            company_storage,
        );

        let res = service
            .endorse_bitcredit_bill(
                "some id",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
                    seller: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    buyer: IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key())
                        .into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .endorse_bitcredit_bill(
                "1234",
                IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key()),
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
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
            company_storage,
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
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));
        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .endorse_bitcredit_bill(
                "some id",
                IdentityPublicData::new_empty(),
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_combined_bitcoin_key_for_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
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

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .get_combined_bitcoin_key_for_bill(
                "some id",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
            )
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn get_combined_bitcoin_key_for_bill_err() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();

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

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let non_participant_keys = BcrKeys::new();
        let res = service
            .get_combined_bitcoin_key_for_bill(
                "some id",
                &IdentityPublicData::new_only_node_id(non_participant_keys.get_public_key()),
                &non_participant_keys,
            )
            .await;
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
            company_storage,
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
            company_storage,
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
            company_storage,
        ) = get_storages();

        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

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
                    seller: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    buyer: IdentityPublicData::new_only_node_id(buyer_node_id.clone()).into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service.check_bills_offer_to_sell_payment().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_offer_to_sell_payment_company_is_seller() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            mut company_storage,
        ) = get_storages();

        let mut identity = get_baseline_identity();
        identity.key_pair = BcrKeys::new();
        identity.identity.node_id = identity.key_pair.get_public_key();

        let company = get_baseline_company_data();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::from(company.1 .0.clone());

        storage
            .expect_get_bill_ids_waiting_for_sell_payment()
            .returning(|| Ok(vec!["1234".to_string()]));
        let company_clone = company.clone();
        company_storage.expect_get_all().returning(move || {
            let mut map = HashMap::new();
            map.insert(
                company_clone.0.clone(),
                (company_clone.1 .0.clone(), company_clone.1 .1.clone()),
            );
            Ok(map)
        });
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let company_clone = company.1 .0.clone();
        let buyer_node_id = BcrKeys::new().get_public_key();
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let offer_to_sell_block = BillBlock::create_block_for_offer_to_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: IdentityPublicData::from(company_clone.clone()).into(),
                    buyer: IdentityPublicData::new_only_node_id(buyer_node_id.clone()).into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: Some(BillSignatoryBlockData {
                        node_id: get_baseline_identity().identity.node_id.clone(),
                        name: get_baseline_identity().identity.name.clone(),
                    }),
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(offer_to_sell_block));
            Ok(chain)
        });
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_bill_is_sold_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service.check_bills_offer_to_sell_payment().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_timeouts_does_nothing_if_not_timed_out() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();

        let op_codes = HashSet::from([
            BillOpCode::RequestToAccept,
            BillOpCode::RequestToPay,
            BillOpCode::OfferToSell,
            BillOpCode::RequestRecourse,
        ]);

        storage
            .expect_get_keys()
            .returning(|_| {
                Ok(BillKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                })
            })
            .times(2);

        // fetches bill ids
        storage
            .expect_get_bill_ids_with_op_codes_since()
            .with(eq(op_codes.clone()), eq(0))
            .returning(|_, _| Ok(vec!["1234".to_string(), "4321".to_string()]));

        // fetches bill chain accept
        chain_storage
            .expect_get_chain()
            .with(eq("1234".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_accept_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        // fetches bill chain pay
        chain_storage
            .expect_get_chain()
            .with(eq("4321".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_pay_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        let notification_service = MockNotificationServiceApi::new();

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        // now is the same as block created time so no timeout should have happened
        let res = service.check_bills_timeouts(1000).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_timeouts_does_nothing_if_notifications_are_already_sent() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();

        let op_codes = HashSet::from([
            BillOpCode::RequestToAccept,
            BillOpCode::RequestToPay,
            BillOpCode::OfferToSell,
            BillOpCode::RequestRecourse,
        ]);

        storage
            .expect_get_keys()
            .returning(|_| {
                Ok(BillKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                })
            })
            .times(2);

        // fetches bill ids
        storage
            .expect_get_bill_ids_with_op_codes_since()
            .with(eq(op_codes.clone()), eq(0))
            .returning(|_, _| Ok(vec!["1234".to_string(), "4321".to_string()]));

        // fetches bill chain accept
        chain_storage
            .expect_get_chain()
            .with(eq("1234".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_accept_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        // fetches bill chain pay
        chain_storage
            .expect_get_chain()
            .with(eq("4321".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_pay_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        let mut notification_service = MockNotificationServiceApi::new();

        // notification already sent
        notification_service
            .expect_check_bill_notification_sent()
            .with(eq("1234"), eq(2), eq(ActionType::AcceptBill))
            .returning(|_, _, _| Ok(true));

        // notification already sent
        notification_service
            .expect_check_bill_notification_sent()
            .with(eq("4321"), eq(2), eq(ActionType::PayBill))
            .returning(|_, _, _| Ok(true));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .check_bills_timeouts(PAYMENT_DEADLINE_SECONDS + 1100)
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_timeouts() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();

        let op_codes = HashSet::from([
            BillOpCode::RequestToAccept,
            BillOpCode::RequestToPay,
            BillOpCode::OfferToSell,
            BillOpCode::RequestRecourse,
        ]);

        storage
            .expect_get_keys()
            .returning(|_| {
                Ok(BillKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                })
            })
            .times(2);

        // fetches bill ids
        storage
            .expect_get_bill_ids_with_op_codes_since()
            .with(eq(op_codes.clone()), eq(0))
            .returning(|_, _| Ok(vec!["1234".to_string(), "4321".to_string()]));

        // fetches bill chain accept
        chain_storage
            .expect_get_chain()
            .with(eq("1234".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_accept_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        // fetches bill chain pay
        chain_storage
            .expect_get_chain()
            .with(eq("4321".to_string()))
            .returning(|id| {
                let mut chain = get_genesis_chain(Some(get_baseline_bill(id)));
                chain.try_add_block(request_to_pay_block(id, 1000, chain.get_latest_block()));
                Ok(chain)
            });

        let mut notification_service = MockNotificationServiceApi::new();

        // notification not sent
        notification_service
            .expect_check_bill_notification_sent()
            .with(eq("1234"), eq(2), eq(ActionType::AcceptBill))
            .returning(|_, _, _| Ok(false));

        // notification not sent
        notification_service
            .expect_check_bill_notification_sent()
            .with(eq("4321"), eq(2), eq(ActionType::PayBill))
            .returning(|_, _, _| Ok(false));

        let identity = get_baseline_identity().identity;
        let cloned = identity.clone();
        // get own identity
        identity_storage
            .expect_get()
            .returning(move || Ok(cloned.clone()));

        // we should have at least two participants
        let recipient_check = function(|r: &Vec<IdentityPublicData>| r.len() >= 2);

        // send accept timeout notification
        notification_service
            .expect_send_request_to_action_timed_out_event()
            .with(
                eq("1234"),
                always(),
                eq(ActionType::AcceptBill),
                recipient_check.clone(),
            )
            .returning(|_, _, _, _| Ok(()));

        // send pay timeout notification
        notification_service
            .expect_send_request_to_action_timed_out_event()
            .with(
                eq("4321"),
                always(),
                eq(ActionType::PayBill),
                recipient_check,
            )
            .returning(|_, _, _, _| Ok(()));

        // marks accept bill timeout as sent
        notification_service
            .expect_mark_bill_notification_sent()
            .with(eq("1234"), eq(2), eq(ActionType::AcceptBill))
            .returning(|_, _, _| Ok(()));

        // marks pay bill timeout as sent
        notification_service
            .expect_mark_bill_notification_sent()
            .with(eq("4321"), eq(2), eq(ActionType::PayBill))
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .check_bills_timeouts(PAYMENT_DEADLINE_SECONDS + 1100)
            .await;
        assert!(res.is_ok());
    }

    fn request_to_accept_block(id: &str, ts: u64, first_block: &BillBlock) -> BillBlock {
        BillBlock::create_block_for_request_to_accept(
            id.to_string(),
            first_block,
            &BillRequestToAcceptBlockData {
                requester: IdentityPublicData::new_only_node_id(
                    BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP)
                        .unwrap()
                        .get_public_key(),
                )
                .into(),
                signatory: None,
                signing_timestamp: ts,
                signing_address: PostalAddress::new_empty(),
            },
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1000,
        )
        .expect("block could not be created")
    }

    fn request_to_pay_block(id: &str, ts: u64, first_block: &BillBlock) -> BillBlock {
        BillBlock::create_block_for_request_to_pay(
            id.to_string(),
            first_block,
            &BillRequestToPayBlockData {
                requester: IdentityPublicData::new_only_node_id(
                    BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP)
                        .unwrap()
                        .get_public_key(),
                )
                .into(),
                currency: "SATS".to_string(),
                signatory: None,
                signing_timestamp: ts,
                signing_address: PostalAddress::new_empty(),
            },
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1000,
        )
        .expect("block could not be created")
    }

    #[tokio::test]
    async fn get_endorsements_baseline() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        bill.drawer = IdentityPublicData::new(identity.identity.clone()).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
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
            company_storage,
        );

        let res = service
            .get_endorsements("1234", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_endorsements_multi() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        let drawer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let mint_endorsee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let endorse_endorsee =
            IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let sell_endorsee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());

        bill.drawer = drawer.clone();
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });

        let endorse_endorsee_clone = endorse_endorsee.clone();
        let mint_endorsee_clone = mint_endorsee.clone();
        let sell_endorsee_clone = sell_endorsee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add endorse block from payee to endorsee
            let endorse_block = BillBlock::create_block_for_endorse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorsee: endorse_endorsee.clone().into(),
                    // endorsed by payee
                    endorser: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    signatory: None,
                    signing_timestamp: now + 1,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 1,
            )
            .unwrap();
            assert!(chain.try_add_block(endorse_block));

            // add sell block from endorsee to sell endorsee
            let sell_block = BillBlock::create_block_for_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillSellBlockData {
                    buyer: sell_endorsee.clone().into(),
                    // endorsed by endorsee
                    seller: endorse_endorsee.clone().into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now + 2,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 2,
            )
            .unwrap();
            assert!(chain.try_add_block(sell_block));

            // add mint block from sell endorsee to mint endorsee
            let mint_block = BillBlock::create_block_for_mint(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillMintBlockData {
                    endorsee: mint_endorsee.clone().into(),
                    // endorsed by sell endorsee
                    endorser: sell_endorsee.clone().into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    signatory: None,
                    signing_timestamp: now + 3,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 3,
            )
            .unwrap();
            assert!(chain.try_add_block(mint_block));

            Ok(chain)
        });

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .get_endorsements("1234", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        // with duplicates
        assert_eq!(res.as_ref().unwrap().len(), 3);
        // mint was last, so it's first
        assert_eq!(
            res.as_ref().unwrap()[0].pay_to_the_order_of.node_id,
            mint_endorsee_clone.node_id
        );
        assert_eq!(
            res.as_ref().unwrap()[1].pay_to_the_order_of.node_id,
            sell_endorsee_clone.node_id
        );
        assert_eq!(
            res.as_ref().unwrap()[2].pay_to_the_order_of.node_id,
            endorse_endorsee_clone.node_id
        );
    }

    #[tokio::test]
    async fn get_past_endorsees_baseline() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        bill.drawer = IdentityPublicData::new(identity.identity.clone()).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
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
            company_storage,
        );

        let res = service
            .get_past_endorsees("1234", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        // if we're the drawee and drawer, there's no holder before us
        assert_eq!(res.as_ref().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_past_endorsees_fails_if_not_my_bill() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        bill.drawer = IdentityPublicData::new(identity.identity.clone()).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
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
            company_storage,
        );

        let res = service
            .get_past_endorsees("1234", "some_other_node_id")
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_past_endorsees_3_party() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        let drawer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.drawer = drawer.clone();
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
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
            company_storage,
        );

        let res = service
            .get_past_endorsees("1234", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        // if it's a 3 party bill and we're the payee, the drawer is a previous holder
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(
            res.as_ref().unwrap()[0].pay_to_the_order_of.node_id,
            drawer.node_id
        );
    }

    #[tokio::test]
    async fn get_past_endorsees_multi() {
        let (
            mut storage,
            mut chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("1234");
        let drawer = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let mint_endorsee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let endorse_endorsee =
            IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let sell_endorsee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());

        bill.drawer = drawer.clone();
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

        storage.expect_exists().returning(|_| true);
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });

        let endorse_endorsee_clone = endorse_endorsee.clone();
        let mint_endorsee_clone = mint_endorsee.clone();
        let sell_endorsee_clone = sell_endorsee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add endorse block from payee to endorsee
            let endorse_block = BillBlock::create_block_for_endorse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorsee: endorse_endorsee.clone().into(),
                    // endorsed by payee
                    endorser: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    signatory: None,
                    signing_timestamp: now + 1,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 1,
            )
            .unwrap();
            assert!(chain.try_add_block(endorse_block));

            // add sell block from endorsee to sell endorsee
            let sell_block = BillBlock::create_block_for_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillSellBlockData {
                    buyer: sell_endorsee.clone().into(),
                    // endorsed by endorsee
                    seller: endorse_endorsee.clone().into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now + 2,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 2,
            )
            .unwrap();
            assert!(chain.try_add_block(sell_block));

            // add mint block from sell endorsee to mint endorsee
            let mint_block = BillBlock::create_block_for_mint(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillMintBlockData {
                    endorsee: mint_endorsee.clone().into(),
                    // endorsed by sell endorsee
                    endorser: sell_endorsee.clone().into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    signatory: None,
                    signing_timestamp: now + 3,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 3,
            )
            .unwrap();
            assert!(chain.try_add_block(mint_block));

            // add endorse block back to endorsee
            let endorse_block_back = BillBlock::create_block_for_endorse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorsee: endorse_endorsee.clone().into(),
                    // endorsed by payee
                    endorser: mint_endorsee.clone().into(),
                    signatory: None,
                    signing_timestamp: now + 4,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 4,
            )
            .unwrap();
            assert!(chain.try_add_block(endorse_block_back));

            // add endorse block back to payee (caller)
            let endorse_block_last = BillBlock::create_block_for_endorse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorsee: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    // endorsed by payee
                    endorser: endorse_endorsee.clone().into(),
                    signatory: None,
                    signing_timestamp: now + 5,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 5,
            )
            .unwrap();
            assert!(chain.try_add_block(endorse_block_last));

            Ok(chain)
        });

        let service = get_service(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .get_past_endorsees("1234", &identity.identity.node_id)
            .await;
        assert!(res.is_ok());
        // if there are mint, sell and endorse blocks, they are considered
        // but without duplicates
        assert_eq!(res.as_ref().unwrap().len(), 4);
        // endorse endorsee is the one directly before
        assert_eq!(
            res.as_ref().unwrap()[0].pay_to_the_order_of.node_id,
            endorse_endorsee_clone.node_id
        );
        // mint endorsee is the one after that
        assert_eq!(
            res.as_ref().unwrap()[1].pay_to_the_order_of.node_id,
            mint_endorsee_clone.node_id
        );
        // sell endorsee is the next one
        assert_eq!(
            res.as_ref().unwrap()[2].pay_to_the_order_of.node_id,
            sell_endorsee_clone.node_id
        );
        // drawer is the last one, because endorse endorsee is already there
        // and drawer != drawee
        assert_eq!(
            res.as_ref().unwrap()[3].pay_to_the_order_of.node_id,
            drawer.node_id
        );
    }

    #[tokio::test]
    async fn reject_acceptance_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let bill = get_baseline_bill("1234");
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let payee = bill.payee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add req to accept block
            let req_to_accept = BillBlock::create_block_for_request_to_accept(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestToAcceptBlockData {
                    requester: payee.clone().into(),
                    signatory: None,
                    signing_timestamp: now + 1,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now + 1,
            )
            .unwrap();
            assert!(chain.try_add_block(req_to_accept));

            Ok(chain)
        });

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_request_to_action_rejected_event()
            .with(eq("1234"), always(), eq(ActionType::AcceptBill), always())
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .reject_acceptance(
                "1234",
                &IdentityPublicData::new(identity.identity).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(
            res.as_ref().unwrap().blocks()[2].op_code,
            BillOpCode::RejectToAccept
        );
    }

    #[tokio::test]
    async fn reject_buying_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let bill = get_baseline_bill("1234");
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let payee = bill.payee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add offer to sell block
            let offer_to_sell_block = BillBlock::create_block_for_offer_to_sell(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillOfferToSellBlockData {
                    seller: payee.clone().into(),
                    buyer: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    payment_address: "1234paymentaddress".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
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

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_request_to_action_rejected_event()
            .with(eq("1234"), always(), eq(ActionType::BuyBill), always())
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .reject_buying(
                "1234",
                &IdentityPublicData::new(identity.identity).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(
            res.as_ref().unwrap().blocks()[2].op_code,
            BillOpCode::RejectToBuy
        );
    }

    #[tokio::test]
    async fn reject_payment() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let bill = get_baseline_bill("1234");
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_is_paid().returning(|_| Ok(false));
        let payee = bill.payee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add req to pay
            let req_to_pay = BillBlock::create_block_for_request_to_pay(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestToPayBlockData {
                    requester: payee.clone().into(),
                    currency: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(req_to_pay));

            Ok(chain)
        });

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_request_to_action_rejected_event()
            .with(eq("1234"), always(), eq(ActionType::PayBill), always())
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .reject_payment(
                "1234",
                &IdentityPublicData::new(identity.identity).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(
            res.as_ref().unwrap().blocks()[2].op_code,
            BillOpCode::RejectToPay
        );
    }

    #[tokio::test]
    async fn reject_recourse() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let bill = get_baseline_bill("1234");
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        identity_storage
            .expect_get_full()
            .returning(|| Ok(get_baseline_identity()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let payee = bill.payee.clone();

        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));

            // add req to pay
            let req_to_pay = BillBlock::create_block_for_request_recourse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestRecourseBlockData {
                    recourser: payee.clone().into(),
                    recoursee: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(req_to_pay));

            Ok(chain)
        });

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_request_to_action_rejected_event()
            .with(eq("1234"), always(), eq(ActionType::RecourseBill), always())
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .reject_payment_for_recourse(
                "1234",
                &IdentityPublicData::new(identity.identity).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(
            res.as_ref().unwrap().blocks()[2].op_code,
            BillOpCode::RejectToPayRecourse
        );
    }

    #[tokio::test]
    async fn check_bills_in_recourse_payment_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();

        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();

        storage
            .expect_get_bill_ids_waiting_for_recourse_payment()
            .returning(|| Ok(vec!["1234".to_string()]));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let recoursee = BcrKeys::new().get_public_key();
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let req_to_recourse = BillBlock::create_block_for_request_recourse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestRecourseBlockData {
                    recourser: IdentityPublicData::new(get_baseline_identity().identity)
                        .unwrap()
                        .into(),
                    recoursee: IdentityPublicData::new_only_node_id(recoursee.clone()).into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    signatory: None,
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(req_to_recourse));
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
            .expect_send_bill_recourse_paid_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service.check_bills_in_recourse_payment().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn check_bills_in_recourse_payment_company_is_recourser() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            mut company_storage,
        ) = get_storages();

        let mut identity = get_baseline_identity();
        identity.key_pair = BcrKeys::new();
        identity.identity.node_id = identity.key_pair.get_public_key();

        let company = get_baseline_company_data();
        let mut bill = get_baseline_bill("1234");
        bill.payee = IdentityPublicData::from(company.1 .0.clone());

        storage
            .expect_get_bill_ids_waiting_for_recourse_payment()
            .returning(|| Ok(vec!["1234".to_string()]));
        let company_clone = company.clone();
        company_storage.expect_get_all().returning(move || {
            let mut map = HashMap::new();
            map.insert(
                company_clone.0.clone(),
                (company_clone.1 .0.clone(), company_clone.1 .1.clone()),
            );
            Ok(map)
        });
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        let company_clone = company.1 .0.clone();
        let recoursee = BcrKeys::new().get_public_key();
        chain_storage.expect_get_chain().returning(move |_| {
            let now = util::date::now().timestamp() as u64;
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let req_to_recourse = BillBlock::create_block_for_request_recourse(
                "1234".to_string(),
                chain.get_latest_block(),
                &BillRequestRecourseBlockData {
                    recourser: IdentityPublicData::from(company_clone.clone()).into(),
                    recoursee: IdentityPublicData::new_only_node_id(recoursee.clone()).into(),
                    currency: "sat".to_string(),
                    sum: 15000,
                    signatory: Some(BillSignatoryBlockData {
                        node_id: get_baseline_identity().identity.node_id.clone(),
                        name: get_baseline_identity().identity.name.clone(),
                    }),
                    signing_timestamp: now,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                now,
            )
            .unwrap();
            assert!(chain.try_add_block(req_to_recourse));
            Ok(chain)
        });
        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();
        notification_service
            .expect_send_bill_recourse_paid_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service.check_bills_in_recourse_payment().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn request_recourse_accept_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let recoursee = bill.payee.clone();
        let endorsee_caller = IdentityPublicData::new(identity.identity.clone()).unwrap();

        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        chain_storage.expect_get_chain().returning(move |_| {
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let endorse_block = BillBlock::create_block_for_endorse(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorser: bill.payee.clone().into(),
                    endorsee: endorsee_caller.clone().into(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(endorse_block);
            let req_to_accept = BillBlock::create_block_for_request_to_accept(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillRequestToAcceptBlockData {
                    requester: bill.payee.clone().into(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(req_to_accept);
            let reject_accept = BillBlock::create_block_for_reject_to_accept(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillRejectBlockData {
                    rejecter: bill.drawee.clone().into(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(reject_accept);
            Ok(chain)
        });
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to recourse event should be sent
        notification_service
            .expect_send_recourse_action_event()
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .request_recourse(
                "some id",
                &recoursee,
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                RecourseReason::Accept,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 5);
        assert!(res.unwrap().blocks()[4].op_code == BillOpCode::RequestRecourse);
    }

    #[tokio::test]
    async fn request_recourse_payment_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let recoursee = bill.payee.clone();
        let endorsee_caller = IdentityPublicData::new(identity.identity.clone()).unwrap();

        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_is_paid().returning(|_| Ok(false));
        chain_storage.expect_get_chain().returning(move |_| {
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let endorse_block = BillBlock::create_block_for_endorse(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillEndorseBlockData {
                    endorser: bill.payee.clone().into(),
                    endorsee: endorsee_caller.clone().into(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(endorse_block);
            let req_to_pay = BillBlock::create_block_for_request_to_pay(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillRequestToPayBlockData {
                    requester: bill.payee.clone().into(),
                    currency: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(req_to_pay);
            let reject_pay = BillBlock::create_block_for_reject_to_pay(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillRejectBlockData {
                    rejecter: bill.drawee.clone().into(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(reject_pay);
            Ok(chain)
        });
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Request to recourse event should be sent
        notification_service
            .expect_send_recourse_action_event()
            .returning(|_, _, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .request_recourse(
                "some id",
                &recoursee,
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                RecourseReason::Pay(15000, "sat".to_string()),
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert!(res.as_ref().unwrap().blocks().len() == 5);
        assert!(res.unwrap().blocks()[4].op_code == BillOpCode::RequestRecourse);
    }

    #[tokio::test]
    async fn recourse_bitcredit_bill_baseline() {
        let (
            mut storage,
            mut chain_storage,
            mut identity_storage,
            file_upload_storage,
            identity_chain_store,
            company_chain_store,
            contact_storage,
            company_storage,
        ) = get_storages();
        let identity = get_baseline_identity();
        let mut bill = get_baseline_bill("some id");
        bill.drawee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        bill.payee = IdentityPublicData::new(identity.identity.clone()).unwrap();
        let recoursee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let recoursee_clone = recoursee.clone();
        let identity_clone = identity.identity.clone();

        chain_storage.expect_add_block().returning(|_, _| Ok(()));
        storage.expect_get_keys().returning(|_| {
            Ok(BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            })
        });
        storage.expect_is_paid().returning(|_| Ok(false));
        chain_storage.expect_get_chain().returning(move |_| {
            let mut chain = get_genesis_chain(Some(bill.clone()));
            let req_to_recourse = BillBlock::create_block_for_request_recourse(
                "some id".to_string(),
                chain.get_latest_block(),
                &BillRequestRecourseBlockData {
                    recourser: IdentityPublicData::new(identity_clone.clone())
                        .unwrap()
                        .into(),
                    recoursee: recoursee_clone.clone().into(),
                    sum: 15000,
                    currency: "sat".to_string(),
                    signatory: None,
                    signing_timestamp: 1731593927,
                    signing_address: PostalAddress::new_empty(),
                },
                &BcrKeys::new(),
                None,
                &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
                1731593927,
            )
            .unwrap();
            chain.try_add_block(req_to_recourse);
            Ok(chain)
        });
        let identity_clone = identity.clone();
        identity_storage
            .expect_get_full()
            .returning(move || Ok(identity_clone.clone()));

        let mut notification_service = MockNotificationServiceApi::new();

        // Recourse paid event should be sent
        notification_service
            .expect_send_bill_recourse_paid_event()
            .returning(|_, _, _| Ok(()));

        let service = get_service_base(
            storage,
            chain_storage,
            identity_storage,
            file_upload_storage,
            identity_chain_store,
            notification_service,
            company_chain_store,
            contact_storage,
            company_storage,
        );

        let res = service
            .recourse_bitcredit_bill(
                "some id",
                recoursee,
                15000,
                "sat",
                &IdentityPublicData::new(identity.identity.clone()).unwrap(),
                &identity.key_pair,
                1731593928,
            )
            .await;
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().blocks().len(), 3);
        assert_eq!(res.unwrap().blocks()[2].op_code, BillOpCode::Recourse);
    }
}
