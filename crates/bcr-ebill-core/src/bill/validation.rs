use crate::{
    ValidationError,
    blockchain::{
        Block, Blockchain,
        bill::{
            BillBlockchain, BillOpCode, OfferToSellWaitingForPayment, RecourseWaitingForPayment,
            block::BillRequestRecourseBlockData,
        },
    },
    constants::{ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS},
    util,
};

use super::{BillAction, BillKeys, BillType, BitcreditBill, RecourseReason};

/// Generic result type
pub type Result<T> = std::result::Result<T, ValidationError>;

pub fn validate_bill_issue(
    sum: &str,
    file_upload_ids: &Vec<String>,
    issue_date: &str,
    maturity_date: &str,
    drawee: &str,
    payee: &str,
    t: u64,
) -> Result<(u64, BillType)> {
    let sum = util::currency::parse_sum(sum).map_err(|_| ValidationError::InvalidSum)?;

    for file_upload_id in file_upload_ids {
        util::validate_file_upload_id(Some(file_upload_id))?;
    }

    util::date::date_string_to_timestamp(issue_date, None)?;
    util::date::date_string_to_timestamp(maturity_date, None)?;

    let bill_type = match t {
        0 => BillType::PromissoryNote,
        1 => BillType::SelfDrafted,
        2 => BillType::ThreeParties,
        _ => return Err(ValidationError::InvalidBillType),
    };

    if drawee == payee {
        return Err(ValidationError::DraweeCantBePayee);
    }
    Ok((sum, bill_type))
}

