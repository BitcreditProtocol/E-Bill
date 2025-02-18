use super::{NotificationType, Result};
use std::sync::Arc;

use crate::{
    data::notification::Notification,
    persistence::notification::NotificationStoreApi,
    service::notification_service::event::{BillActionEventPayload, Event},
};

use super::{
    handler::NotificationHandlerApi, push_notification::PushApi, EventEnvelope, EventType,
};
use async_trait::async_trait;

#[derive(Clone)]
pub struct BillActionEventHandler {
    notification_store: Arc<dyn NotificationStoreApi>,
    push_service: Arc<dyn PushApi>,
}

impl BillActionEventHandler {
    pub fn new(
        notification_store: Arc<dyn NotificationStoreApi>,
        push_service: Arc<dyn PushApi>,
    ) -> Self {
        Self {
            notification_store,
            push_service,
        }
    }

    fn event_description(&self, event_type: &EventType) -> String {
        match event_type {
            EventType::BillSigned => "Bill has been signed".to_string(),
            EventType::BillAccepted => "Bill has been accepted".to_string(),
            EventType::BillAcceptanceRequested => "Bill should be accepted".to_string(),
            EventType::BillAcceptanceRejected => "Bill acceptance has been rejected".to_string(),
            EventType::BillAcceptanceTimeout => "Bill acceptance has taken too long".to_string(),
            EventType::BillAcceptanceRecourse => "Bill in recourse should be accepted".to_string(),
            EventType::BillPaymentRequested => "Bill should be paid".to_string(),
            EventType::BillPaymentRejected => "Bill payment has been rejected".to_string(),
            EventType::BillPaymentTimeout => "Bill payment has taken too long".to_string(),
            EventType::BillPaymentRecourse => "Bill in recourse should be paid".to_string(),
            EventType::BillRecourseRejected => "Bill recourse has been rejected".to_string(),
            EventType::BillRecourseTimeout => "Bill recourse has taken too long".to_string(),
            EventType::BillSellOffered => "Bill should be sold".to_string(),
            EventType::BillBuyingRejected => "Bill buying has been rejected".to_string(),
            EventType::BillPaid => "Bill has been paid".to_string(),
            EventType::BillRecoursePaid => "Bill recourse has been paid".to_string(),
            EventType::BillEndorsed => "Bill has been endorsed".to_string(),
            EventType::BillSold => "Bill has been sold".to_string(),
            EventType::BillMintingRequested => "Bill should be minted".to_string(),
            EventType::BillNewQuote => "New quote has been added".to_string(),
            EventType::BillQuoteApproved => "Quote has been approved".to_string(),
        }
    }
}

#[async_trait]
impl NotificationHandlerApi for BillActionEventHandler {
    fn handles_event(&self, _event_type: &EventType) -> bool {
        true
    }

    async fn handle_event(&self, event: EventEnvelope, node_id: &str) -> Result<()> {
        let event: Option<Event<BillActionEventPayload>> = event.try_into().ok();
        if let Some(event) = event {
            // create notification
            let notification = Notification::new_bill_notification(
                &event.data.bill_id,
                node_id,
                &self.event_description(&event.event_type),
                Some(serde_json::to_value(&event.data)?),
            );

            // mark Bill event as done if any active one exists
            if let Some(currently_active) = self
                .notification_store
                .get_latest_by_reference(&event.data.bill_id, NotificationType::Bill)
                .await?
            {
                self.notification_store
                    .mark_as_done(&currently_active.id)
                    .await?;
            }

            // save new notification to database
            self.notification_store.add(notification.clone()).await?;

            // send push notification to connected clients
            self.push_service
                .send(serde_json::to_value(notification)?)
                .await;
        }
        Ok(())
    }
}
