use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bcr_ebill_persistence::nostr::{NostrQueuedMessage, NostrQueuedMessageStoreApi};
use bcr_ebill_transport::{BillChainEvent, BillChainEventPayload, Error, Event, EventEnvelope};
use log::{error, warn};

use super::NotificationJsonTransportApi;
use super::{NotificationServiceApi, Result};
use crate::data::{
    bill::BitcreditBill,
    contact::IdentityPublicData,
    notification::{Notification, NotificationType},
};
use crate::persistence::notification::{NotificationFilter, NotificationStoreApi};
use crate::service::contact_service::ContactServiceApi;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::notification::{ActionType, BillEventType};

/// A default implementation of the NotificationServiceApi that can
/// send events via json and email transports.
#[allow(dead_code)]
pub struct DefaultNotificationService {
    notification_transport: Box<dyn NotificationJsonTransportApi>,
    notification_store: Arc<dyn NotificationStoreApi>,
    contact_service: Arc<dyn ContactServiceApi>,
    queued_message_store: Arc<dyn NostrQueuedMessageStoreApi>,
}

impl ServiceTraitBounds for DefaultNotificationService {}

impl DefaultNotificationService {
    // the number of times we want to retry sending a block message
    const NOSTR_MAX_RETRIES: i32 = 10;

    pub fn new(
        notification_transport: Box<dyn NotificationJsonTransportApi>,
        notification_store: Arc<dyn NotificationStoreApi>,
        contact_service: Arc<dyn ContactServiceApi>,
        queued_message_store: Arc<dyn NostrQueuedMessageStoreApi>,
    ) -> Self {
        Self {
            notification_transport,
            notification_store,
            contact_service,
            queued_message_store,
        }
    }

