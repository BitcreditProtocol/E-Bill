use super::Result;
use super::service::BillService;
use crate::util;
use bcr_ebill_core::{
    bill::{
        BillAcceptanceStatus, BillCurrentWaitingState, BillData, BillKeys, BillParticipants,
        BillPaymentStatus, BillRecourseStatus, BillSellStatus, BillStatus,
        BillWaitingForPaymentState, BillWaitingForRecourseState, BillWaitingForSellState,
        BitcreditBill, BitcreditBillResult, LightSignedBy, PastEndorsee,
    },
    blockchain::{
        Blockchain,
        bill::{
            BillBlockchain, BillOpCode, OfferToSellWaitingForPayment, RecourseWaitingForPayment,
            block::{
                BillEndorseBlockData, BillMintBlockData, BillSellBlockData, BillSignatoryBlockData,
            },
        },
    },
    constants::{ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS},
    contact::{ContactType, IdentityPublicData, LightIdentityPublicData},
    identity::{Identity, IdentityWithAll},
    util::BcrKeys,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct BillSigningKeys {
    pub signatory_keys: BcrKeys,
    pub company_keys: Option<BcrKeys>,
    pub signatory_identity: Option<BillSignatoryBlockData>,
}

impl BillService {
    pub(super) async fn get_last_version_bill(
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

    pub(super) fn get_bill_signing_keys(
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

    pub(super) async fn get_full_bill(
        &self,
        bill_id: &str,
        local_identity: &Identity,
        current_identity_node_id: &str,
        current_timestamp: u64,
    ) -> Result<BitcreditBillResult> {
        let chain = self.blockchain_store.get_chain(bill_id).await?;
        let bill_keys = self.store.get_keys(bill_id).await?;
        let bill = self
            .get_last_version_bill(&chain, &bill_keys, local_identity)
            .await?;
        let first_version_bill = chain.get_first_version_bill(&bill_keys)?;
        let time_of_drawing = first_version_bill.signing_timestamp;

        let chain_clone = chain.clone();
        let bill_participants = chain_clone.get_all_nodes_from_bill(&bill_keys)?;
        let endorsements_count = chain.get_endorsements_count();

        let holder = match bill.endorsee {
            None => &bill.payee,
            Some(ref endorsee) => endorsee,
        };

        let mut requested_to_pay = chain.block_with_operation_code_exists(BillOpCode::RequestToPay);
        let mut paid = false;
        if requested_to_pay {
            paid = self.store.is_paid(&bill.id).await?;
        }

        // calculate, if the caller has received funds at any point in the bill
        let mut redeemed_funds_available =
            chain.is_beneficiary_from_a_block(&bill_keys, current_identity_node_id);
        if holder.node_id == current_identity_node_id && paid {
            redeemed_funds_available = true;
        }

        let mut request_to_pay_timed_out = false;

        let mut offered_to_sell = chain.block_with_operation_code_exists(BillOpCode::OfferToSell);
        let mut offer_to_sell_timed_out = false;

        let mut requested_to_recourse =
            chain.block_with_operation_code_exists(BillOpCode::RequestRecourse);
        let mut request_to_recourse_timed_out = false;

        let accepted = chain.block_with_operation_code_exists(BillOpCode::Accept);
        let mut requested_to_accept =
            chain.block_with_operation_code_exists(BillOpCode::RequestToAccept);
        let rejected_to_accept = chain.block_with_operation_code_exists(BillOpCode::RejectToAccept);
        let mut request_to_accept_timed_out = false;
        if requested_to_accept && !accepted && !rejected_to_accept {
            if let Some(req_block) =
                chain.get_last_version_block_with_op_code(BillOpCode::RequestToAccept)
            {
                if chain.check_if_deadline_has_passed(
                    req_block.timestamp,
                    current_timestamp,
                    ACCEPT_DEADLINE_SECONDS,
                ) {
                    request_to_accept_timed_out = true;
                    requested_to_accept = true;
                }
            }
        }

        let last_block = chain.get_latest_block();
        let current_waiting_state = match last_block.op_code {
            BillOpCode::OfferToSell => {
                if let OfferToSellWaitingForPayment::Yes(payment_info) = chain
                    .is_last_offer_to_sell_block_waiting_for_payment(
                        &bill_keys,
                        current_timestamp,
                    )?
                {
                    // we're waiting, collect data
                    let buyer = self
                        .extend_bill_chain_identity_data_from_contacts_or_identity(
                            payment_info.buyer.clone(),
                            local_identity,
                        )
                        .await;
                    let seller = self
                        .extend_bill_chain_identity_data_from_contacts_or_identity(
                            payment_info.seller.clone(),
                            local_identity,
                        )
                        .await;

                    let address_to_pay = self
                        .bitcoin_client
                        .get_address_to_pay(&bill_keys.public_key, &payment_info.seller.node_id)?;

                    let link_to_pay = self.bitcoin_client.generate_link_to_pay(
                        &address_to_pay,
                        payment_info.sum,
                        &format!("Payment in relation to a bill {}", &bill.id),
                    );

                    let mempool_link_for_address_to_pay = self
                        .bitcoin_client
                        .get_mempool_link_for_address(&address_to_pay);

                    Some(BillCurrentWaitingState::Sell(BillWaitingForSellState {
                        time_of_request: last_block.timestamp,
                        seller,
                        buyer,
                        currency: payment_info.currency,
                        sum: util::currency::sum_to_string(payment_info.sum),
                        link_to_pay,
                        address_to_pay,
                        mempool_link_for_address_to_pay,
                    }))
                } else {
                    // it timed out, we're not waiting anymore
                    offer_to_sell_timed_out = true;
                    offered_to_sell = true;
                    None
                }
            }
            BillOpCode::RequestToPay => {
                if paid {
                    // it's paid - we're not waiting anymore
                    None
                } else if chain.check_if_deadline_has_passed(
                    last_block.timestamp,
                    current_timestamp,
                    PAYMENT_DEADLINE_SECONDS,
                ) {
                    // it timed out, we're not waiting anymore
                    request_to_pay_timed_out = true;
                    requested_to_pay = true;
                    None
                } else {
                    // we're waiting, collect data
                    let address_to_pay = self
                        .bitcoin_client
                        .get_address_to_pay(&bill_keys.public_key, &holder.node_id)?;

                    let link_to_pay = self.bitcoin_client.generate_link_to_pay(
                        &address_to_pay,
                        bill.sum,
                        &format!("Payment in relation to a bill {}", bill.id.clone()),
                    );

                    let mempool_link_for_address_to_pay = self
                        .bitcoin_client
                        .get_mempool_link_for_address(&address_to_pay);

                    Some(BillCurrentWaitingState::Payment(
                        BillWaitingForPaymentState {
                            time_of_request: last_block.timestamp,
                            payer: bill.drawee.clone(),
                            payee: holder.clone(),
                            currency: bill.currency.clone(),
                            sum: util::currency::sum_to_string(bill.sum),
                            link_to_pay,
                            address_to_pay,
                            mempool_link_for_address_to_pay,
                        },
                    ))
                }
            }
            BillOpCode::RequestRecourse => {
                if let RecourseWaitingForPayment::Yes(payment_info) = chain
                    .is_last_request_to_recourse_block_waiting_for_payment(
                        &bill_keys,
                        current_timestamp,
                    )?
                {
                    // we're waiting, collect data
                    let recourser = self
                        .extend_bill_chain_identity_data_from_contacts_or_identity(
                            payment_info.recourser.clone(),
                            local_identity,
                        )
                        .await;
                    let recoursee = self
                        .extend_bill_chain_identity_data_from_contacts_or_identity(
                            payment_info.recoursee.clone(),
                            local_identity,
                        )
                        .await;

                    let address_to_pay = self.bitcoin_client.get_address_to_pay(
                        &bill_keys.public_key,
                        &payment_info.recourser.node_id,
                    )?;

                    let link_to_pay = self.bitcoin_client.generate_link_to_pay(
                        &address_to_pay,
                        payment_info.sum,
                        &format!("Payment in relation to a bill {}", &bill.id),
                    );

                    let mempool_link_for_address_to_pay = self
                        .bitcoin_client
                        .get_mempool_link_for_address(&address_to_pay);

                    Some(BillCurrentWaitingState::Recourse(
                        BillWaitingForRecourseState {
                            time_of_request: last_block.timestamp,
                            recourser,
                            recoursee,
                            currency: payment_info.currency,
                            sum: util::currency::sum_to_string(payment_info.sum),
                            link_to_pay,
                            address_to_pay,
                            mempool_link_for_address_to_pay,
                        },
                    ))
                } else {
                    // it timed out, we're not waiting anymore
                    request_to_recourse_timed_out = true;
                    requested_to_recourse = true;
                    None
                }
            }
            _ => None,
        };

        let status = BillStatus {
            acceptance: BillAcceptanceStatus {
                requested_to_accept,
                accepted,
                request_to_accept_timed_out,
                rejected_to_accept,
            },
            payment: BillPaymentStatus {
                requested_to_pay,
                paid,
                request_to_pay_timed_out,
                rejected_to_pay: chain.block_with_operation_code_exists(BillOpCode::RejectToPay),
            },
            sell: BillSellStatus {
                offered_to_sell,
                offer_to_sell_timed_out,
                rejected_offer_to_sell: chain
                    .block_with_operation_code_exists(BillOpCode::RejectToBuy),
            },
            recourse: BillRecourseStatus {
                requested_to_recourse,
                request_to_recourse_timed_out,
                rejected_request_to_recourse: chain
                    .block_with_operation_code_exists(BillOpCode::RejectToPayRecourse),
            },
            redeemed_funds_available,
        };

        let active_notification = self
            .notification_service
            .get_active_bill_notification(&bill.id)
            .await;

        let participants = BillParticipants {
            drawee: bill.drawee,
            drawer: bill.drawer,
            payee: bill.payee,
            endorsee: bill.endorsee,
            endorsements_count,
            all_participant_node_ids: bill_participants,
        };

        let bill_data = BillData {
            language: bill.language,
            time_of_drawing,
            issue_date: bill.issue_date,
            time_of_maturity: util::date::date_string_to_i64_timestamp(&bill.maturity_date, None)
                .unwrap_or(0) as u64,
            maturity_date: bill.maturity_date,
            country_of_issuing: bill.country_of_issuing,
            city_of_issuing: bill.city_of_issuing,
            country_of_payment: bill.country_of_payment,
            city_of_payment: bill.city_of_payment,
            currency: bill.currency,
            sum: util::currency::sum_to_string(bill.sum),
            files: bill.files,
            active_notification,
        };

        Ok(BitcreditBillResult {
            id: bill.id,
            participants,
            data: bill_data,
            status,
            current_waiting_state,
        })
    }

    pub(super) fn get_past_endorsees_for_bill(
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
}
