use super::Result;
use super::service::BillService;
use crate::service::bill_service::{BillAction, BillServiceApi};
use bcr_ebill_core::{
    bill::RecourseReason,
    blockchain::{
        Blockchain,
        bill::{
            BillOpCode, OfferToSellWaitingForPayment, RecourseWaitingForPayment,
            block::{BillRecourseReasonBlockData, NodeId},
        },
    },
    company::{Company, CompanyKeys},
    contact::BillIdentifiedParticipant,
    identity::{Identity, IdentityWithAll},
    util::BcrKeys,
};
use log::{debug, info};
use std::collections::HashMap;

impl BillService {
    pub(super) async fn check_bill_payment(
        &self,
        bill_id: &str,
        identity: &Identity,
    ) -> Result<()> {
        info!("Checking bill payment for {bill_id}");
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let contacts = self.contact_store.get_map().await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, identity, &contacts)
            .await?;

        if chain.block_with_operation_code_exists(BillOpCode::RequestRecourse) {
            // if the bill is in recourse, we don't have to check payment anymore
            debug!("bill {bill_id} is in recourse - not checking for payment");
            return Ok(());
        }

        let holder_public_key = match bill.endorsee {
            None => &bill.payee.node_id(),
            Some(ref endorsee) => &endorsee.node_id(),
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
                debug!("bill {bill_id} is paid - setting to paid and invalidating cache");
                self.store.set_to_paid(bill_id, &address_to_pay).await?;
                // invalidate bill cache, so payment state is updated on next fetch
                self.store.invalidate_bill_in_cache(bill_id).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn check_bill_in_recourse_payment(
        &self,
        bill_id: &str,
        identity: &IdentityWithAll,
        now: u64,
    ) -> Result<()> {
        info!("Checking bill recourse payment for {bill_id}");
        let bill_keys = self.store.get_keys(bill_id).await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let contacts = self.contact_store.get_map().await?;
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
                    debug!(
                        "bill {bill_id} is recourse-paid - creating recourse block if we're recourser"
                    );
                    // If we are the recourser and a bill issuer and it's paid, we add a Recourse block
                    if payment_info.recourser.node_id == identity.identity.node_id {
                        if let Some(signer_identity) =
                            BillIdentifiedParticipant::new(identity.identity.clone())
                        {
                            let reason = match payment_info.reason {
                                BillRecourseReasonBlockData::Pay => RecourseReason::Pay(
                                    payment_info.sum,
                                    payment_info.currency.clone(),
                                ),
                                BillRecourseReasonBlockData::Accept => RecourseReason::Accept,
                            };
                            let _ = self
                                .execute_bill_action(
                                    bill_id,
                                    BillAction::Recourse(
                                        self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                            payment_info.recoursee.clone(),
                                            &identity.identity,
                                            &contacts
                                        )
                                        .await, payment_info.sum, payment_info.currency, reason),
                                    &signer_identity,
                                    &identity.key_pair,
                                    now,
                                )
                                .await?;
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
                            let reason = match payment_info.reason {
                                BillRecourseReasonBlockData::Pay => RecourseReason::Pay(
                                    payment_info.sum,
                                    payment_info.currency.clone(),
                                ),
                                BillRecourseReasonBlockData::Accept => RecourseReason::Accept,
                            };
                            let _ = self
                                .execute_bill_action(
                                    bill_id,
                                    BillAction::Recourse(self.extend_bill_chain_identity_data_from_contacts_or_identity(
                                        payment_info.recoursee.clone(),
                                        &identity.identity,
                                        &contacts
                                    )
                                    .await, payment_info.sum, payment_info.currency, reason),
                                    // signer identity (company)
                                    &BillIdentifiedParticipant::from(recourser_company.0.clone()),
                                    // signer keys (company keys)
                                    &BcrKeys::from_private_key(&recourser_company.1.private_key)?,
                                    now,
                                )
                                .await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) async fn check_bill_offer_to_sell_payment(
        &self,
        bill_id: &str,
        identity: &IdentityWithAll,
        now: u64,
    ) -> Result<()> {
        info!("Checking bill offer to sell payment for {bill_id}");
        let bill_keys = self.store.get_keys(bill_id).await?;
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let contacts = self.contact_store.get_map().await?;
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
                    debug!("bill {bill_id} got bought - creating sell block if we're seller");
                    // If we are the seller and a bill issuer and it's paid, we add a Sell block
                    if payment_info.seller.node_id() == identity.identity.node_id {
                        if let Some(signer_identity) =
                            BillIdentifiedParticipant::new(identity.identity.clone())
                        {
                            let _ = self
                                .execute_bill_action(
                                    bill_id,
                                    BillAction::Sell(
                                    self.extend_bill_chain_participant_data_from_contacts_or_identity(
                                        payment_info.buyer.clone().into(),
                                        &identity.identity,
                                        &contacts
                                    )
                                    .await,
                                    payment_info.sum,
                                    payment_info.currency,
                                    payment_info.payment_address),
                                    &signer_identity,
                                    &identity.key_pair,
                                    now,
                                )
                                .await?;
                        }
                        return Ok(()); // return early
                    }

                    let local_companies: HashMap<String, (Company, CompanyKeys)> =
                        self.company_store.get_all().await?;
                    // If a local company is the seller, create the sell block as that company
                    if let Some(seller_company) =
                        local_companies.get(&payment_info.seller.node_id())
                    {
                        if seller_company
                            .0
                            .signatories
                            .iter()
                            .any(|s| s == &identity.identity.node_id)
                        {
                            let _ = self
                                .execute_bill_action(
                                    bill_id,
                                    BillAction::Sell(
                                    self.extend_bill_chain_participant_data_from_contacts_or_identity(
                                        payment_info.buyer.clone().into(),
                                        &identity.identity,
                                        &contacts
                                    )
                                    .await,
                                    payment_info.sum,
                                    payment_info.currency,
                                    payment_info.payment_address),
                                    // signer identity (company)
                                    &BillIdentifiedParticipant::from(seller_company.0.clone()),
                                    // signer keys (company keys)
                                    &BcrKeys::from_private_key(&seller_company.1.private_key)?,
                                    now,
                                )
                                .await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
