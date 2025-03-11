use super::error::Error;
use super::{BillAction, BillServiceApi, Result};
use crate::blockchain::Blockchain;
use crate::blockchain::bill::block::BillIdentityBlockData;
use crate::blockchain::bill::{BillBlockchain, BillOpCode};
use crate::data::{
    File,
    bill::{
        BillCombinedBitcoinKey, BillKeys, BillRole, BillsBalance, BillsBalanceOverview,
        BillsFilterRole, BitcreditBill, BitcreditBillResult, Endorsement, LightBitcreditBillResult,
        LightSignedBy, PastEndorsee,
    },
    contact::{ContactType, IdentityPublicData, LightIdentityPublicData},
    identity::Identity,
};
use crate::external::bitcoin::BitcoinClientApi;
use crate::get_config;
use crate::persistence::bill::BillChainStoreApi;
use crate::persistence::bill::BillStoreApi;
use crate::persistence::company::{CompanyChainStoreApi, CompanyStoreApi};
use crate::persistence::contact::ContactStoreApi;
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::{IdentityChainStoreApi, IdentityStoreApi};
use crate::util::BcrKeys;
use crate::{external, util};
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::constants::{
    ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS,
};
use bcr_ebill_core::notification::ActionType;
use bcr_ebill_transport::NotificationServiceApi;
use futures::future::try_join_all;
use log::{error, info};
use std::collections::HashSet;
use std::sync::Arc;

/// The bill service is responsible for all bill-related logic and for syncing them with the
/// network
#[derive(Clone)]
pub struct BillService {
    pub store: Arc<dyn BillStoreApi>,
    pub blockchain_store: Arc<dyn BillChainStoreApi>,
    pub identity_store: Arc<dyn IdentityStoreApi>,
    pub file_upload_store: Arc<dyn FileUploadStoreApi>,
    pub bitcoin_client: Arc<dyn BitcoinClientApi>,
    pub notification_service: Arc<dyn NotificationServiceApi>,
    pub identity_blockchain_store: Arc<dyn IdentityChainStoreApi>,
    pub company_blockchain_store: Arc<dyn CompanyChainStoreApi>,
    pub contact_store: Arc<dyn ContactStoreApi>,
    pub company_store: Arc<dyn CompanyStoreApi>,
}
impl ServiceTraitBounds for BillService {}

impl BillService {
    pub fn new(
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

    /// If it's our identity, we take the fields from there, otherwise we check contacts,
    /// companies, or leave them empty
    pub(super) async fn extend_bill_chain_identity_data_from_contacts_or_identity(
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
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
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
    ) -> Result<Vec<LightBitcreditBillResult>> {
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

    async fn get_bills_from_all_identities(&self) -> Result<Vec<BitcreditBillResult>> {
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

    async fn get_bills(&self, current_identity_node_id: &str) -> Result<Vec<BitcreditBillResult>> {
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

        // The first key is always the bill key
        let private_key = self.bitcoin_client.get_combined_private_key(
            &BcrKeys::from_private_key(&bill_keys.private_key)?
                .get_bitcoin_private_key(get_config().bitcoin_network()),
            &caller_keys.get_bitcoin_private_key(get_config().bitcoin_network()),
        )?;
        return Ok(BillCombinedBitcoinKey { private_key });
    }

    async fn get_detail(
        &self,
        bill_id: &str,
        identity: &Identity,
        current_identity_node_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillResult> {
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
        t: u64,
        country_of_issuing: String,
        city_of_issuing: String,
        issue_date: String,
        maturity_date: String,
        drawee: String,
        payee: String,
        sum: String,
        currency: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        file_upload_id: Option<String>,
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
        timestamp: u64,
    ) -> Result<BitcreditBill> {
        self.issue_bill(
            t,
            country_of_issuing,
            city_of_issuing,
            issue_date,
            maturity_date,
            drawee,
            payee,
            sum,
            currency,
            country_of_payment,
            city_of_payment,
            language,
            file_upload_id,
            drawer_public_data,
            drawer_keys,
            timestamp,
        )
        .await
    }

    async fn execute_bill_action(
        &self,
        bill_id: &str,
        bill_action: BillAction,
        signer_public_data: &IdentityPublicData,
        signer_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<BillBlockchain> {
        // fetch data
        let identity = self.identity_store.get_full().await?;
        let mut blockchain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&blockchain, &bill_keys, &identity.identity)
            .await?;

        // validate
        self.validate_bill_action(
            &blockchain,
            &bill,
            &bill_keys,
            timestamp,
            &signer_public_data.node_id,
            &bill_action,
        )
        .await?;

        // create and sign blocks
        self.create_blocks_for_bill_action(
            &bill,
            &mut blockchain,
            &bill_keys,
            &bill_action,
            signer_public_data,
            signer_keys,
            &identity,
            timestamp,
        )
        .await?;

        // notify
        self.notify_for_block_action(&blockchain, &bill_keys, &bill_action, &identity.identity)
            .await?;

        // propagate
        let self_clone = self.clone();
        let latest_block = blockchain.get_latest_block().clone();
        let bill_id_clone = bill_id.to_owned();
        if let Err(e) = self_clone
            .propagate_block(&bill_id_clone, &latest_block)
            .await
        {
            error!("Error propagating block: {e}");
        }

        match bill_action {
            BillAction::Endorse(endorsee) => {
                if let Err(e) = self_clone
                    .propagate_bill_for_node_id(&bill_id_clone, &endorsee.node_id)
                    .await
                {
                    error!("Error propagating bill for node_id: {e}");
                }
            }
            BillAction::Sell(buyer, _, _, _) => {
                if let Err(e) = self_clone
                    .propagate_bill_for_node_id(&bill_id_clone, &buyer.node_id)
                    .await
                {
                    error!("Error propagating bill for node_id: {e}");
                }
            }
            BillAction::Mint(mint, _, _) => {
                if let Err(e) = self_clone
                    .propagate_bill_for_node_id(&bill_id_clone, &mint.node_id)
                    .await
                {
                    error!("Error propagating bill for node_id: {e}");
                }
            }
            BillAction::Recourse(recoursee, _, _) => {
                if let Err(e) = self_clone
                    .propagate_bill_for_node_id(&bill_id_clone, &recoursee.node_id)
                    .await
                {
                    error!("Error propagating bill for node_id: {e}");
                }
            }
            _ => (),
        };

        Ok(blockchain)
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
