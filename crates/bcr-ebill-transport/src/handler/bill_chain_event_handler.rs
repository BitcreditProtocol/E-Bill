use super::BillEventType;
use super::NotificationHandlerApi;
use crate::BillChainEventPayload;
use crate::EventType;
use crate::{Error, Event, EventEnvelope, PushApi, Result};
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::bill::BillKeys;
use bcr_ebill_core::blockchain::Blockchain;
use bcr_ebill_core::blockchain::bill::{BillBlock, BillBlockchain};
use bcr_ebill_core::notification::{Notification, NotificationType};
use bcr_ebill_persistence::NotificationStoreApi;
use bcr_ebill_persistence::bill::BillChainStoreApi;
use bcr_ebill_persistence::bill::BillStoreApi;
use log::error;
use log::warn;
use std::sync::Arc;

#[derive(Clone)]
pub struct BillChainEventHandler {
    notification_store: Arc<dyn NotificationStoreApi>,
    push_service: Arc<dyn PushApi>,
    bill_blockchain_store: Arc<dyn BillChainStoreApi>,
    bill_store: Arc<dyn BillStoreApi>,
}

impl BillChainEventHandler {
    pub fn new(
        notification_store: Arc<dyn NotificationStoreApi>,
        push_service: Arc<dyn PushApi>,
        bill_blockchain_store: Arc<dyn BillChainStoreApi>,
        bill_store: Arc<dyn BillStoreApi>,
    ) -> Self {
        Self {
            notification_store,
            push_service,
            bill_blockchain_store,
            bill_store,
        }
    }

    async fn create_notification(
        &self,
        event: &BillChainEventPayload,
        node_id: &str,
    ) -> Result<()> {
        // create notification
        let notification = Notification::new_bill_notification(
            &event.bill_id,
            node_id,
            &event_description(&event.event_type),
            Some(serde_json::to_value(event)?),
        );
        // mark Bill event as done if any active one exists
        match self
            .notification_store
            .get_latest_by_reference(&event.bill_id, NotificationType::Bill)
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
        match serde_json::to_value(notification) {
            Ok(notification) => {
                self.push_service.send(notification).await;
            }
            Err(e) => {
                error!("Failed to serialize notification for push service: {}", e);
            }
        }
        Ok(())
    }

    async fn process_chain_data(
        &self,
        bill_id: &str,
        blocks: Vec<BillBlock>,
        keys: Option<BillKeys>,
    ) -> Result<()> {
        match keys {
            Some(keys) => self.add_new_chain(blocks, &keys).await,
            None if !blocks.is_empty() => self.add_bill_blocks(bill_id, blocks).await,
            _ => Ok(()),
        }
    }

    async fn add_bill_blocks(&self, bill_id: &str, blocks: Vec<BillBlock>) -> Result<()> {
        if let Ok(mut chain) = self.bill_blockchain_store.get_chain(bill_id).await {
            for block in blocks {
                chain.try_add_block(block.clone());
                if !chain.is_chain_valid() {
                    error!("Received block is not valid for bill {bill_id}");
                    return Err(Error::BlockChain(
                        "Received bill block is not valid".to_string(),
                    ));
                }
                self.save_block(bill_id, &block).await?
            }
            Ok(())
        } else {
            error!("Failed to get chain for received bill block {bill_id}");
            Err(Error::BlockChain(
                "Failed to get chain for bill".to_string(),
            ))
        }
    }

    async fn add_new_chain(&self, blocks: Vec<BillBlock>, keys: &BillKeys) -> Result<()> {
        let (bill_id, chain) = self.get_valid_chain(blocks, keys)?;
        for block in chain.blocks() {
            self.save_block(&bill_id, block).await?;
        }
        self.save_keys(&bill_id, keys).await?;
        Ok(())
    }

    fn get_valid_chain(
        &self,
        blocks: Vec<BillBlock>,
        keys: &BillKeys,
    ) -> Result<(String, BillBlockchain)> {
        match BillBlockchain::new_from_blocks(blocks) {
            Ok(chain) if chain.is_chain_valid() => match chain.get_first_version_bill(keys) {
                Ok(bill) => Ok((bill.id, chain)),
                Err(e) => {
                    error!(
                        "Failed to get first version bill from newly received chain: {}",
                        e
                    );
                    Err(Error::Crypto(format!(
                        "Failed to decrypt new bill chain with given keys: {e}"
                    )))
                }
            },
            _ => {
                error!("Newly received chain is not valid");
                Err(Error::BlockChain(
                    "Newly received chain is not valid".to_string(),
                ))
            }
        }
    }

    async fn save_block(&self, bill_id: &str, block: &BillBlock) -> Result<()> {
        if let Err(e) = self.bill_blockchain_store.add_block(bill_id, block).await {
            error!("Failed to add block to blockchain store: {}", e);
            return Err(Error::Persistence(
                "Failed to add block to blockchain store".to_string(),
            ));
        }
        Ok(())
    }

    async fn save_keys(&self, bill_id: &str, keys: &BillKeys) -> Result<()> {
        if let Err(e) = self.bill_store.save_keys(bill_id, keys).await {
            error!("Failed to save keys to bill store: {}", e);
            return Err(Error::Persistence(
                "Failed to save keys to bill store".to_string(),
            ));
        }
        Ok(())
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
        if let Ok(decoded) = Event::<BillChainEventPayload>::try_from(event.clone()) {
            if !decoded.data.blocks.is_empty() {
                if let Err(e) = self
                    .process_chain_data(
                        &decoded.data.bill_id,
                        decoded.data.blocks.clone(),
                        decoded.data.keys.clone(),
                    )
                    .await
                {
                    error!("Failed to process chain data: {}", e);
                }
            }
            if let Err(e) = self.create_notification(&decoded.data, node_id).await {
                error!("Failed to create notification for bill event: {}", e);
            }
        } else {
            warn!("Could not decode event to BillChainEventPayload {event:?}");
        }
        Ok(())
    }
}

// generates a human readable description for an event
fn event_description(event_type: &BillEventType) -> String {
    match event_type {
        BillEventType::BillSigned => "Bill has been signed".to_string(),
        BillEventType::BillAccepted => "Bill has been accepted".to_string(),
        BillEventType::BillAcceptanceRequested => "Bill should be accepted".to_string(),
        BillEventType::BillAcceptanceRejected => "Bill acceptance has been rejected".to_string(),
        BillEventType::BillAcceptanceTimeout => "Bill acceptance has taken too long".to_string(),
        BillEventType::BillAcceptanceRecourse => "Bill in recourse should be accepted".to_string(),
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
