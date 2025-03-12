use super::BillEventType;
use super::NotificationHandlerApi;
use crate::BillChainEventPayload;
use crate::EventType;
use crate::{Error, Event, EventEnvelope, PushApi, Result};
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::notification::{Notification, NotificationType};
use bcr_ebill_persistence::NotificationStoreApi;
use log::error;
use std::sync::Arc;

#[derive(Clone)]
pub struct BillChainEventHandler {
    notification_store: Arc<dyn NotificationStoreApi>,
    push_service: Arc<dyn PushApi>,
}

impl BillChainEventHandler {
    pub fn new(
        notification_store: Arc<dyn NotificationStoreApi>,
        push_service: Arc<dyn PushApi>,
    ) -> Self {
        Self {
            notification_store,
            push_service,
        }
    }

    fn event_description(&self, event_type: &BillEventType) -> String {
        match event_type {
            BillEventType::BillSigned => "Bill has been signed".to_string(),
            BillEventType::BillAccepted => "Bill has been accepted".to_string(),
            BillEventType::BillAcceptanceRequested => "Bill should be accepted".to_string(),
            BillEventType::BillAcceptanceRejected => {
                "Bill acceptance has been rejected".to_string()
            }
            BillEventType::BillAcceptanceTimeout => {
                "Bill acceptance has taken too long".to_string()
            }
            BillEventType::BillAcceptanceRecourse => {
                "Bill in recourse should be accepted".to_string()
            }
            BillEventType::BillPaymentRequested => "Bill should be paid".to_string(),
            BillEventType::BillPaymentRejected => "Bill payment has been rejected".to_string(),
            BillEventType::BillPaymentTimeout => "Bill payment has taken too long".to_string(),
            BillEventType::BillPaymentRecourse => "Bill in recourse should be paid".to_string(),
            BillEventType::BillRecourseRejected => "Bill recourse has been rejected".to_string(),
            BillEventType::BillRecourseTimeout => "Bill recourse has taken too long".to_string(),
            BillEventType::BillSellOffered => "Bill should be sold".to_string(),
            BillEventType::BillBuyingRejected => "Bill buying has been rejected".to_string(),
            BillEventType::BillPaid => "Bill has been paid".to_string(),
            BillEventType::BillRecoursePaid => "Bill recourse has been paid".to_string(),
            BillEventType::BillEndorsed => "Bill has been endorsed".to_string(),
            BillEventType::BillSold => "Bill has been sold".to_string(),
            BillEventType::BillMintingRequested => "Bill should be minted".to_string(),
            BillEventType::BillNewQuote => "New quote has been added".to_string(),
            BillEventType::BillQuoteApproved => "Quote has been approved".to_string(),
            BillEventType::BillBlock => "".to_string(),
        }
    }
}

impl ServiceTraitBounds for BillChainEventHandler {}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl NotificationHandlerApi for BillChainEventHandler {
    fn handles_event(&self, event_type: &EventType) -> bool {
        event_type == &EventType::Bill
    }

    async fn handle_event(&self, event: EventEnvelope, node_id: &str) -> Result<()> {
        let event: Option<Event<BillChainEventPayload>> = event.try_into().ok();
        if let Some(event) = event {
            // create notification
            let notification = Notification::new_bill_notification(
                &event.data.bill_id,
                node_id,
                &self.event_description(&event.data.event_type),
                Some(serde_json::to_value(&event.data)?),
            );

            // mark Bill event as done if any active one exists
            match self
                .notification_store
                .get_latest_by_reference(&event.data.bill_id, NotificationType::Bill)
                .await
            {
                Ok(Some(currently_active)) => {
                    if let Err(e) = self
                        .notification_store
                        .mark_as_done(&currently_active.id)
                        .await
                    {
                        error!(
                            "Failed to mark currently active notification as done: {}",
                            e
                        );
                    }
                }
                Err(e) => error!("Failed to get latest notification by reference: {}", e),
                Ok(None) => {}
            }

            // save new notification to database
            self.notification_store
                .add(notification.clone())
                .await
                .map_err(|e| {
                    error!("Failed to save new notification to database: {}", e);
                    Error::Persistence("Failed to save new notification to database".to_string())
                })?;

            // send push notification to connected clients
            self.push_service
                .send(serde_json::to_value(notification)?)
                .await;
        }
        Ok(())
    }
}
