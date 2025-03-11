use async_trait::async_trait;
use log::error;
use std::sync::Arc;

#[cfg(test)]
use mockall::automock;
use serde_json::Value;
use tokio::sync::broadcast::{Receiver, Sender};

#[cfg_attr(test, automock)]
#[async_trait]
pub trait PushApi: Send + Sync {
    /// Push a json message to the client
    async fn send(&self, value: Value);
    /// Subscribe to the message stream.
    async fn subscribe(&self) -> Receiver<Value>;
}

pub struct PushService {
    sender: Arc<Sender<Value>>,
}

impl PushService {
    pub fn new() -> Self {
        let (rx, _) = tokio::sync::broadcast::channel::<Value>(5);
        Self {
            sender: Arc::new(rx),
        }
    }
}

impl Default for PushService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PushApi for PushService {
    async fn send(&self, value: Value) {
        match self.sender.send(value) {
            Ok(_) => {}
            Err(err) => {
                error!("Error sending push message: {}", err);
            }
        }
    }

    async fn subscribe(&self) -> Receiver<Value> {
        self.sender.subscribe()
    }
}