pub async fn validate_bill_action(
    blockchain: &BillBlockchain,
    bill: &BitcreditBill,
    bill_keys: &BillKeys,
    timestamp: u64,
    signer_node_id: &str,
    bill_action: &BillAction,
    is_paid: bool,
) -> Result<()> {
    let holder_node_id = match bill.endorsee {
        None => &bill.payee.node_id,
        Some(ref endorsee) => &endorsee.node_id,
    };

    // If the bill was paid, no further actions are allowed
    if is_paid {
        return Err(ValidationError::BillAlreadyPaid);
    }

    match bill_action {
        BillAction::Accept => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // not already accepted
            if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                return Err(ValidationError::BillAlreadyAccepted);
            }
            // signer is drawee
            if !bill.drawee.node_id.eq(signer_node_id) {
                return Err(ValidationError::CallerIsNotDrawee);
            }
        }
        BillAction::RequestAcceptance => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // not already accepted
            if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                return Err(ValidationError::BillAlreadyAccepted);
            }
            // not already requested to accept
            if blockchain.block_with_operation_code_exists(BillOpCode::RequestToAccept) {
                return Err(ValidationError::BillAlreadyRequestedToAccept);
            }
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }
        }
        BillAction::RequestToPay(_) => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // not already requested to pay
            if blockchain.block_with_operation_code_exists(BillOpCode::RequestToPay) {
                return Err(ValidationError::BillWasRequestedToPay);
            }
            // maturity date must have started
            let maturity_date_start =
                util::date::date_string_to_timestamp(&bill.maturity_date, None)?;
            if timestamp < maturity_date_start {
                return Err(ValidationError::BillRequestedToPayBeforeMaturityDate);
            }
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }
        }
        BillAction::RequestRecourse(recoursee, recourse_reason) => {
            let past_holders = blockchain.get_past_endorsees_for_bill(bill_keys, signer_node_id)?;

            // validation
            if !past_holders
                .iter()
                .any(|h| h.pay_to_the_order_of.node_id == recoursee.node_id)
            {
                return Err(ValidationError::RecourseeNotPastHolder);
            }

            // not blocked
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }

            match recourse_reason {
                RecourseReason::Accept => {
                    if let Some(req_to_accept) =
                        blockchain.get_last_version_block_with_op_code(BillOpCode::RejectToAccept)
                    {
                        // only if the request to accept expired or was rejected
                        if !util::date::check_if_deadline_has_passed(
                            req_to_accept.timestamp,
                            timestamp,
                            ACCEPT_DEADLINE_SECONDS,
                        ) && !blockchain
                            .block_with_operation_code_exists(BillOpCode::RejectToAccept)
                        {
                            return Err(
                                ValidationError::BillRequestToAcceptDidNotExpireAndWasNotRejected,
                            );
                        }
                    } else {
                        return Err(ValidationError::BillWasNotRequestedToAccept);
                    }
                }
                RecourseReason::Pay(_, _) => {
                    if let Some(req_to_pay) =
                        blockchain.get_last_version_block_with_op_code(BillOpCode::RejectToPay)
                    {
                        // only if the bill is not paid already
                        if is_paid {
                            return Err(ValidationError::BillAlreadyPaid);
                        }

                        // only if the request to pay expired or was rejected
                        let deadline_base = get_deadline_base_for_req_to_pay(
                            req_to_pay.timestamp,
                            &bill.maturity_date,
                        )?;
                        if !util::date::check_if_deadline_has_passed(
                            deadline_base,
                            timestamp,
                            PAYMENT_DEADLINE_SECONDS,
                        ) && !blockchain
                            .block_with_operation_code_exists(BillOpCode::RejectToPay)
                        {
                            return Err(
                                ValidationError::BillRequestToPayDidNotExpireAndWasNotRejected,
                            );
                        }
                    } else {
                        return Err(ValidationError::BillWasNotRequestedToPay);
                    }
                }
            };
        }
        BillAction::Recourse(recoursee, sum, currency) => {
            // not waiting for req to pay
            bill_waiting_for_req_to_pay(blockchain, bill, timestamp, is_paid).await?;
            // not waiting for offer to sell
            bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;

            if let RecourseWaitingForPayment::Yes(payment_info) = blockchain
                .is_last_request_to_recourse_block_waiting_for_payment(bill_keys, timestamp)?
            {
                if payment_info.sum != *sum
                    || payment_info.currency != *currency
                    || payment_info.recoursee.node_id != recoursee.node_id
                    || payment_info.recourser.node_id != signer_node_id
                {
                    return Err(ValidationError::BillRecourseDataInvalid);
                }

                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            } else {
                return Err(ValidationError::BillIsNotRequestedToRecourseAndWaitingForPayment);
            }
        }
        BillAction::Mint(_, _, _) => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // the bill has to have been accepted
            if !blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                return Err(ValidationError::BillNotAccepted);
            }
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }
        }
        BillAction::OfferToSell(_, _, _) => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }
        }
        BillAction::Sell(buyer, sum, currency, payment_address) => {
            // not in recourse
            bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
            // not waiting for req to pay
            bill_waiting_for_req_to_pay(blockchain, bill, timestamp, is_paid).await?;

            if let Ok(OfferToSellWaitingForPayment::Yes(payment_info)) =
                blockchain.is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)
            {
                if payment_info.sum != *sum
                    || payment_info.currency != *currency
                    || payment_info.payment_address != *payment_address
                    || payment_info.buyer.node_id != buyer.node_id
                    || payment_info.seller.node_id != signer_node_id
                {
                    return Err(ValidationError::BillSellDataInvalid);
                }
                // the caller has to be the bill holder
                if signer_node_id != *holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            } else {
                return Err(ValidationError::BillIsNotOfferToSellWaitingForPayment);
            }
        }
        BillAction::Endorse(_) => {
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // the caller has to be the bill holder
            if signer_node_id != *holder_node_id {
                return Err(ValidationError::CallerIsNotHolder);
            }
        }
        BillAction::RejectAcceptance => {
            // if the op was already rejected, can't reject again
            if BillOpCode::RejectToAccept == *blockchain.get_latest_block().op_code() {
                return Err(ValidationError::RequestAlreadyRejected);
            }
            bill_is_blocked(blockchain, bill, bill_keys, timestamp, is_paid).await?;
            // caller has to be the drawee
            if signer_node_id != bill.drawee.node_id {
                return Err(ValidationError::CallerIsNotDrawee);
            }
            // there is not allowed to be an accept block
            if blockchain.block_with_operation_code_exists(BillOpCode::Accept) {
                return Err(ValidationError::BillAlreadyAccepted);
            }
        }
        BillAction::RejectBuying => {
            // if the op was already rejected, can't reject again
            if BillOpCode::RejectToBuy == *blockchain.get_latest_block().op_code() {
                return Err(ValidationError::RequestAlreadyRejected);
            }
            // not in recourse
            bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
            // not waiting for req to pay
            bill_waiting_for_req_to_pay(blockchain, bill, timestamp, is_paid).await?;
            // there has to be a offer to sell block that is not expired
            if let OfferToSellWaitingForPayment::Yes(payment_info) =
                blockchain.is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)?
            {
                // caller has to be buyer of the offer to sell
                if signer_node_id != payment_info.buyer.node_id {
                    return Err(ValidationError::CallerIsNotBuyer);
                }
            } else {
                return Err(ValidationError::BillWasNotOfferedToSell);
            }
        }
        BillAction::RejectPayment => {
            // if the op was already rejected, can't reject again
            if BillOpCode::RejectToPay == *blockchain.get_latest_block().op_code() {
                return Err(ValidationError::RequestAlreadyRejected);
            }
            // not waiting for offer to sell
            bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
            // not in recourse
            bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
            // caller has to be the drawee
            if signer_node_id != bill.drawee.node_id {
                return Err(ValidationError::CallerIsNotDrawee);
            }
            // bill is not paid already
            if is_paid {
                return Err(ValidationError::BillAlreadyPaid);
            }
            // there has to be a request to pay block that is not expired
            if let Some(req_to_pay) =
                blockchain.get_last_version_block_with_op_code(BillOpCode::RequestToPay)
            {
                let deadline_base =
                    get_deadline_base_for_req_to_pay(req_to_pay.timestamp, &bill.maturity_date)?;
                if util::date::check_if_deadline_has_passed(
                    deadline_base,
                    timestamp,
                    PAYMENT_DEADLINE_SECONDS,
                ) {
                    return Err(ValidationError::RequestAlreadyExpired);
                }
            } else {
                return Err(ValidationError::BillWasNotRequestedToPay);
            }
        }
        BillAction::RejectPaymentForRecourse => {
            // if the op was already rejected, can't reject again
            if BillOpCode::RejectToPayRecourse == *blockchain.get_latest_block().op_code() {
                return Err(ValidationError::RequestAlreadyRejected);
            }
            // not offered to sell
            bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
            // there has to be a request to recourse that is not expired
            if let Some(req_to_recourse) =
                blockchain.get_last_version_block_with_op_code(BillOpCode::RequestRecourse)
            {
                // has to be the last block
                if blockchain.get_latest_block().id != req_to_recourse.id {
                    return Err(ValidationError::BillWasNotRequestedToRecourse);
                }
                if util::date::check_if_deadline_has_passed(
                    req_to_recourse.timestamp,
                    timestamp,
                    RECOURSE_DEADLINE_SECONDS,
                ) {
                    return Err(ValidationError::RequestAlreadyExpired);
                }
                // caller has to be recoursee of the request to recourse block
                let block_data: BillRequestRecourseBlockData =
                    req_to_recourse.get_decrypted_block_bytes(bill_keys)?;
                if signer_node_id != block_data.recoursee.node_id {
                    return Err(ValidationError::CallerIsNotRecoursee);
                }
            } else {
                return Err(ValidationError::BillWasNotRequestedToRecourse);
            }
        }
    };
    Ok(())
}

