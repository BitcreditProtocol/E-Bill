use crate::{Result, event::chain_event::BillChainEvent};
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::{
    bill::BitcreditBill,
    contact::{BillIdentifiedParticipant, BillParticipant},
    notification::{ActionType, Notification},
};
use bcr_ebill_persistence::notification::NotificationFilter;
#[cfg(test)]
use mockall::automock;
use std::collections::HashMap;

#[cfg(test)]
impl ServiceTraitBounds for MockNotificationServiceApi {}

/// Send events via all channels required for the event type.
#[allow(dead_code)]
#[cfg_attr(test, automock)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait NotificationServiceApi: ServiceTraitBounds {
    /// Sent when: A bill is signed by: Drawer
    /// Receiver: Payer, Action: AcceptBill
    /// Receiver: Payee, Action: CheckBill
    async fn send_bill_is_signed_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is accepted by: Payer
    /// Receiver: Holder, Action: CheckBill
    async fn send_bill_is_accepted_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is requested to be accepted, Sent by: Holder
    /// Receiver: Payer, Action: AcceptBill
    async fn send_request_to_accept_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is requested to be paid, Sent by: Holder
    /// Receiver: Payer, Action: PayBill
    async fn send_request_to_pay_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is paid by: Payer (Bitcoin API)
    /// Receiver: Payee, Action: CheckBill
    async fn send_bill_is_paid_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is endorsed by: Previous Holder
    /// Receiver: NewHolder, Action: CheckBill
    async fn send_bill_is_endorsed_event(&self, event: &BillChainEvent) -> Result<()>;

    /// Sent when: A bill is offered to be sold, Sent by: Holder
    /// Receiver: Buyer, Action: CheckBill (with buy page)
    async fn send_offer_to_sell_event(
        &self,
        event: &BillChainEvent,
        buyer: &BillParticipant,
    ) -> Result<()>;

    /// Sent when: A bill is sold by: Seller (old holder)
    /// Receiver: Buyer (new holder), Action: CheckBill
    async fn send_bill_is_sold_event(
        &self,
        event: &BillChainEvent,
        buyer: &BillParticipant,
    ) -> Result<()>;

    /// Sent when: A bill recourse was paid, by: Recourser (old holder)
    /// Receiver: Recoursee (new holder), Action: CheckBill
    async fn send_bill_recourse_paid_event(
        &self,
        event: &BillChainEvent,
        recoursee: &BillIdentifiedParticipant,
    ) -> Result<()>;

    /// In case a participant rejects one of the 'request to' actions (e.g. request to accept,
    /// request to pay) we send this event to all bill participants. Will only send the event
    /// if the given action can be a rejected action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * rejected_action: The action that was rejected
    /// * recipients: The list of recipients that should receive the notification
    async fn send_request_to_action_rejected_event(
        &self,
        event: &BillChainEvent,
        rejected_action: ActionType,
    ) -> Result<()>;

    /// In case a participant did not perform an action (e.g. request to accept, request
    /// to pay) in time we notify all bill participants about the timed out action. Will
    /// only send the event if the given action can be a timed out action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * timed_out_action: The action that has timed out
    /// * recipients: The list of recipients that should receive the notification
    async fn send_request_to_action_timed_out_event(
        &self,
        sender_node_id: &str,
        bill_id: &str,
        sum: Option<u64>,
        timed_out_action: ActionType,
        recipients: Vec<BillIdentifiedParticipant>,
    ) -> Result<()>;

    /// In case an action was rejected or timed out a holder can request a recourse action
    /// from another participant in the chain. Will only send the event if the given action
    /// can be a recourse action.
    /// Arguments:
    /// * bill_id: The id of the bill affected
    /// * action: The action that should be performed via recourse. This will also be the action
    /// sent in the event given it can be a recourse action.
    /// * recipient: The recourse recipient that should perform the action
    async fn send_recourse_action_event(
        &self,
        event: &BillChainEvent,
        action: ActionType,
        recoursee: &BillIdentifiedParticipant,
    ) -> Result<()>;

    /// Sent when: A bill is requested to be minted, Sent by: Holder
    /// Receiver: Mint, Action: CheckBill (with generate quote page)
    async fn send_request_to_mint_event(
        &self,
        sender_node_id: &str,
        bill: &BitcreditBill,
    ) -> Result<()>;

    /// Sent when: A new quote is created, Sent by: Mint
    /// Receiver: Holder, Action: Check quote page
    async fn send_new_quote_event(&self, quote: &BitcreditBill) -> Result<()>;

    /// Sent when: A quote is approved by: Previous Holder
    /// Receiver: Mint (new holder), Action: CheckBill
    async fn send_quote_is_approved_event(&self, quote: &BitcreditBill) -> Result<()>;

    /// Returns filtered client notifications
    async fn get_client_notifications(
        &self,
        filter: NotificationFilter,
    ) -> Result<Vec<Notification>>;

    /// Marks the notification with given id as done
    async fn mark_notification_as_done(&self, notification_id: &str) -> Result<()>;

    /// Returns the active bill notification for the given bill id
    async fn get_active_bill_notification(&self, bill_id: &str) -> Option<Notification>;

    async fn get_active_bill_notifications(
        &self,
        bill_ids: &[String],
    ) -> HashMap<String, Notification>;

    /// Returns whether a notification was already sent for the given bill id and action
    async fn check_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<bool>;

    /// Stores that a notification was sent for the given bill id and action
    async fn mark_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<()>;

    /// Retry sending a queued message to the given node id
    async fn send_retry_messages(&self) -> Result<()>;
}
