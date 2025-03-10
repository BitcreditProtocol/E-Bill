use crate::service::ServiceTraitBounds;

use super::Result;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

#[cfg(test)]
impl ServiceTraitBounds for MockNotificationEmailTransportApi {}

#[cfg_attr(test, automock)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait NotificationEmailTransportApi: ServiceTraitBounds {
    /// Generically send an email message to different email transports.
    #[allow(dead_code)]
    async fn send(&self, event: EmailMessage) -> Result<()>;
}

/// A simple email message. We can add more features (like html, multi recipient, etc.) later.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}
