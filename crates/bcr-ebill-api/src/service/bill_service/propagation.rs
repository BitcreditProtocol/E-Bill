use super::{BillAction, Result, service::BillService};
use bcr_ebill_core::{
    bill::{BillKeys, RecourseReason},
    blockchain::bill::{BillBlock, BillBlockchain},
    identity::Identity,
    notification::ActionType,
};
use bcr_ebill_transport::BillChainEvent;

impl BillService {
    pub(super) async fn notify_for_block_action(
        &self,
        blockchain: &BillBlockchain,
        bill_keys: &BillKeys,
        bill_action: &BillAction,
        identity: &Identity,
    ) -> Result<()> {
        let last_version_bill = self
            .get_last_version_bill(blockchain, bill_keys, identity)
            .await?;

        let chain_event = BillChainEvent::new(&last_version_bill, blockchain, bill_keys, true)?;

        match bill_action {
            BillAction::Accept => {
                self.notification_service
                    .send_bill_is_accepted_event(&chain_event)
                    .await?;
            }
            BillAction::RequestAcceptance => {
                self.notification_service
                    .send_request_to_accept_event(&chain_event)
                    .await?;
            }
            BillAction::RequestToPay(_) => {
                self.notification_service
                    .send_request_to_pay_event(&chain_event)
                    .await?;
            }
            BillAction::RequestRecourse(recoursee, recourse_reason) => {
                let action_type = match recourse_reason {
                    RecourseReason::Accept => ActionType::AcceptBill,
                    RecourseReason::Pay(_, _) => ActionType::PayBill,
                };
                self.notification_service
                    .send_recourse_action_event(&chain_event, action_type, recoursee)
                    .await?;
            }
            BillAction::Recourse(recoursee, _, _) => {
                self.notification_service
                    .send_bill_recourse_paid_event(&chain_event, recoursee)
                    .await?;
            }
            BillAction::Mint(_, _, _) => {
                self.notification_service
                    .send_request_to_mint_event(&last_version_bill)
                    .await?;
            }
            BillAction::OfferToSell(buyer, _, _) => {
                self.notification_service
                    .send_offer_to_sell_event(&chain_event, buyer)
                    .await?;
            }
            BillAction::Sell(buyer, _, _, _) => {
                self.notification_service
                    .send_bill_is_sold_event(&chain_event, buyer)
                    .await?;
            }
            BillAction::Endorse(_) => {
                self.notification_service
                    .send_bill_is_endorsed_event(&chain_event)
                    .await?;
            }
            BillAction::RejectAcceptance => {
                self.notification_service
                    .send_request_to_action_rejected_event(&chain_event, ActionType::AcceptBill)
                    .await?;
            }
            BillAction::RejectBuying => {
                self.notification_service
                    .send_request_to_action_rejected_event(&&chain_event, ActionType::BuyBill)
                    .await?;
            }
            BillAction::RejectPayment => {
                self.notification_service
                    .send_request_to_action_rejected_event(&chain_event, ActionType::PayBill)
                    .await?;
            }
            BillAction::RejectPaymentForRecourse => {
                self.notification_service
                    .send_request_to_action_rejected_event(&chain_event, ActionType::RecourseBill)
                    .await?;
            }
        };
        Ok(())
    }

    pub(super) async fn propagate_block(&self, _bill_id: &str, _block: &BillBlock) -> Result<()> {
        // TODO NOSTR: propagate new block to bill topic
        Ok(())
    }

    pub(super) async fn propagate_bill_for_node_id(
        &self,
        _bill_id: &str,
        _node_id: &str,
    ) -> Result<()> {
        // TODO NOSTR: propagate bill to given node
        Ok(())
    }

    pub(super) async fn propagate_bill_and_subscribe(
        &self,
        _bill_id: &str,
        _drawer_node_id: &str,
        _drawee_node_id: &str,
        _payee_node_id: &str,
    ) -> Result<()> {
        // TODO NOSTR: propagate bill to participants
        // TODO NOSTR: subscribe to bill topic
        // TODO NOSTR: propagate data and uploaded files metadata
        Ok(())
    }
}
