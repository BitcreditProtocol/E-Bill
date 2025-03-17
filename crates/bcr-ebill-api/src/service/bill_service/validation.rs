use crate::util;

use super::{BillAction, Result, error::Error, service::BillService};
use bcr_ebill_core::{
    bill::{BillKeys, BillType, BitcreditBill, RecourseReason},
    blockchain::{
        Block, Blockchain,
        bill::{
            BillBlockchain, BillOpCode, OfferToSellWaitingForPayment, RecourseWaitingForPayment,
            block::BillRequestRecourseBlockData,
        },
    },
    constants::{ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS},
};

impl BillService {
    pub(super) fn validate_bill_issue(
        &self,
        sum: &str,
        file_upload_ids: &Vec<String>,
        issue_date: &str,
        maturity_date: &str,
        drawee: &str,
        payee: &str,
        t: u64,
    ) -> Result<(u64, BillType)> {
        let sum = util::currency::parse_sum(sum).map_err(|e| Error::Validation(e.to_string()))?;

        for file_upload_id in file_upload_ids {
            util::file::validate_file_upload_id(&Some(file_upload_id.to_owned()))
                .map_err(|e| Error::Validation(e.to_string()))?;
        }

        if util::date::date_string_to_i64_timestamp(issue_date, None).is_none() {
            return Err(Error::Validation(String::from("invalid issue date")));
        }

        if util::date::date_string_to_i64_timestamp(maturity_date, None).is_none() {
            return Err(Error::Validation(String::from("invalid maturity date")));
        }

        let bill_type = match t {
            0 => BillType::PromissoryNote,
            1 => BillType::SelfDrafted,
            2 => BillType::ThreeParties,
            _ => return Err(Error::InvalidBillType),
        };

        if drawee == payee {
            return Err(Error::Validation(String::from(
                "Drawee can't be Payee at the same time",
            )));
        }
        Ok((sum, bill_type))
    }

