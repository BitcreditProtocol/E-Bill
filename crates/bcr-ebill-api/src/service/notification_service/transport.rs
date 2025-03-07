use super::{EventEnvelope, Result};
use crate::{data::contact::IdentityPublicData, service::ServiceTraitBounds};
use async_trait::async_trait;
use log::info;
#[cfg(test)]
use mockall::automock;

#[cfg(test)]
impl ServiceTraitBounds for MockNotificationJsonTransportApi {}

#[cfg_attr(test, automock)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait NotificationJsonTransportApi: ServiceTraitBounds {
    async fn send(&self, recipient: &IdentityPublicData, event: EventEnvelope) -> Result<()>;
}

/// A dummy transport that logs all events that are sent as json.
pub struct LoggingNotificationJsonTransport;

impl ServiceTraitBounds for LoggingNotificationJsonTransport {}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl NotificationJsonTransportApi for LoggingNotificationJsonTransport {
    async fn send(&self, recipient: &IdentityPublicData, event: EventEnvelope) -> Result<()> {
        info!(
            "Sending json event: {:?}({}) with payload: {:?} to peer: {}",
            event.event_type, event.version, event.data, recipient.node_id
        );
        Ok(())
    }
}
