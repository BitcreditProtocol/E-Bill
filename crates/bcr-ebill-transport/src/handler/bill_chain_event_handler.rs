use super::NotificationHandlerApi;
use crate::BillChainEventPayload;
use crate::EventType;
use crate::{Error, Event, EventEnvelope, PushApi, Result};
use async_trait::async_trait;
use bcr_ebill_core::ServiceTraitBounds;
use bcr_ebill_core::bill::BillKeys;
use bcr_ebill_core::blockchain::Blockchain;
use bcr_ebill_core::blockchain::bill::{BillBlock, BillBlockchain};
use bcr_ebill_core::notification::BillEventType;
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
        // no action no notification required
        if event.action_type.is_none() {
            return Ok(());
        }
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

#[cfg(test)]
mod tests {
    use bcr_ebill_core::{
        OptionalPostalAddress, PostalAddress,
        bill::BitcreditBill,
        blockchain::bill::block::{BillEndorseBlockData, BillIssueBlockData},
        contact::{ContactType, IdentityPublicData},
        identity::{Identity, IdentityWithAll},
        notification::ActionType,
        util::BcrKeys,
    };
    use mockall::predicate::{always, eq};

    use crate::handler::test_utils::{
        MockBillChainStore, MockBillStore, MockNotificationStore, MockPushService,
    };

    use super::*;

    #[tokio::test]
    async fn test_create_event_handler() {
        let (notification_store, push_service, bill_chain_store, bill_store) = create_mocks();
        BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
    }