    pub(super) async fn validate_bill_action(
        &self,
        blockchain: &BillBlockchain,
        bill: &BitcreditBill,
        bill_keys: &BillKeys,
        timestamp: u64,
        signer_node_id: &str,
        bill_action: &BillAction,
    ) -> Result<()> {
        let holder_node_id = match bill.endorsee {
            None => &bill.payee.node_id,
            Some(ref endorsee) => &endorsee.node_id,
        };

        match bill_action {
            BillAction::Accept => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // not already accepted
                if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                    return Err(Error::BillAlreadyAccepted);
                }
                // signer is drawee
                if !bill.drawee.node_id.eq(signer_node_id) {
                    return Err(Error::CallerIsNotDrawee);
                }
            }
            BillAction::RequestAcceptance => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // not already accepted
                if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                    return Err(Error::BillAlreadyAccepted);
                }
                // not currently requested to accept
                if blockchain.block_with_operation_code_exists(BillOpCode::RequestToAccept) {
                    if let Some(req_to_accept_block) =
                        blockchain.get_last_version_block_with_op_code(BillOpCode::RequestToAccept)
                    {
                        if req_to_accept_block.timestamp + ACCEPT_DEADLINE_SECONDS >= timestamp {
                            return Err(Error::BillAlreadyAccepted);
                        }
                    }
                }

                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }
            }
            BillAction::RequestToPay(_) => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }
            }
            BillAction::RequestRecourse(recoursee, recourse_reason) => {
                let past_holders =
                    self.get_past_endorsees_for_bill(blockchain, bill_keys, signer_node_id)?;

                // validation
                if !past_holders
                    .iter()
                    .any(|h| h.pay_to_the_order_of.node_id == recoursee.node_id)
                {
                    return Err(Error::RecourseeNotPastHolder);
                }

                // not blocked
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }

                match recourse_reason {
                    RecourseReason::Accept => {
                        if let Some(req_to_accept) = blockchain
                            .get_last_version_block_with_op_code(BillOpCode::RejectToAccept)
                        {
                            // only if the request to accept expired or was rejected
                            if (req_to_accept.timestamp + ACCEPT_DEADLINE_SECONDS >= timestamp)
                                && !blockchain
                                    .block_with_operation_code_exists(BillOpCode::RejectToAccept)
                            {
                                return Err(
                                    Error::BillRequestToAcceptDidNotExpireAndWasNotRejected,
                                );
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
                            if let Ok(true) = self.store.is_paid(&bill.id).await {
                                return Err(Error::BillAlreadyPaid);
                            }
                            // only if the request to pay expired or was rejected
                            if (req_to_pay.timestamp + PAYMENT_DEADLINE_SECONDS >= timestamp)
                                && !blockchain
                                    .block_with_operation_code_exists(BillOpCode::RejectToPay)
                            {
                                return Err(Error::BillRequestToPayDidNotExpireAndWasNotRejected);
                            }
                        } else {
                            return Err(Error::BillWasNotRequestedToPay);
                        }
                    }
                };
            }
            BillAction::Recourse(recoursee, sum, currency) => {
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay(&bill.id, blockchain, timestamp)
                    .await?;
                // not waiting for offer to sell
                self.bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;

                if let RecourseWaitingForPayment::Yes(payment_info) = blockchain
                    .is_last_request_to_recourse_block_waiting_for_payment(bill_keys, timestamp)?
                {
                    if payment_info.sum != *sum
                        || payment_info.currency != *currency
                        || payment_info.recoursee.node_id != recoursee.node_id
                        || payment_info.recourser.node_id != signer_node_id
                    {
                        return Err(Error::BillRecourseDataInvalid);
                    }

                    // the caller has to be the bill holder
                    if signer_node_id != *holder_node_id {
                        return Err(Error::CallerIsNotHolder);
                    }
                } else {
                    return Err(Error::BillIsNotRequestedToRecourseAndWaitingForPayment);
                }
            }
            BillAction::Mint(_, _, _) => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // the bill has to have been accepted
                if !blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                    return Err(Error::BillNotAccepted);
                }
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }
            }
            BillAction::OfferToSell(_, _, _) => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }
            }
            BillAction::Sell(buyer, sum, currency, payment_address) => {
                // not in recourse
                self.bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay(&bill.id, blockchain, timestamp)
                    .await?;

                if let Ok(OfferToSellWaitingForPayment::Yes(payment_info)) =
                    blockchain.is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)
                {
                    if payment_info.sum != *sum
                        || payment_info.currency != *currency
                        || payment_info.payment_address != *payment_address
                        || payment_info.buyer.node_id != buyer.node_id
                        || payment_info.seller.node_id != signer_node_id
                    {
                        return Err(Error::BillSellDataInvalid);
                    }
                    // the caller has to be the bill holder
                    if signer_node_id != *holder_node_id {
                        return Err(Error::CallerIsNotHolder);
                    }
                } else {
                    return Err(Error::BillIsNotOfferToSellWaitingForPayment);
                }
            }
            BillAction::Endorse(_) => {
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(Error::CallerIsNotHolder);
                }
            }
            BillAction::RejectAcceptance => {
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToAccept == *blockchain.get_latest_block().op_code() {
                    return Err(Error::RequestAlreadyRejected);
                }
                self.bill_is_blocked(&bill.id, blockchain, bill_keys, timestamp)
                    .await?;
                // caller has to be the drawee
                if signer_node_id != bill.drawee.node_id {
                    return Err(Error::CallerIsNotDrawee);
                }
                // there is not allowed to be an accept block
                if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                    return Err(Error::BillAlreadyAccepted);
                }
            }
            BillAction::RejectBuying => {
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToBuy == *blockchain.get_latest_block().op_code() {
                    return Err(Error::RequestAlreadyRejected);
                }
                // not in recourse
                self.bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay(&bill.id, blockchain, timestamp)
                    .await?;
                // there has to be a offer to sell block that is not expired
                if let OfferToSellWaitingForPayment::Yes(payment_info) = blockchain
                    .is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)?
                {
                    // caller has to be buyer of the offer to sell
                    if signer_node_id != payment_info.buyer.node_id {
                        return Err(Error::CallerIsNotBuyer);
                    }
                } else {
                    return Err(Error::BillWasNotOfferedToSell);
                }
            }
            BillAction::RejectPayment => {
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToPay == *blockchain.get_latest_block().op_code() {
                    return Err(Error::RequestAlreadyRejected);
                }
                // not waiting for offer to sell
                self.bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
                // not in recourse
                self.bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
                // caller has to be the drawee
                if signer_node_id != bill.drawee.node_id {
                    return Err(Error::CallerIsNotDrawee);
                }
                // bill is not paid already
                if let Ok(true) = self.store.is_paid(&bill.id).await {
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
            BillAction::RejectPaymentForRecourse => {
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToPayRecourse == *blockchain.get_latest_block().op_code() {
                    return Err(Error::RequestAlreadyRejected);
                }
                // not offered to sell
                self.bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
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
                        req_to_recourse.get_decrypted_block_bytes(bill_keys)?;
                    if signer_node_id != block_data.recoursee.node_id {
                        return Err(Error::CallerIsNotRecoursee);
                    }
                } else {
                    return Err(Error::BillWasNotRequestedToRecourse);
                }
            }
        };
        Ok(())
    }

    async fn bill_is_blocked(
        &self,
        bill_id: &str,
        blockchain: &BillBlockchain,
        bill_keys: &BillKeys,
        timestamp: u64,
    ) -> Result<()> {
        // not waiting for req to pay
        self.bill_waiting_for_req_to_pay(bill_id, blockchain, timestamp)
            .await?;
        // not offered to sell
        self.bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
        // not in recourse
        self.bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
        Ok(())
    }

    fn bill_waiting_for_offer_to_sell(
        &self,
        blockchain: &BillBlockchain,
        bill_keys: &BillKeys,
        timestamp: u64,
    ) -> Result<()> {
        if let OfferToSellWaitingForPayment::Yes(_) =
            blockchain.is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)?
        {
            return Err(Error::BillIsOfferedToSellAndWaitingForPayment);
        }
        Ok(())
    }

    fn bill_waiting_for_recourse_payment(
        &self,
        blockchain: &BillBlockchain,
        bill_keys: &BillKeys,
        timestamp: u64,
    ) -> Result<()> {
        if let RecourseWaitingForPayment::Yes(_) = blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(bill_keys, timestamp)?
        {
            return Err(Error::BillIsInRecourseAndWaitingForPayment);
        }
        Ok(())
    }

    async fn bill_waiting_for_req_to_pay(
        &self,
        bill_id: &str,
        blockchain: &BillBlockchain,
        timestamp: u64,
    ) -> Result<()> {
        if blockchain.get_latest_block().op_code == BillOpCode::RequestToPay {
            if let Some(req_to_pay) =
                blockchain.get_last_version_block_with_op_code(BillOpCode::RequestToPay)
            {
                let paid = self.store.is_paid(bill_id).await?;
                if !paid && req_to_pay.timestamp + PAYMENT_DEADLINE_SECONDS >= timestamp {
                    return Err(Error::BillIsRequestedToPayAndWaitingForPayment);
                }
            }
        }
        Ok(())
    }
}