/// calculates the base for the expiration deadline of a request to pay - if it was before the
/// maturity date, we take the end of the day of the maturity date, otherwise the req to pay
/// timestamp
pub fn get_deadline_base_for_req_to_pay(
    req_to_pay_ts: u64,
    bill_maturity_date: &str,
) -> Result<u64> {
    let maturity_date = util::date::date_string_to_timestamp(bill_maturity_date, None)?;
    let maturity_date_end_of_day = util::date::end_of_day_as_timestamp(maturity_date);
    let mut deadline_base = req_to_pay_ts;
    // requested to pay after maturity date - deadline base is req to pay
    if deadline_base < maturity_date_end_of_day {
        // requested to pay before end of maturity date - deadline base is maturity
        // date end of day
        deadline_base = maturity_date_end_of_day;
    }
    Ok(deadline_base)
}

async fn bill_is_blocked(
    blockchain: &BillBlockchain,
    bill: &BitcreditBill,
    bill_keys: &BillKeys,
    timestamp: u64,
    is_paid: bool,
) -> Result<()> {
    // not waiting for req to pay
    bill_waiting_for_req_to_pay(blockchain, bill, timestamp, is_paid).await?;
    // not offered to sell
    bill_waiting_for_offer_to_sell(blockchain, bill_keys, timestamp)?;
    // not in recourse
    bill_waiting_for_recourse_payment(blockchain, bill_keys, timestamp)?;
    Ok(())
}

fn bill_waiting_for_offer_to_sell(
    blockchain: &BillBlockchain,
    bill_keys: &BillKeys,
    timestamp: u64,
) -> Result<()> {
    if let OfferToSellWaitingForPayment::Yes(_) =
        blockchain.is_last_offer_to_sell_block_waiting_for_payment(bill_keys, timestamp)?
    {
        return Err(ValidationError::BillIsOfferedToSellAndWaitingForPayment);
    }
    Ok(())
}

fn bill_waiting_for_recourse_payment(
    blockchain: &BillBlockchain,
    bill_keys: &BillKeys,
    timestamp: u64,
) -> Result<()> {
    if let RecourseWaitingForPayment::Yes(_) =
        blockchain.is_last_request_to_recourse_block_waiting_for_payment(bill_keys, timestamp)?
    {
        return Err(ValidationError::BillIsInRecourseAndWaitingForPayment);
    }
    Ok(())
}

async fn bill_waiting_for_req_to_pay(
    blockchain: &BillBlockchain,
    bill: &BitcreditBill,
    timestamp: u64,
    is_paid: bool,
) -> Result<()> {
    if blockchain.get_latest_block().op_code == BillOpCode::RequestToPay {
        if let Some(req_to_pay) =
            blockchain.get_last_version_block_with_op_code(BillOpCode::RequestToPay)
        {
            let deadline_base =
                get_deadline_base_for_req_to_pay(req_to_pay.timestamp, &bill.maturity_date)?;
            if !is_paid
                && !util::date::check_if_deadline_has_passed(
                    deadline_base,
                    timestamp,
                    PAYMENT_DEADLINE_SECONDS,
                )
            {
                return Err(ValidationError::BillIsRequestedToPayAndWaitingForPayment);
            }
        }
    }
    Ok(())
}
