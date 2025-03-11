use crate::Result;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

pub mod email_sendgrid;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait NotificationEmailTransportApi: Send + Sync {
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