    async fn send_all_events(&self, events: Vec<Event<BillChainEventPayload>>) -> Result<()> {
        for event_to_process in events.into_iter() {
            if let Ok(Some(identity)) = self
                .contact_service
                .get_identity_by_node_id(&event_to_process.node_id)
                .await
            {
                if let Err(e) = self
                    .notification_transport
                    .send(&identity, event_to_process.clone().try_into()?)
                    .await
                {
                    error!(
                        "Failed to send block notification, will add it to retry queue: {}",
                        e
                    );
                    let queue_message = NostrQueuedMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        node_id: event_to_process.node_id.clone(),
                        payload: serde_json::to_value(event_to_process)?,
                    };
                    if let Err(e) = self
                        .queued_message_store
                        .add_message(queue_message, Self::NOSTR_MAX_RETRIES)
                        .await
                    {
                        error!("Failed to add block notification to retry queue: {}", e);
                    }
                }
            } else {
                warn!(
                    "Failed to find recipient in contacts for node_id: {}",
                    event_to_process.node_id
                );
            }
        }
        Ok(())
    }

    async fn send_retry_message(&self, node_id: &str, message: EventEnvelope) -> Result<()> {
        if let Ok(Some(identity)) = self.contact_service.get_identity_by_node_id(node_id).await {
            self.notification_transport.send(&identity, message).await?;
        }
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl NotificationServiceApi for DefaultNotificationService {
    async fn send_bill_is_signed_event(&self, event: &BillChainEvent) -> Result<()> {
        let event_type = BillEventType::BillSigned;

        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![
                (
                    event.bill.drawee.node_id.clone(),
                    (event_type.clone(), ActionType::AcceptBill),
                ),
                (
                    event.bill.payee.node_id.clone(),
                    (event_type, ActionType::CheckBill),
                ),
            ]),
            None,
            None,
        );

        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_bill_is_accepted_event(&self, event: &BillChainEvent) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                event.bill.payee.node_id.clone(),
                (BillEventType::BillAccepted, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_request_to_accept_event(&self, event: &BillChainEvent) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                event.bill.drawee.node_id.clone(),
                (
                    BillEventType::BillAcceptanceRequested,
                    ActionType::AcceptBill,
                ),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_request_to_pay_event(&self, event: &BillChainEvent) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                event.bill.drawee.node_id.clone(),
                (BillEventType::BillPaymentRequested, ActionType::PayBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_bill_is_paid_event(&self, event: &BillChainEvent) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                event.bill.payee.node_id.clone(),
                (BillEventType::BillPaid, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_bill_is_endorsed_event(&self, bill: &BillChainEvent) -> Result<()> {
        let all_events = bill.generate_action_messages(
            HashMap::from_iter(vec![(
                bill.bill.endorsee.as_ref().unwrap().node_id.clone(),
                (BillEventType::BillEndorsed, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_offer_to_sell_event(
        &self,
        event: &BillChainEvent,
        buyer: &IdentityPublicData,
    ) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                buyer.node_id.clone(),
                (BillEventType::BillSellOffered, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_bill_is_sold_event(
        &self,
        event: &BillChainEvent,
        buyer: &IdentityPublicData,
    ) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                buyer.node_id.clone(),
                (BillEventType::BillSold, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_bill_recourse_paid_event(
        &self,
        event: &BillChainEvent,
        recoursee: &IdentityPublicData,
    ) -> Result<()> {
        let all_events = event.generate_action_messages(
            HashMap::from_iter(vec![(
                recoursee.node_id.clone(),
                (BillEventType::BillRecoursePaid, ActionType::CheckBill),
            )]),
            None,
            None,
        );
        self.send_all_events(all_events).await?;
        Ok(())
    }

    async fn send_request_to_mint_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new_bill(
            &bill.endorsee.as_ref().unwrap().node_id,
            BillChainEventPayload {
                event_type: BillEventType::BillMintingRequested,
                bill_id: bill.id.clone(),
                action_type: Some(ActionType::CheckBill),
                sum: Some(bill.sum),
                ..Default::default()
            },
        );
        self.notification_transport
            .send(bill.endorsee.as_ref().unwrap(), event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_request_to_action_rejected_event(
        &self,
        event: &BillChainEvent,
        rejected_action: ActionType,
    ) -> Result<()> {
        if let Some(event_type) = rejected_action.get_rejected_event_type() {
            let all_events = event.generate_action_messages(
                HashMap::new(),
                Some(event_type),
                Some(rejected_action),
            );

            self.send_all_events(all_events).await?;
        }
        Ok(())
    }

    async fn send_request_to_action_timed_out_event(
        &self,
        bill_id: &str,
        sum: Option<u64>,
        timed_out_action: ActionType,
        recipients: Vec<IdentityPublicData>,
    ) -> Result<()> {
        if let Some(event_type) = timed_out_action.get_timeout_event_type() {
            // only send to a recipient once
            let unique: HashMap<String, IdentityPublicData> =
                HashMap::from_iter(recipients.iter().map(|r| (r.node_id.clone(), r.clone())));

            let payload = BillChainEventPayload {
                event_type,
                bill_id: bill_id.to_owned(),
                action_type: Some(ActionType::CheckBill),
                sum,
                ..Default::default()
            };
            for (_, recipient) in unique {
                let event = Event::new_bill(&recipient.node_id, payload.clone());
                self.notification_transport
                    .send(&recipient, event.try_into()?)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_recourse_action_event(
        &self,
        event: &BillChainEvent,
        action: ActionType,
        recoursee: &IdentityPublicData,
    ) -> Result<()> {
        if let Some(event_type) = action.get_recourse_event_type() {
            let all_events = event.generate_action_messages(
                HashMap::from_iter(vec![(
                    recoursee.node_id.clone(),
                    (event_type.clone(), action.clone()),
                )]),
                Some(BillEventType::BillBlock),
                None,
            );
            self.send_all_events(all_events).await?;
        }
        Ok(())
    }

    async fn send_new_quote_event(&self, _bill: &BitcreditBill) -> Result<()> {
        // @TODO: How do we know the quoting participants
        Ok(())
    }

    async fn send_quote_is_approved_event(&self, _bill: &BitcreditBill) -> Result<()> {
        // @TODO: How do we address a mint ???
        Ok(())
    }

    async fn get_client_notifications(
        &self,
        filter: NotificationFilter,
    ) -> Result<Vec<Notification>> {
        let result = self.notification_store.list(filter).await.map_err(|e| {
            error!("Failed to get client notifications: {}", e);
            Error::Persistence("Failed to get client notifications".to_string())
        })?;
        Ok(result)
    }

    async fn mark_notification_as_done(&self, notification_id: &str) -> Result<()> {
        let _ = self
            .notification_store
            .mark_as_done(notification_id)
            .await
            .map_err(|e| {
                error!("Failed to mark notification as done: {}", e);
                Error::Persistence("Failed to mark notification as done".to_string())
            })?;
        Ok(())
    }

    async fn get_active_bill_notification(&self, bill_id: &str) -> Option<Notification> {
        self.notification_store
            .get_latest_by_reference(bill_id, NotificationType::Bill)
            .await
            .unwrap_or_default()
    }

    async fn check_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<bool> {
        Ok(self
            .notification_store
            .bill_notification_sent(bill_id, block_height, action)
            .await
            .map_err(|e| {
                error!(
                    "Failed to check if bill notification was already sent: {}",
                    e
                );
                Error::Persistence(
                    "Failed to check if bill notification was already sent".to_string(),
                )
            })?)
    }

    /// Stores that a notification was sent for the given bill id and action
    async fn mark_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action: ActionType,
    ) -> Result<()> {
        self.notification_store
            .set_bill_notification_sent(bill_id, block_height, action)
            .await
            .map_err(|e| {
                error!("Failed to mark bill notification as sent: {}", e);
                Error::Persistence("Failed to mark bill notification as sent".to_string())
            })?;
        Ok(())
    }

    async fn send_retry_messages(&self) -> Result<()> {
        let mut failed_ids = vec![];
        while let Ok(Some(queued_message)) = self
            .queued_message_store
            .get_retry_messages(1)
            .await
            .map(|r| r.first().cloned())
        {
            if let Ok(message) = serde_json::from_value::<EventEnvelope>(queued_message.payload) {
                if let Err(e) = self
                    .send_retry_message(&message.node_id, message.clone())
                    .await
                {
                    error!("Failed to send retry message: {}", e);
                    failed_ids.push(queued_message.id.clone());
                } else {
                    if let Err(e) = self
                        .queued_message_store
                        .succeed_retry(&queued_message.id)
                        .await
                    {
                        error!("Failed to mark retry message as sent: {}", e);
                    }
                }
            }
        }

        for failed in failed_ids {
            if let Err(e) = self.queued_message_store.fail_retry(&failed).await {
                error!("Failed to store failed retry attemt: {}", e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use bcr_ebill_core::PostalAddress;
    use bcr_ebill_core::bill::BillKeys;
    use bcr_ebill_core::blockchain::Blockchain;
    use bcr_ebill_core::blockchain::bill::block::{
        BillAcceptBlockData, BillOfferToSellBlockData, BillRecourseBlockData,
        BillRequestToAcceptBlockData, BillRequestToPayBlockData,
    };
    use bcr_ebill_core::blockchain::bill::{BillBlock, BillBlockchain};
    use bcr_ebill_core::util::date::now;
    use bcr_ebill_transport::{EventEnvelope, EventType, PushApi};
    use mockall::{mock, predicate::eq};
    use std::sync::Arc;

    use crate::service::bill_service::test_utils::{get_baseline_identity, get_genesis_chain};
    use crate::service::contact_service::MockContactServiceApi;
    use crate::service::notification_service::create_nostr_consumer;
    use async_broadcast::Receiver;
    use serde_json::Value;

    impl ServiceTraitBounds for MockNotificationJsonTransport {}
    mock! {
        pub NotificationJsonTransport {}
        #[async_trait]
        impl NotificationJsonTransportApi for NotificationJsonTransport {
            async fn send(&self, recipient: &IdentityPublicData, event: EventEnvelope) -> bcr_ebill_transport::Result<()>;
        }

    }

    mock! {
        pub PushService {}
        #[async_trait]
        impl PushApi for PushService {
            async fn send(&self, value: Value);
            async fn subscribe(&self) -> Receiver<Value>;
        }
    }

    use super::super::test_utils::{
        get_identity_public_data, get_mock_nostr_client, get_test_bitcredit_bill,
    };
    use super::*;
    use crate::tests::tests::{
        MockBillChainStoreApiMock, MockBillStoreApiMock, MockNostrEventOffsetStoreApiMock,
        MockNostrQueuedMessageStore, MockNotificationStoreApiMock, TEST_PRIVATE_KEY_SECP,
        TEST_PUB_KEY_SECP,
    };

    fn check_chain_payload(event: &EventEnvelope, bill_event_type: BillEventType) -> bool {
        let valid_event_type = event.event_type == EventType::Bill;
        let event: Event<BillChainEventPayload> = event.clone().try_into().unwrap();
        valid_event_type && event.data.event_type == bill_event_type
    }

    #[tokio::test]
    async fn test_send_request_to_action_rejected_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let event = BillChainEvent::new(
            &bill,
            &chain,
            &BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            },
            true,
        )
        .unwrap();

        let mut mock_contact_service = MockContactServiceApi::new();

        // every participant should receive events
        mock_contact_service
            .expect_get_identity_by_node_id()
            .with(eq("buyer"))
            .returning(move |_| Ok(Some(buyer.clone())));
        mock_contact_service
            .expect_get_identity_by_node_id()
            .with(eq("drawee"))
            .returning(move |_| Ok(Some(payer.clone())));
        mock_contact_service
            .expect_get_identity_by_node_id()
            .with(eq("payee"))
            .returning(move |_| Ok(Some(payee.clone())));

        let mut mock = MockNotificationJsonTransport::new();

        // expect to send payment rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillPaymentRejected))
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send acceptance rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillAcceptanceRejected))
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send buying rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillBuyingRejected))
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send recourse rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillRecourseRejected))
            .returning(|_, _| Ok(()))
            .times(3);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(mock_contact_service),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_request_to_action_rejected_event(&event, ActionType::PayBill)
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(&event, ActionType::AcceptBill)
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(&event, ActionType::BuyBill)
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(&event, ActionType::RecourseBill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_action_rejected_does_not_send_non_rejectable_action() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let event = BillChainEvent::new(
            &bill,
            &chain,
            &BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            },
            true,
        )
        .unwrap();

        let mut mock_contact_service = MockContactServiceApi::new();

        // no participant should receive events
        mock_contact_service
            .expect_get_identity_by_node_id()
            .never();

        let mut mock = MockNotificationJsonTransport::new();

        // expect to not send rejected event for non rejectable actions
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(mock_contact_service),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_request_to_action_rejected_event(&event, ActionType::CheckBill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_action_timed_out_event() {
        let recipients = vec![
            get_identity_public_data("part1", "part1@example.com", None),
            get_identity_public_data("part2", "part2@example.com", None),
            get_identity_public_data("part3", "part3@example.com", None),
        ];

        let mut mock = MockNotificationJsonTransport::new();

        // expect to send payment timeout event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillPaymentTimeout))
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send acceptance timeout event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillAcceptanceTimeout))
            .returning(|_, _| Ok(()))
            .times(3);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(MockContactServiceApi::new()),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
                Some(100),
                ActionType::PayBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
                Some(100),
                ActionType::AcceptBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_action_timed_out_does_not_send_non_timeout_action() {
        let recipients = vec![
            get_identity_public_data("part1", "part1@example.com", None),
            get_identity_public_data("part2", "part2@example.com", None),
            get_identity_public_data("part3", "part3@example.com", None),
        ];

        let mut mock = MockNotificationJsonTransport::new();

        // expect to never send timeout event on non expiring events
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(MockContactServiceApi::new()),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
                Some(100),
                ActionType::CheckBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_recourse_action_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let event = BillChainEvent::new(
            &bill,
            &chain,
            &BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            },
            true,
        )
        .unwrap();

        let mut mock_contact_service = MockContactServiceApi::new();

        let buyer_clone = buyer.clone();
        // participants should receive events
        mock_contact_service
            .expect_get_identity_by_node_id()
            .returning(move |_| Ok(Some(buyer_clone.clone())));
        mock_contact_service
            .expect_get_identity_by_node_id()
            .returning(move |_| Ok(Some(payee.clone())));
        mock_contact_service
            .expect_get_identity_by_node_id()
            .returning(move |_| Ok(Some(payer.clone())));

        let mut mock = MockNotificationJsonTransport::new();

        // expect to send payment recourse event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillPaymentRecourse))
            .returning(|_, _| Ok(()))
            .times(1);
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillBlock))
            .returning(|_, _| Ok(()))
            .times(2);

        // expect to send acceptance recourse event to all recipients
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillAcceptanceRecourse))
            .returning(|_, _| Ok(()))
            .times(1);
        mock.expect_send()
            .withf(|_, e| check_chain_payload(e, BillEventType::BillBlock))
            .returning(|_, _| Ok(()))
            .times(2);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(mock_contact_service),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_recourse_action_event(&event, ActionType::PayBill, &buyer)
            .await
            .expect("failed to send event");

        service
            .send_recourse_action_event(&event, ActionType::AcceptBill, &buyer)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_recourse_action_event_does_not_send_non_recurse_action() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let event = BillChainEvent::new(
            &bill,
            &chain,
            &BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            },
            true,
        )
        .unwrap();

        let mut mock_contact_service = MockContactServiceApi::new();

        // participants should receive events
        mock_contact_service
            .expect_get_identity_by_node_id()
            .never();

        let mut mock = MockNotificationJsonTransport::new();

        // expect not to send non recourse event
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(MockContactServiceApi::new()),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        service
            .send_recourse_action_event(&event, ActionType::CheckBill, &payer)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_failed_to_send_is_added_to_retry_queue() {
        // given a payer and payee with a new bill
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));

        let mut mock_contact_service = MockContactServiceApi::new();
        mock_contact_service
            .expect_get_identity_by_node_id()
            .with(eq(payer.node_id.clone()))
            .returning(move |_| Ok(Some(payer.clone())));

        mock_contact_service
            .expect_get_identity_by_node_id()
            .with(eq(payee.node_id.clone()))
            .returning(move |_| Ok(Some(payee.clone())));

        let mut mock = MockNotificationJsonTransport::new();
        mock.expect_send().returning(|_, _| Ok(())).once();
        mock.expect_send()
            .returning(|_, _| Err(Error::Network("Failed to send".to_string())));

        let mut queue_mock = MockNostrQueuedMessageStore::new();
        queue_mock
            .expect_add_message()
            .returning(|_, _| Ok(()))
            .once();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(mock_contact_service),
            queued_message_store: Arc::new(queue_mock),
        };

        let event = BillChainEvent::new(
            &bill,
            &chain,
            &BillKeys {
                private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                public_key: TEST_PUB_KEY_SECP.to_owned(),
            },
            true,
        )
        .unwrap();

        service
            .send_bill_is_signed_event(&event)
            .await
            .expect("failed to send event");
    }

    fn setup_chain_expectation(
        participants: Vec<(IdentityPublicData, BillEventType, Option<ActionType>)>,
        bill: &BitcreditBill,
        chain: &BillBlockchain,
        new_blocks: bool,
    ) -> (DefaultNotificationService, BillChainEvent) {
        let mut mock_contact_service = MockContactServiceApi::new();
        let mut mock = MockNotificationJsonTransport::new();
        for p in participants.into_iter() {
            let clone1 = p.clone();
            mock_contact_service
                .expect_get_identity_by_node_id()
                .with(eq(p.0.node_id.clone()))
                .returning(move |_| Ok(Some(clone1.0.clone())));

            let clone2 = p.clone();
            mock.expect_send()
                .withf(move |r, e| {
                    let part = clone2.clone();
                    let valid_node_id = r.node_id == part.0.node_id && e.node_id == part.0.node_id;
                    let event: Event<BillChainEventPayload> = e.clone().try_into().unwrap();
                    let valid_event_type = event.data.event_type == part.1;
                    valid_node_id && valid_event_type && event.data.action_type == part.2
                })
                .returning(|_, _| Ok(()));
        }

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(mock_contact_service),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        };

        (
            service,
            BillChainEvent::new(
                bill,
                chain,
                &BillKeys {
                    private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
                    public_key: TEST_PUB_KEY_SECP.to_owned(),
                },
                new_blocks,
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn test_send_bill_is_signed_event() {
        // given a payer and payee with a new bill
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));
        let (service, event) = setup_chain_expectation(
            vec![
                (
                    payer,
                    BillEventType::BillSigned,
                    Some(ActionType::AcceptBill),
                ),
                (
                    payee,
                    BillEventType::BillSigned,
                    Some(ActionType::CheckBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );
        service
            .send_bill_is_signed_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_accepted_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_accept(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillAcceptBlockData {
                accepter: payer.clone().into(),
                signatory: None,
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (
                    payee,
                    BillEventType::BillAccepted,
                    Some(ActionType::CheckBill),
                ),
                (payer, BillEventType::BillBlock, None),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_bill_is_accepted_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_accept_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_request_to_accept(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillRequestToAcceptBlockData {
                requester: payee.clone().into(),
                signatory: None,
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (
                    payer,
                    BillEventType::BillAcceptanceRequested,
                    Some(ActionType::AcceptBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_request_to_accept_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_pay_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_request_to_pay(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillRequestToPayBlockData {
                requester: payee.clone().into(),
                currency: "USD".to_string(),
                signatory: None,
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (
                    payer,
                    BillEventType::BillPaymentRequested,
                    Some(ActionType::PayBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_request_to_pay_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_paid_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));
        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillPaid, Some(ActionType::CheckBill)),
                (payer, BillEventType::BillBlock, None),
            ],
            &bill,
            &chain,
            false,
        );

        service
            .send_bill_is_paid_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_endorsed_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let endorsee = get_identity_public_data("endorsee", "endorsee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, Some(&endorsee));
        let chain = get_genesis_chain(Some(bill.clone()));

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (payer, BillEventType::BillBlock, None),
                (
                    endorsee,
                    BillEventType::BillAcceptanceRequested,
                    Some(ActionType::AcceptBill),
                ),
            ],
            &bill,
            &chain,
            false,
        );

        service
            .send_bill_is_endorsed_event(&event)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_offer_to_sell_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (payer, BillEventType::BillBlock, None),
                (
                    buyer.clone(),
                    BillEventType::BillSellOffered,
                    Some(ActionType::CheckBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_offer_to_sell_event(&event, &buyer)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_sold_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let buyer = get_identity_public_data("buyer", "buyer@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_offer_to_sell(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillOfferToSellBlockData {
                seller: payee.clone().into(),
                buyer: buyer.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                payment_address: "Address".to_string(),
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (payer, BillEventType::BillBlock, None),
                (
                    buyer.clone(),
                    BillEventType::BillSold,
                    Some(ActionType::CheckBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_bill_is_sold_event(&event, &buyer)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_recourse_paid_event() {
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let recoursee = get_identity_public_data("recoursee", "recoursee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let mut chain = get_genesis_chain(Some(bill.clone()));
        let timestamp = now().timestamp() as u64;
        let keys = get_baseline_identity().key_pair;
        let block = BillBlock::create_block_for_recourse(
            bill.id.to_owned(),
            chain.get_latest_block(),
            &BillRecourseBlockData {
                recourser: payee.clone().into(),
                recoursee: recoursee.clone().into(),
                sum: 100,
                currency: "USD".to_string(),
                signatory: None,
                signing_timestamp: timestamp,
                signing_address: PostalAddress::default(),
            },
            &keys,
            None,
            &keys,
            timestamp,
        )
        .unwrap();

        chain.try_add_block(block);

        let (service, event) = setup_chain_expectation(
            vec![
                (payee, BillEventType::BillBlock, None),
                (payer, BillEventType::BillBlock, None),
                (
                    recoursee.clone(),
                    BillEventType::BillRecoursePaid,
                    Some(ActionType::CheckBill),
                ),
            ],
            &bill,
            &chain,
            true,
        );

        service
            .send_bill_recourse_paid_event(&event, &recoursee)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_mint_event() {
        let bill = get_test_bill();

        // should send minting requested to endorsee (mint)
        let service = setup_service_expectation(
            "endorsee",
            BillEventType::BillMintingRequested,
            ActionType::CheckBill,
        );

        service
            .send_request_to_mint_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn get_client_notifications() {
        let mut mock_store = MockNotificationStoreApiMock::new();
        let result = Notification::new_bill_notification("bill_id", "node_id", "desc", None);
        let returning = result.clone();
        let filter = NotificationFilter {
            active: Some(true),
            ..Default::default()
        };
        mock_store
            .expect_list()
            .with(eq(filter.clone()))
            .returning(move |_| Ok(vec![returning.clone()]));

        let service = DefaultNotificationService::new(
            Box::new(MockNotificationJsonTransport::new()),
            Arc::new(mock_store),
            Arc::new(MockContactServiceApi::new()),
            Arc::new(MockNostrQueuedMessageStore::new()),
        );

        let res = service
            .get_client_notifications(filter)
            .await
            .expect("could not get notifications");
        assert!(!res.is_empty());
        assert_eq!(res[0].id, result.id);
    }

    #[tokio::test]
    async fn get_mark_notification_done() {
        let mut mock_store = MockNotificationStoreApiMock::new();
        mock_store
            .expect_mark_as_done()
            .with(eq("notification_id"))
            .returning(|_| Ok(()));

        let service = DefaultNotificationService::new(
            Box::new(MockNotificationJsonTransport::new()),
            Arc::new(mock_store),
            Arc::new(MockContactServiceApi::new()),
            Arc::new(MockNostrQueuedMessageStore::new()),
        );

        service
            .mark_notification_as_done("notification_id")
            .await
            .expect("could not mark notification as done");
    }

    fn setup_service_expectation(
        node_id: &str,
        event_type: BillEventType,
        action_type: ActionType,
    ) -> DefaultNotificationService {
        let node_id = node_id.to_owned();
        let mut mock = MockNotificationJsonTransport::new();
        mock.expect_send()
            .withf(move |r, e| {
                let valid_node_id = r.node_id == node_id && e.node_id == node_id;
                let event: Event<BillChainEventPayload> = e.clone().try_into().unwrap();
                valid_node_id
                    && event.data.event_type == event_type
                    && event.data.action_type == Some(action_type.clone())
            })
            .returning(|_, _| Ok(()));
        DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApiMock::new()),
            contact_service: Arc::new(MockContactServiceApi::new()),
            queued_message_store: Arc::new(MockNostrQueuedMessageStore::new()),
        }
    }

    fn get_test_bill() -> BitcreditBill {
        get_test_bitcredit_bill(
            "bill",
            &get_identity_public_data("drawee", "drawee@example.com", None),
            &get_identity_public_data("payee", "payee@example.com", None),
            Some(&get_identity_public_data(
                "drawer",
                "drawer@example.com",
                None,
            )),
            Some(&get_identity_public_data(
                "endorsee",
                "endorsee@example.com",
                None,
            )),
        )
    }

    #[tokio::test]
    async fn test_create_nostr_consumer() {
        let client = get_mock_nostr_client().await;
        let contact_service = Arc::new(MockContactServiceApi::new());
        let store = Arc::new(MockNostrEventOffsetStoreApiMock::new());
        let notification_store = Arc::new(MockNotificationStoreApiMock::new());
        let push_service = Arc::new(MockPushService::new());
        let bill_store = Arc::new(MockBillStoreApiMock::new());
        let bill_blockchain_store = Arc::new(MockBillChainStoreApiMock::new());
        let _ = create_nostr_consumer(
            client,
            contact_service,
            store,
            notification_store,
            push_service,
            bill_blockchain_store,
            bill_store,
        )
        .await;
    }
}
