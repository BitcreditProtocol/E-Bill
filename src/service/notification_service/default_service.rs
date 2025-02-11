use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::event::{ActionType, BillActionEventPayload, Event, EventType};
use super::transport::NotificationJsonTransportApi;
use super::{Notification, NotificationServiceApi, NotificationType, Result};
use crate::persistence::notification::{NotificationFilter, NotificationStoreApi};
use crate::service::bill_service::BitcreditBill;
use crate::service::contact_service::IdentityPublicData;

/// A default implementation of the NotificationServiceApi that can
/// send events via json and email transports.
#[allow(dead_code)]
pub struct DefaultNotificationService {
    notification_transport: Box<dyn NotificationJsonTransportApi>,
    notification_store: Arc<dyn NotificationStoreApi>,
}

impl DefaultNotificationService {
    pub fn new(
        notification_transport: Box<dyn NotificationJsonTransportApi>,
        notification_store: Arc<dyn NotificationStoreApi>,
    ) -> Self {
        Self {
            notification_transport,
            notification_store,
        }
    }
}

#[async_trait]
impl NotificationServiceApi for DefaultNotificationService {
    async fn send_bill_is_signed_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event_type = EventType::BillSigned;

        let payer_event = Event::new(
            event_type.to_owned(),
            &bill.drawee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::AcceptBill,
            },
        );
        let payee_event = Event::new(
            event_type,
            &bill.payee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::CheckBill,
            },
        );

        self.notification_transport
            .send(&bill.drawee, payer_event.try_into()?)
            .await?;

        self.notification_transport
            .send(&bill.payee, payee_event.try_into()?)
            .await?;

        Ok(())
    }

    async fn send_bill_is_accepted_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillAccepted,
            &bill.payee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::CheckBill,
            },
        );

        self.notification_transport
            .send(&bill.payee, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_request_to_accept_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillAcceptanceRequested,
            &bill.drawee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::AcceptBill,
            },
        );
        self.notification_transport
            .send(&bill.drawee, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_request_to_pay_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillPaymentRequested,
            &bill.drawee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::PayBill,
            },
        );
        self.notification_transport
            .send(&bill.drawee, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_bill_is_paid_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillPaid,
            &bill.payee.node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::CheckBill,
            },
        );

        self.notification_transport
            .send(&bill.payee, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_bill_is_endorsed_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillEndorsed,
            &bill.endorsee.as_ref().unwrap().node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::CheckBill,
            },
        );

        self.notification_transport
            .send(bill.endorsee.as_ref().unwrap(), event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_offer_to_sell_event(
        &self,
        bill_id: &str,
        buyer: &IdentityPublicData,
    ) -> Result<()> {
        let event = Event::new(
            EventType::BillSellOffered,
            &buyer.node_id,
            BillActionEventPayload {
                bill_id: bill_id.to_owned(),
                action_type: ActionType::CheckBill,
            },
        );
        self.notification_transport
            .send(buyer, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_bill_is_sold_event(
        &self,
        bill_id: &str,
        buyer: &IdentityPublicData,
    ) -> Result<()> {
        let event = Event::new(
            EventType::BillSold,
            &buyer.node_id,
            BillActionEventPayload {
                bill_id: bill_id.to_owned(),
                action_type: ActionType::CheckBill,
            },
        );
        self.notification_transport
            .send(buyer, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_bill_recourse_paid_event(
        &self,
        bill_id: &str,
        recoursee: &IdentityPublicData,
    ) -> Result<()> {
        let event = Event::new(
            EventType::BillRecoursePaid,
            &recoursee.node_id,
            BillActionEventPayload {
                bill_id: bill_id.to_owned(),
                action_type: ActionType::CheckBill,
            },
        );
        self.notification_transport
            .send(recoursee, event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_request_to_mint_event(&self, bill: &BitcreditBill) -> Result<()> {
        let event = Event::new(
            EventType::BillMintingRequested,
            &bill.endorsee.as_ref().unwrap().node_id,
            BillActionEventPayload {
                bill_id: bill.id.clone(),
                action_type: ActionType::CheckBill,
            },
        );
        self.notification_transport
            .send(bill.endorsee.as_ref().unwrap(), event.try_into()?)
            .await?;
        Ok(())
    }

    async fn send_request_to_action_rejected_event(
        &self,
        bill_id: &str,
        rejected_action: ActionType,
        recipients: Vec<IdentityPublicData>,
    ) -> Result<()> {
        if let Some(event_type) = rejected_action.get_rejected_event_type() {
            let payload = BillActionEventPayload {
                bill_id: bill_id.to_owned(),
                action_type: ActionType::CheckBill,
            };
            for recipient in recipients {
                let event = Event::new(event_type.to_owned(), &recipient.node_id, payload.clone());
                self.notification_transport
                    .send(&recipient, event.try_into()?)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_request_to_action_timed_out_event(
        &self,
        bill_id: &str,
        timed_out_action: ActionType,
        recipients: Vec<IdentityPublicData>,
    ) -> Result<()> {
        if let Some(event_type) = timed_out_action.get_timeout_event_type() {
            // only send to a recipient once
            let unique: HashMap<String, IdentityPublicData> =
                HashMap::from_iter(recipients.iter().map(|r| (r.node_id.clone(), r.clone())));

            let payload = BillActionEventPayload {
                bill_id: bill_id.to_owned(),
                action_type: ActionType::CheckBill,
            };
            for (_, recipient) in unique {
                let event = Event::new(event_type.to_owned(), &recipient.node_id, payload.clone());
                self.notification_transport
                    .send(&recipient, event.try_into()?)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_recourse_action_event(
        &self,
        bill_id: &str,
        action: ActionType,
        recipient: &IdentityPublicData,
    ) -> Result<()> {
        if let Some(event_type) = action.get_recourse_event_type() {
            let event = Event::new(
                event_type.to_owned(),
                &recipient.node_id,
                BillActionEventPayload {
                    bill_id: bill_id.to_owned(),
                    action_type: action,
                },
            );
            self.notification_transport
                .send(recipient, event.try_into()?)
                .await?;
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
        let result = self.notification_store.list(filter).await?;
        Ok(result)
    }

    async fn mark_notification_as_done(&self, notification_id: &str) -> Result<()> {
        let _ = self
            .notification_store
            .mark_as_done(notification_id)
            .await?;
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
            .await?)
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
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use mockall::predicate::eq;

    use crate::persistence::nostr::MockNostrEventOffsetStoreApi;
    use crate::persistence::notification::MockNotificationStoreApi;
    use crate::service::contact_service::MockContactServiceApi;
    use crate::service::notification_service::create_nostr_consumer;
    use crate::service::notification_service::push_notification::MockPushApi;
    use crate::service::notification_service::transport::MockNotificationJsonTransportApi;

    use super::super::test_utils::{
        get_identity_public_data, get_mock_nostr_client, get_test_bitcredit_bill,
    };
    use super::*;

    #[tokio::test]
    async fn test_send_request_to_action_rejected_event() {
        let recipients = vec![
            get_identity_public_data("part1", "part1@example.com", None),
            get_identity_public_data("part2", "part2@example.com", None),
            get_identity_public_data("part3", "part3@example.com", None),
        ];

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect to send payment rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillPaymentRejected)
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send acceptance rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillAcceptanceRejected)
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send buying rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillBuyingRejected)
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send recourse rejected event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillRecourseRejected)
            .returning(|_, _| Ok(()))
            .times(3);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_request_to_action_rejected_event(
                "bill_id",
                ActionType::PayBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(
                "bill_id",
                ActionType::AcceptBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(
                "bill_id",
                ActionType::BuyBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_rejected_event(
                "bill_id",
                ActionType::RecourseBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_action_rejected_does_not_send_non_rejectable_action() {
        let recipients = vec![
            get_identity_public_data("part1", "part1@example.com", None),
            get_identity_public_data("part2", "part2@example.com", None),
            get_identity_public_data("part3", "part3@example.com", None),
        ];

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect to not send rejected event for non rejectable actions
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_request_to_action_rejected_event(
                "bill_id",
                ActionType::CheckBill,
                recipients.clone(),
            )
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

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect to send payment timeout event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillPaymentTimeout)
            .returning(|_, _| Ok(()))
            .times(3);

        // expect to send acceptance timeout event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillAcceptanceTimeout)
            .returning(|_, _| Ok(()))
            .times(3);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
                ActionType::PayBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
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

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect to never send timeout event on non expiring events
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_request_to_action_timed_out_event(
                "bill_id",
                ActionType::CheckBill,
                recipients.clone(),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_recourse_action_event() {
        let recipient = get_identity_public_data("part1", "part1@example.com", None);

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect to send payment recourse event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillPaymentRecourse)
            .returning(|_, _| Ok(()))
            .times(1);

        // expect to send acceptance recourse event to all recipients
        mock.expect_send()
            .withf(|_, e| e.event_type == EventType::BillAcceptanceRecourse)
            .returning(|_, _| Ok(()))
            .times(1);

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_recourse_action_event("bill_id", ActionType::PayBill, &recipient)
            .await
            .expect("failed to send event");

        service
            .send_recourse_action_event("bill_id", ActionType::AcceptBill, &recipient)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_recourse_action_event_does_not_send_non_recurse_action() {
        let recipient = get_identity_public_data("part1", "part1@example.com", None);

        let mut mock = MockNotificationJsonTransportApi::new();

        // expect not to send non recourse event
        mock.expect_send().never();

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_recourse_action_event("bill_id", ActionType::CheckBill, &recipient)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_signed_event() {
        // given a payer and payee with a new bill
        let payer = get_identity_public_data("drawee", "drawee@example.com", None);
        let payee = get_identity_public_data("payee", "payee@example.com", None);
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);

        let mut mock = MockNotificationJsonTransportApi::new();
        mock.expect_send()
            .withf(|r, e| {
                let valid_node_id = r.node_id == "drawee" && e.node_id == "drawee";
                let valid_event_type = e.event_type == EventType::BillSigned;
                let event: Event<BillActionEventPayload> = e.clone().try_into().unwrap();
                valid_node_id
                    && valid_event_type
                    && event.data.action_type == ActionType::AcceptBill
            })
            .returning(|_, _| Ok(()));

        mock.expect_send()
            .withf(|r, e| {
                let valid_node_id = r.node_id == "payee" && e.node_id == "payee";
                let valid_event_type = e.event_type == EventType::BillSigned;
                let event: Event<BillActionEventPayload> = e.clone().try_into().unwrap();
                valid_node_id && valid_event_type && event.data.action_type == ActionType::CheckBill
            })
            .returning(|_, _| Ok(()));

        let service = DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
        };

        service
            .send_bill_is_signed_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_accepted_event() {
        let bill = get_test_bill();

        // should send accepted to payee
        let service =
            setup_service_expectation("payee", EventType::BillAccepted, ActionType::CheckBill);

        service
            .send_bill_is_accepted_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_accept_event() {
        let bill = get_test_bill();

        // should send request to accept to drawee
        let service = setup_service_expectation(
            "drawee",
            EventType::BillAcceptanceRequested,
            ActionType::AcceptBill,
        );

        service
            .send_request_to_accept_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_pay_event() {
        let bill = get_test_bill();

        // should send request to pay to drawee
        let service = setup_service_expectation(
            "drawee",
            EventType::BillPaymentRequested,
            ActionType::PayBill,
        );

        service
            .send_request_to_pay_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_paid_event() {
        let bill = get_test_bill();

        // should send paid to payee
        let service =
            setup_service_expectation("payee", EventType::BillPaid, ActionType::CheckBill);

        service
            .send_bill_is_paid_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_endorsed_event() {
        let bill = get_test_bill();

        // should send endorsed to endorsee
        let service =
            setup_service_expectation("endorsee", EventType::BillEndorsed, ActionType::CheckBill);

        service
            .send_bill_is_endorsed_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_offer_to_sell_event() {
        let bill = get_test_bill();

        // should send offer to sell to endorsee
        let service =
            setup_service_expectation("buyer", EventType::BillSellOffered, ActionType::CheckBill);

        service
            .send_offer_to_sell_event(
                &bill.id,
                &get_identity_public_data("buyer", "buyer@example.com", None),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_is_sold_event() {
        let bill = get_test_bill();

        // should send sold event to buyer
        let service =
            setup_service_expectation("buyer", EventType::BillSold, ActionType::CheckBill);

        service
            .send_bill_is_sold_event(
                &bill.id,
                &get_identity_public_data("buyer", "buyer@example.com", None),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_bill_recourse_paid_event() {
        let bill = get_test_bill();

        // should send sold event to recoursee
        let service = setup_service_expectation(
            "recoursee",
            EventType::BillRecoursePaid,
            ActionType::CheckBill,
        );

        service
            .send_bill_recourse_paid_event(
                &bill.id,
                &get_identity_public_data("recoursee", "recoursee@example.com", None),
            )
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn test_send_request_to_mint_event() {
        let bill = get_test_bill();

        // should send minting requested to endorsee (mint)
        let service = setup_service_expectation(
            "endorsee",
            EventType::BillMintingRequested,
            ActionType::CheckBill,
        );

        service
            .send_request_to_mint_event(&bill)
            .await
            .expect("failed to send event");
    }

    #[tokio::test]
    async fn get_client_notifications() {
        let mut mock_store = MockNotificationStoreApi::new();
        let result = Notification::new_bill_notification("bill_id", "node_id", "desc", None);
        let returning = result.clone();
        let mut filter = NotificationFilter::default();
        filter.active = Some(true);
        mock_store
            .expect_list()
            .with(eq(filter.clone()))
            .returning(move |_| Ok(vec![returning.clone()]));

        let service = DefaultNotificationService::new(
            Box::new(MockNotificationJsonTransportApi::new()),
            Arc::new(mock_store),
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
        let mut mock_store = MockNotificationStoreApi::new();
        mock_store
            .expect_mark_as_done()
            .with(eq("notification_id"))
            .returning(|_| Ok(()));

        let service = DefaultNotificationService::new(
            Box::new(MockNotificationJsonTransportApi::new()),
            Arc::new(mock_store),
        );

        service
            .mark_notification_as_done("notification_id")
            .await
            .expect("could not mark notification as done");
    }

    fn setup_service_expectation(
        node_id: &str,
        event_type: EventType,
        action_type: ActionType,
    ) -> DefaultNotificationService {
        let node_id = node_id.to_owned();
        let mut mock = MockNotificationJsonTransportApi::new();
        mock.expect_send()
            .withf(move |r, e| {
                let valid_node_id = r.node_id == node_id && e.node_id == node_id;
                let valid_event_type = e.event_type == event_type;
                let event: Event<BillActionEventPayload> = e.clone().try_into().unwrap();
                valid_node_id && valid_event_type && event.data.action_type == action_type
            })
            .returning(|_, _| Ok(()));
        DefaultNotificationService {
            notification_transport: Box::new(mock),
            notification_store: Arc::new(MockNotificationStoreApi::new()),
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
        let store = Arc::new(MockNostrEventOffsetStoreApi::new());
        let notification_store = Arc::new(MockNotificationStoreApi::new());
        let push_service = Arc::new(MockPushApi::new());
        let _ = create_nostr_consumer(
            client,
            contact_service,
            store,
            notification_store,
            push_service,
        )
        .await;
    }
}
