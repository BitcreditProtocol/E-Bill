use crate::{
    Validate, ValidationError,
    blockchain::{
        Block, Blockchain,
        bill::{
            BillOpCode, OfferToSellWaitingForPayment, RecourseWaitingForPayment,
            block::{BillRecourseReasonBlockData, BillRequestRecourseBlockData},
        },
    },
    constants::{ACCEPT_DEADLINE_SECONDS, PAYMENT_DEADLINE_SECONDS, RECOURSE_DEADLINE_SECONDS},
    util,
};

use super::{BillAction, BillIssueData, BillType, BillValidateActionData, RecourseReason};

pub fn validate_bill_issue(data: &BillIssueData) -> Result<(u64, BillType), ValidationError> {
    let sum = util::currency::parse_sum(&data.sum).map_err(|_| ValidationError::InvalidSum)?;

    for file_upload_id in &data.file_upload_ids {
        util::validate_file_upload_id(Some(file_upload_id))?;
    }

    if util::crypto::validate_pub_key(&data.drawee).is_err() {
        return Err(ValidationError::InvalidSecp256k1Key(data.drawee.clone()));
    }

    if util::crypto::validate_pub_key(&data.payee).is_err() {
        return Err(ValidationError::InvalidSecp256k1Key(data.payee.clone()));
    }

    if util::crypto::validate_pub_key(&data.drawer_public_data.node_id).is_err() {
        return Err(ValidationError::InvalidSecp256k1Key(
            data.drawer_public_data.node_id.clone(),
        ));
    }

    util::date::date_string_to_timestamp(&data.issue_date, None)?;
    util::date::date_string_to_timestamp(&data.maturity_date, None)?;

    let bill_type = match data.t {
        0 => BillType::PromissoryNote,
        1 => BillType::SelfDrafted,
        2 => BillType::ThreeParties,
        _ => return Err(ValidationError::InvalidBillType),
    };

    if data.drawee == data.payee {
        return Err(ValidationError::DraweeCantBePayee);
    }
    Ok((sum, bill_type))
}