    #[tokio::test]
    async fn test_creates_new_chain_for_new_chain_event() {
        let payer = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));
        let keys = get_bill_keys();

        let (notification_store, push_service, mut bill_chain_store, mut bill_store) =
            create_mocks();

        bill_chain_store
            .expect_add_block()
            .with(eq("bill"), eq(chain.blocks()[0].clone()))
            .times(1)
            .returning(move |_, _| Ok(()));

        bill_store
            .expect_save_keys()
            .with(eq("bill"), always())
            .times(1)
            .returning(move |_, _| Ok(()));

        let handler = BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
        let event = Event::new(
            EventType::Bill,
            "node_id",
            BillChainEventPayload {
                bill_id: "bill_id".to_string(),
                event_type: BillEventType::BillBlock,
                blocks: chain.blocks().clone(),
                keys: Some(keys.clone()),
                sum: Some(0),
                action_type: None,
            },
        );

        handler
            .handle_event(event.try_into().expect("Envelope from event"), "node_id")
            .await
            .expect("Event should be handled");
    }

    #[tokio::test]
    async fn test_adds_block_for_existing_chain_event() {
        let payer = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let endorsee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));
        let block = BillBlock::create_block_for_endorse(
            "bill".to_string(),
            chain.get_latest_block(),
            &BillEndorseBlockData {
                endorsee: endorsee.clone().into(),
                // endorsed by payee
                endorser: IdentityPublicData::new(get_baseline_identity().identity)
                    .unwrap()
                    .into(),
                signatory: None,
                signing_timestamp: 1000,
                signing_address: empty_address(),
            },
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1000,
        )
        .unwrap();

        let (notification_store, push_service, mut bill_chain_store, bill_store) = create_mocks();

        let chain_clone = chain.clone();
        bill_chain_store
            .expect_get_chain()
            .with(eq("bill"))
            .times(1)
            .returning(move |_| Ok(chain_clone.clone()));

        bill_chain_store
            .expect_add_block()
            .with(eq("bill"), eq(block.clone()))
            .times(1)
            .returning(move |_, _| Ok(()));

        let handler = BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
        let event = Event::new(
            EventType::Bill,
            "node_id",
            BillChainEventPayload {
                bill_id: "bill".to_string(),
                event_type: BillEventType::BillBlock,
                blocks: vec![block.clone()],
                keys: None,
                sum: Some(0),
                action_type: None,
            },
        );

        handler
            .handle_event(event.try_into().expect("Envelope from event"), "node_id")
            .await
            .expect("Event should be handled");
    }

    #[tokio::test]
    async fn test_fails_to_add_block_for_unknown_chain() {
        let payer = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let payee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let endorsee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        let bill = get_test_bitcredit_bill("bill", &payer, &payee, None, None);
        let chain = get_genesis_chain(Some(bill.clone()));

        let block = BillBlock::create_block_for_endorse(
            "bill".to_string(),
            chain.get_latest_block(),
            &BillEndorseBlockData {
                endorsee: endorsee.clone().into(),
                // endorsed by payee
                endorser: IdentityPublicData::new(get_baseline_identity().identity)
                    .unwrap()
                    .into(),
                signatory: None,
                signing_timestamp: 1000,
                signing_address: empty_address(),
            },
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            Some(&BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap()),
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1000,
        )
        .unwrap();

        let (notification_store, push_service, mut bill_chain_store, bill_store) = create_mocks();

        bill_chain_store
            .expect_get_chain()
            .with(eq("bill"))
            .times(1)
            .returning(move |_| Err(bcr_ebill_persistence::Error::NoBillBlock));

        bill_chain_store.expect_add_block().never();

        let handler = BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
        let event = Event::new(
            EventType::Bill,
            "node_id",
            BillChainEventPayload {
                bill_id: "bill".to_string(),
                event_type: BillEventType::BillBlock,
                blocks: vec![block.clone()],
                keys: None,
                sum: Some(0),
                action_type: None,
            },
        );

        handler
            .handle_event(event.try_into().expect("Envelope from event"), "node_id")
            .await
            .expect("Event should be handled");
    }

    #[tokio::test]
    async fn test_creates_no_notification_for_non_action_event() {
        let (mut notification_store, mut push_service, bill_chain_store, bill_store) =
            create_mocks();

        // look for currently active notification
        notification_store.expect_get_latest_by_reference().never();

        // store new notification
        notification_store.expect_add().never();

        // send push notification
        push_service.expect_send().never();

        let handler = BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
        let event = Event::new(
            EventType::Bill,
            "node_id",
            BillChainEventPayload {
                bill_id: "bill_id".to_string(),
                event_type: BillEventType::BillBlock,
                blocks: vec![],
                keys: None,
                sum: None,
                action_type: None,
            },
        );

        handler
            .handle_event(event.try_into().expect("Envelope from event"), "node_id")
            .await
            .expect("Event should be handled");
    }

    #[tokio::test]
    async fn test_creates_notification_for_simple_action_event() {
        let (mut notification_store, mut push_service, bill_chain_store, bill_store) =
            create_mocks();

        // look for currently active notification
        notification_store
            .expect_get_latest_by_reference()
            .with(eq("bill_id"), eq(NotificationType::Bill))
            .times(1)
            .returning(|_, _| Ok(None));

        // store new notification
        notification_store.expect_add().times(1).returning(|_| {
            Ok(Notification::new_bill_notification(
                "bill_id",
                "node_id",
                "description",
                None,
            ))
        });

        // send push notification
        push_service.expect_send().times(1).returning(|_| ());

        let handler = BillChainEventHandler::new(
            Arc::new(notification_store),
            Arc::new(push_service),
            Arc::new(bill_chain_store),
            Arc::new(bill_store),
        );
        let event = Event::new(
            EventType::Bill,
            "node_id",
            BillChainEventPayload {
                bill_id: "bill_id".to_string(),
                event_type: BillEventType::BillSigned,
                blocks: vec![],
                keys: None,
                sum: Some(0),
                action_type: Some(ActionType::CheckBill),
            },
        );

        handler
            .handle_event(event.try_into().expect("Envelope from event"), "node_id")
            .await
            .expect("Event should be handled");
    }
    pub fn get_test_bitcredit_bill(
        id: &str,
        payer: &IdentityPublicData,
        payee: &IdentityPublicData,
        drawer: Option<&IdentityPublicData>,
        endorsee: Option<&IdentityPublicData>,
    ) -> BitcreditBill {
        let mut bill = empty_bitcredit_bill();
        bill.id = id.to_owned();
        bill.payee = payee.clone();
        bill.drawee = payer.clone();
        if let Some(drawer) = drawer {
            bill.drawer = drawer.clone();
        }
        bill.endorsee = endorsee.cloned();
        bill
    }
    fn get_genesis_chain(bill: Option<BitcreditBill>) -> BillBlockchain {
        let bill = bill.unwrap_or(get_baseline_bill("some id"));
        BillBlockchain::new(
            &BillIssueBlockData::from(bill, None, 1731593928),
            get_baseline_identity().key_pair,
            None,
            BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap()
    }
    fn get_baseline_bill(bill_id: &str) -> BitcreditBill {
        let mut bill = empty_bitcredit_bill();
        let keys = BcrKeys::new();

        bill.maturity_date = "2099-10-15".to_string();
        bill.payee = empty_identity_public_data();
        bill.payee.name = "payee".to_owned();
        bill.payee.node_id = keys.get_public_key();
        bill.drawee = IdentityPublicData::new(get_baseline_identity().identity).unwrap();
        bill.id = bill_id.to_owned();
        bill
    }
    fn empty_bitcredit_bill() -> BitcreditBill {
        BitcreditBill {
            id: "".to_string(),
            country_of_issuing: "".to_string(),
            city_of_issuing: "".to_string(),
            drawee: empty_identity_public_data(),
            drawer: empty_identity_public_data(),
            payee: empty_identity_public_data(),
            endorsee: None,
            currency: "".to_string(),
            sum: 0,
            maturity_date: "".to_string(),
            issue_date: "".to_string(),
            city_of_payment: "".to_string(),
            country_of_payment: "".to_string(),
            language: "".to_string(),
            files: vec![],
        }
    }

    pub fn get_bill_keys() -> BillKeys {
        BillKeys {
            private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
            public_key: TEST_PUB_KEY_SECP.to_owned(),
        }
    }

    fn get_baseline_identity() -> IdentityWithAll {
        let keys = BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap();
        let mut identity = empty_identity();
        identity.name = "drawer".to_owned();
        identity.node_id = keys.get_public_key();
        identity.postal_address.country = Some("AT".to_owned());
        identity.postal_address.city = Some("Vienna".to_owned());
        identity.postal_address.address = Some("Hayekweg 5".to_owned());
        IdentityWithAll {
            identity,
            key_pair: keys,
        }
    }
    fn empty_identity_public_data() -> IdentityPublicData {
        IdentityPublicData {
            t: ContactType::Person,
            node_id: "".to_string(),
            name: "".to_string(),
            postal_address: empty_address(),
            email: None,
            nostr_relay: None,
        }
    }
    fn empty_address() -> PostalAddress {
        PostalAddress {
            country: "".to_string(),
            city: "".to_string(),
            zip: None,
            address: "".to_string(),
        }
    }
    fn empty_identity() -> Identity {
        Identity {
            node_id: "".to_string(),
            name: "".to_string(),
            email: "".to_string(),
            postal_address: empty_optional_address(),
            date_of_birth: None,
            country_of_birth: None,
            city_of_birth: None,
            identification_number: None,
            nostr_relay: None,
            profile_picture_file: None,
            identity_document_file: None,
        }
    }

    pub fn empty_optional_address() -> OptionalPostalAddress {
        OptionalPostalAddress {
            country: None,
            city: None,
            zip: None,
            address: None,
        }
    }

    const TEST_PRIVATE_KEY_SECP: &str =
        "d1ff7427912d3b81743d3b67ffa1e65df2156d3dab257316cbc8d0f35eeeabe9";

    pub const TEST_PUB_KEY_SECP: &str =
        "02295fb5f4eeb2f21e01eaf3a2d9a3be10f39db870d28f02146130317973a40ac0";

    fn create_mocks() -> (
        MockNotificationStore,
        MockPushService,
        MockBillChainStore,
        MockBillStore,
    ) {
        (
            MockNotificationStore::new(),
            MockPushService::new(),
            MockBillChainStore::new(),
            MockBillStore::new(),
        )
    }
}