impl Validate for BillValidateActionData {
    fn validate(&self) -> Result<(), ValidationError> {
        let holder_node_id = match self.endorsee_node_id {
            None => self.payee_node_id.clone(),
            Some(ref endorsee) => endorsee.clone(),
        };

        // if the bill was rejected to recourse, no further actions are allowed
        if self
            .blockchain
            .block_with_operation_code_exists(BillOpCode::RejectToPayRecourse)
        {
            return Err(ValidationError::BillWasRejectedToRecourse);
        }

        // if the bill was recoursed and there are no past endorsees to recourse against anymore,
        // no further actions are allowed
        if self
            .blockchain
            .block_with_operation_code_exists(BillOpCode::Recourse)
        {
            let past_holders = self
                .blockchain
                .get_past_endorsees_for_bill(&self.bill_keys, &self.signer_node_id)?;
            if past_holders.is_empty() {
                return Err(ValidationError::BillWasRecoursedToTheEnd);
            }
        }

        // if the last block is req to recourse and it expired, no further actions are allowed
        if let Some(req_to_recourse) = self
            .blockchain
            .get_last_version_block_with_op_code(BillOpCode::RequestRecourse)
        {
            if BillOpCode::RequestRecourse == *self.blockchain.get_latest_block().op_code()
                && util::date::check_if_deadline_has_passed(
                    req_to_recourse.timestamp,
                    self.timestamp,
                    RECOURSE_DEADLINE_SECONDS,
                )
            {
                return Err(ValidationError::BillRequestToRecourseExpired);
            }
        }

        // If the bill was paid, no further actions are allowed
        if self.is_paid {
            return Err(ValidationError::BillAlreadyPaid);
        }

        match &self.bill_action {
            BillAction::Accept => {
                self.bill_is_blocked()?;
                self.bill_can_only_be_recoursed()?;
                // not already accepted
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::Accept)
                {
                    return Err(ValidationError::BillAlreadyAccepted);
                }
                // signer is drawee
                if !self.drawee_node_id.eq(&self.signer_node_id) {
                    return Err(ValidationError::CallerIsNotDrawee);
                }
            }
            BillAction::RequestAcceptance => {
                self.bill_is_blocked()?;
                self.bill_can_only_be_recoursed()?;
                // not already accepted
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::Accept)
                {
                    return Err(ValidationError::BillAlreadyAccepted);
                }
                // not already requested to accept
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::RequestToAccept)
                {
                    return Err(ValidationError::BillAlreadyRequestedToAccept);
                }
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            }
            BillAction::RequestToPay(_) => {
                self.bill_is_blocked()?;
                self.bill_can_only_be_recoursed()?;
                // not already requested to pay
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::RequestToPay)
                {
                    return Err(ValidationError::BillWasRequestedToPay);
                }
                // maturity date must have started
                let maturity_date_start =
                    util::date::date_string_to_timestamp(&self.maturity_date, None)?;
                if self.timestamp < maturity_date_start {
                    return Err(ValidationError::BillRequestedToPayBeforeMaturityDate);
                }
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            }
            BillAction::RequestRecourse(recoursee, recourse_reason) => {
                let past_holders = self
                    .blockchain
                    .get_past_endorsees_for_bill(&self.bill_keys, &self.signer_node_id)?;

                // validation
                if !past_holders
                    .iter()
                    .any(|h| h.pay_to_the_order_of.node_id == recoursee.node_id)
                {
                    return Err(ValidationError::RecourseeNotPastHolder);
                }

                // not blocked
                self.bill_is_blocked()?;
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }

                match recourse_reason {
                    RecourseReason::Accept => {
                        if let Some(req_to_accept) = self
                            .blockchain
                            .get_last_version_block_with_op_code(BillOpCode::RejectToAccept)
                        {
                            // only if the request to accept expired or was rejected
                            if !util::date::check_if_deadline_has_passed(
                                req_to_accept.timestamp,
                                self.timestamp,
                                ACCEPT_DEADLINE_SECONDS,
                            ) && !self
                                .blockchain
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
                        if let Some(req_to_pay) = self
                            .blockchain
                            .get_last_version_block_with_op_code(BillOpCode::RejectToPay)
                        {
                            // only if the bill is not paid already
                            if self.is_paid {
                                return Err(ValidationError::BillAlreadyPaid);
                            }

                            // only if the request to pay expired or was rejected
                            let deadline_base = get_deadline_base_for_req_to_pay(
                                req_to_pay.timestamp,
                                &self.maturity_date,
                            )?;
                            if !util::date::check_if_deadline_has_passed(
                                deadline_base,
                                self.timestamp,
                                PAYMENT_DEADLINE_SECONDS,
                            ) && !self
                                .blockchain
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
            BillAction::Recourse(recoursee, sum, currency, reason) => {
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay()?;
                // not waiting for offer to sell
                self.bill_waiting_for_offer_to_sell()?;
                let recourse_reason = match reason {
                    RecourseReason::Pay(_, _) => BillRecourseReasonBlockData::Pay,
                    RecourseReason::Accept => BillRecourseReasonBlockData::Accept,
                };

                if let RecourseWaitingForPayment::Yes(payment_info) = self
                    .blockchain
                    .is_last_request_to_recourse_block_waiting_for_payment(
                        &self.bill_keys,
                        self.timestamp,
                    )?
                {
                    if payment_info.sum != *sum
                        || payment_info.currency != *currency
                        || payment_info.recoursee.node_id != recoursee.node_id
                        || payment_info.recourser.node_id != self.signer_node_id
                        || payment_info.reason != recourse_reason
                    {
                        return Err(ValidationError::BillRecourseDataInvalid);
                    }

                    // the caller has to be the bill holder
                    if self.signer_node_id != holder_node_id {
                        return Err(ValidationError::CallerIsNotHolder);
                    }
                } else {
                    return Err(ValidationError::BillIsNotRequestedToRecourseAndWaitingForPayment);
                }
            }
            BillAction::Mint(_, _, _) => {
                self.bill_is_blocked()?;
                self.bill_can_only_be_recoursed()?;
                // the bill has to have been accepted
                if !self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::Accept)
                {
                    return Err(ValidationError::BillNotAccepted);
                }
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            }
            BillAction::OfferToSell(_, _, _) => {
                self.bill_is_blocked()?;
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            }
            BillAction::Sell(buyer, sum, currency, payment_address) => {
                self.bill_can_only_be_recoursed()?;
                // not in recourse
                self.bill_waiting_for_recourse_payment()?;
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay()?;

                if let Ok(OfferToSellWaitingForPayment::Yes(payment_info)) = self
                    .blockchain
                    .is_last_offer_to_sell_block_waiting_for_payment(
                        &self.bill_keys,
                        self.timestamp,
                    )
                {
                    if payment_info.sum != *sum
                        || payment_info.currency != *currency
                        || payment_info.payment_address != *payment_address
                        || payment_info.buyer.node_id != buyer.node_id
                        || payment_info.seller.node_id != self.signer_node_id
                    {
                        return Err(ValidationError::BillSellDataInvalid);
                    }
                    // the caller has to be the bill holder
                    if self.signer_node_id != holder_node_id {
                        return Err(ValidationError::CallerIsNotHolder);
                    }
                } else {
                    return Err(ValidationError::BillIsNotOfferToSellWaitingForPayment);
                }
            }
            BillAction::Endorse(_) => {
                self.bill_can_only_be_recoursed()?;
                self.bill_is_blocked()?;
                // the caller has to be the bill holder
                if self.signer_node_id != holder_node_id {
                    return Err(ValidationError::CallerIsNotHolder);
                }
            }
            BillAction::RejectAcceptance => {
                self.bill_can_only_be_recoursed()?;
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToAccept == *self.blockchain.get_latest_block().op_code() {
                    return Err(ValidationError::RequestAlreadyRejected);
                }
                self.bill_is_blocked()?;
                // caller has to be the drawee
                if self.signer_node_id != self.drawee_node_id {
                    return Err(ValidationError::CallerIsNotDrawee);
                }
                // there is not allowed to be an accept block
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::Accept)
                {
                    return Err(ValidationError::BillAlreadyAccepted);
                }
            }
            BillAction::RejectBuying => {
                self.bill_can_only_be_recoursed()?;
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToBuy == *self.blockchain.get_latest_block().op_code() {
                    return Err(ValidationError::RequestAlreadyRejected);
                }
                // not in recourse
                self.bill_waiting_for_recourse_payment()?;
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay()?;
                // there has to be a offer to sell block that is not expired
                if let OfferToSellWaitingForPayment::Yes(payment_info) = self
                    .blockchain
                    .is_last_offer_to_sell_block_waiting_for_payment(
                        &self.bill_keys,
                        self.timestamp,
                    )?
                {
                    // caller has to be buyer of the offer to sell
                    if self.signer_node_id != payment_info.buyer.node_id {
                        return Err(ValidationError::CallerIsNotBuyer);
                    }
                } else {
                    return Err(ValidationError::BillWasNotOfferedToSell);
                }
            }
            BillAction::RejectPayment => {
                self.bill_can_only_be_recoursed()?;
                // if the op was already rejected, can't reject again
                if BillOpCode::RejectToPay == *self.blockchain.get_latest_block().op_code() {
                    return Err(ValidationError::RequestAlreadyRejected);
                }
                // not waiting for offer to sell
                self.bill_waiting_for_offer_to_sell()?;
                // not in recourse
                self.bill_waiting_for_recourse_payment()?;
                // caller has to be the drawee
                if self.signer_node_id != self.drawee_node_id {
                    return Err(ValidationError::CallerIsNotDrawee);
                }
                // bill is not paid already
                if self.is_paid {
                    return Err(ValidationError::BillAlreadyPaid);
                }
                // there has to be a request to pay block that is not expired
                if let Some(req_to_pay) = self
                    .blockchain
                    .get_last_version_block_with_op_code(BillOpCode::RequestToPay)
                {
                    let deadline_base = get_deadline_base_for_req_to_pay(
                        req_to_pay.timestamp,
                        &self.maturity_date,
                    )?;
                    if util::date::check_if_deadline_has_passed(
                        deadline_base,
                        self.timestamp,
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
                if BillOpCode::RejectToPayRecourse == *self.blockchain.get_latest_block().op_code()
                {
                    return Err(ValidationError::RequestAlreadyRejected);
                }
                // not offered to sell
                self.bill_waiting_for_offer_to_sell()?;
                // not waiting for req to pay
                self.bill_waiting_for_req_to_pay()?;
                // there has to be a request to recourse that is not expired
                if let Some(req_to_recourse) = self
                    .blockchain
                    .get_last_version_block_with_op_code(BillOpCode::RequestRecourse)
                {
                    // has to be the last block
                    if self.blockchain.get_latest_block().id != req_to_recourse.id {
                        return Err(ValidationError::BillWasNotRequestedToRecourse);
                    }
                    if util::date::check_if_deadline_has_passed(
                        req_to_recourse.timestamp,
                        self.timestamp,
                        RECOURSE_DEADLINE_SECONDS,
                    ) {
                        return Err(ValidationError::RequestAlreadyExpired);
                    }
                    // caller has to be recoursee of the request to recourse block
                    let block_data: BillRequestRecourseBlockData =
                        req_to_recourse.get_decrypted_block_bytes(&self.bill_keys)?;
                    if self.signer_node_id != block_data.recoursee.node_id {
                        return Err(ValidationError::CallerIsNotRecoursee);
                    }
                } else {
                    return Err(ValidationError::BillWasNotRequestedToRecourse);
                }
            }
        };
        Ok(())
    }
}

/// calculates the base for the expiration deadline of a request to pay - if it was before the
/// maturity date, we take the end of the day of the maturity date, otherwise the req to pay
/// timestamp
pub fn get_deadline_base_for_req_to_pay(
    req_to_pay_ts: u64,
    bill_maturity_date: &str,
) -> Result<u64, ValidationError> {
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

impl BillValidateActionData {
    /// if the bill was rejected to accept, rejected to pay, or either of them expired, it can only
    /// be recoursed from that point on
    fn bill_can_only_be_recoursed(&self) -> Result<(), ValidationError> {
        match self.bill_action {
            BillAction::Recourse(_, _, _, _)
            | BillAction::RequestRecourse(_, _)
            | BillAction::RejectPaymentForRecourse => {
                // do nothing, these actions are fine
                Ok(())
            }
            _ => {
                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::RejectToAccept)
                {
                    return Err(ValidationError::BillWasRejectedToAccept);
                }

                if self
                    .blockchain
                    .block_with_operation_code_exists(BillOpCode::RejectToPay)
                {
                    return Err(ValidationError::BillWasRejectedToPay);
                }

                if let Some(req_to_pay_block) = self
                    .blockchain
                    .get_last_version_block_with_op_code(BillOpCode::RequestToPay)
                {
                    let deadline_base = get_deadline_base_for_req_to_pay(
                        req_to_pay_block.timestamp,
                        &self.maturity_date,
                    )?;
                    // not paid and not rejected (checked above)
                    if !self.is_paid
                        && util::date::check_if_deadline_has_passed(
                            deadline_base,
                            self.timestamp,
                            PAYMENT_DEADLINE_SECONDS,
                        )
                    {
                        return Err(ValidationError::BillPaymentExpired);
                    }
                }

                if let Some(req_to_accept_block) = self
                    .blockchain
                    .get_last_version_block_with_op_code(BillOpCode::RequestToAccept)
                {
                    let accepted = self
                        .blockchain
                        .block_with_operation_code_exists(BillOpCode::Accept);

                    // not accepted and not rejected (checked above)
                    if !accepted
                        && util::date::check_if_deadline_has_passed(
                            req_to_accept_block.timestamp,
                            self.timestamp,
                            ACCEPT_DEADLINE_SECONDS,
                        )
                    {
                        return Err(ValidationError::BillAcceptanceExpired);
                    }
                }

                Ok(())
            }
        }
    }

    /// if the bill is waiting for payment, it's blocked
    fn bill_is_blocked(&self) -> Result<(), ValidationError> {
        // not waiting for req to pay
        self.bill_waiting_for_req_to_pay()?;
        // not offered to sell
        self.bill_waiting_for_offer_to_sell()?;
        // not in recourse
        self.bill_waiting_for_recourse_payment()?;
        Ok(())
    }

    fn bill_waiting_for_offer_to_sell(&self) -> Result<(), ValidationError> {
        if let OfferToSellWaitingForPayment::Yes(_) = self
            .blockchain
            .is_last_offer_to_sell_block_waiting_for_payment(&self.bill_keys, self.timestamp)?
        {
            return Err(ValidationError::BillIsOfferedToSellAndWaitingForPayment);
        }
        Ok(())
    }

    fn bill_waiting_for_recourse_payment(&self) -> Result<(), ValidationError> {
        if let RecourseWaitingForPayment::Yes(_) = self
            .blockchain
            .is_last_request_to_recourse_block_waiting_for_payment(
                &self.bill_keys,
                self.timestamp,
            )?
        {
            return Err(ValidationError::BillIsInRecourseAndWaitingForPayment);
        }
        Ok(())
    }

    fn bill_waiting_for_req_to_pay(&self) -> Result<(), ValidationError> {
        if self.blockchain.get_latest_block().op_code == BillOpCode::RequestToPay {
            if let Some(req_to_pay) = self
                .blockchain
                .get_last_version_block_with_op_code(BillOpCode::RequestToPay)
            {
                let deadline_base =
                    get_deadline_base_for_req_to_pay(req_to_pay.timestamp, &self.maturity_date)?;
                if !self.is_paid
                    && !util::date::check_if_deadline_has_passed(
                        deadline_base,
                        self.timestamp,
                        PAYMENT_DEADLINE_SECONDS,
                    )
                {
                    return Err(ValidationError::BillIsRequestedToPayAndWaitingForPayment);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        bill::BillKeys,
        blockchain::bill::{
            BillBlockchain,
            block::{BillIssueBlockData, tests::valid_bill_issue_block_data},
        },
        contact::IdentityPublicData,
        tests::tests::{
            OTHER_TEST_PUB_KEY_SECP, TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP,
            valid_identity_public_data,
        },
        util::{BcrKeys, date::now},
    };

    use super::*;
    use rstest::rstest;

    fn valid_bill_issue_data() -> BillIssueData {
        BillIssueData {
            t: 0,
            country_of_issuing: "AT".into(),
            city_of_issuing: "Vienna".into(),
            issue_date: "2024-08-12".into(),
            maturity_date: "2024-11-12".into(),
            drawee: TEST_PUB_KEY_SECP.into(),
            payee: OTHER_TEST_PUB_KEY_SECP.into(),
            sum: "500".into(),
            currency: "sat".into(),
            country_of_payment: "FR".into(),
            city_of_payment: "Paris".into(),
            language: "de".into(),
            file_upload_ids: vec![],
            drawer_public_data: valid_identity_public_data(),
            drawer_keys: BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            timestamp: 1731593928,
        }
    }

    #[test]
    fn test_valid_bill_issue_data() {
        let result = validate_bill_issue(&valid_bill_issue_data());
        assert_eq!(result, Ok((500, BillType::PromissoryNote)));
    }

    #[rstest]
    #[case::invalid_sum( BillIssueData { sum: "invalidsum".into(), ..valid_bill_issue_data() }, ValidationError::InvalidSum)]
    #[case::invalid_file_id( BillIssueData { file_upload_ids: vec!["".into()], ..valid_bill_issue_data() }, ValidationError::InvalidFileUploadId)]
    #[case::invalid_issue_date( BillIssueData { issue_date: "invaliddate".into(), ..valid_bill_issue_data() }, ValidationError::InvalidDate)]
    #[case::invalid_maturity_date( BillIssueData { maturity_date: "invaliddate".into(), ..valid_bill_issue_data() }, ValidationError::InvalidDate)]
    #[case::invalid_bill_type( BillIssueData { t: 5, ..valid_bill_issue_data() }, ValidationError::InvalidBillType)]
    #[case::drawee_equals_payee( BillIssueData { drawee: TEST_PUB_KEY_SECP.into(), payee: TEST_PUB_KEY_SECP.into(), ..valid_bill_issue_data() }, ValidationError::DraweeCantBePayee)]
    #[case::invalid_payee( BillIssueData { payee: "invalidkey".into(), ..valid_bill_issue_data() }, ValidationError::InvalidSecp256k1Key("invalidkey".into()))]
    #[case::invalid_drawee( BillIssueData { drawee: "invalidkey".into(),  ..valid_bill_issue_data() }, ValidationError::InvalidSecp256k1Key("invalidkey".into()))]
    #[case::invalid_drawer( BillIssueData { drawer_public_data: IdentityPublicData { node_id: "invalidkey".into(), ..valid_identity_public_data() }, ..valid_bill_issue_data() }, ValidationError::InvalidSecp256k1Key("invalidkey".into()))]
    fn test_validate_bill_issue_data_errors(
        #[case] input: BillIssueData,
        #[case] expected: ValidationError,
    ) {
        assert_eq!(validate_bill_issue(&input), Err(expected));
    }

    fn valid_bill_blockchain_issue(issue_block_data: BillIssueBlockData) -> BillBlockchain {
        let chain = BillBlockchain::new(
            &issue_block_data,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            now().timestamp() as u64 - 10,
        )
        .unwrap();
        assert!(chain.is_chain_valid());
        chain
    }

    fn valid_bill_validate_action_data(chain: BillBlockchain) -> BillValidateActionData {
        BillValidateActionData {
            blockchain: chain,
            drawee_node_id: TEST_PUB_KEY_SECP.into(),
            payee_node_id: OTHER_TEST_PUB_KEY_SECP.into(),
            endorsee_node_id: None,
            maturity_date: "2024-11-12".into(),
            bill_keys: BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.into(),
                public_key: TEST_PUB_KEY_SECP.into(),
            },
            timestamp: now().timestamp() as u64,
            signer_node_id: TEST_PUB_KEY_SECP.into(),
            bill_action: BillAction::Accept,
            is_paid: false,
        }
    }

    #[rstest]
    #[case::is_paid(BillValidateActionData { is_paid: true, ..valid_bill_validate_action_data(valid_bill_blockchain_issue( valid_bill_issue_block_data(),)) }, Err(ValidationError::BillAlreadyPaid))]
    #[case::is_not_paid(BillValidateActionData { ..valid_bill_validate_action_data(valid_bill_blockchain_issue( valid_bill_issue_block_data(),)) }, Ok(()))]
    fn test_validate_bill_paid_or_not(
        #[case] input: BillValidateActionData,
        #[case] expected: Result<(), ValidationError>,
    ) {
        assert_eq!(input.validate(), expected);
    }
}
