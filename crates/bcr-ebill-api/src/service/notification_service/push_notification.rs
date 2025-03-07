use async_trait::async_trait;
use log::error;
use std::sync::Arc;

use async_broadcast::{InactiveReceiver, Receiver, Sender};
#[cfg(test)]
use mockall::automock;
use serde_json::Value;

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
    _receiver: InactiveReceiver<Value>, // keep receiver around, so channel doesn't get closed
}

impl PushService {
    pub fn new() -> Self {
        let (tx, rx) = async_broadcast::broadcast::<Value>(5);
        let inactive = rx.deactivate();
        Self {
            sender: Arc::new(tx),
            _receiver: inactive,
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
        match self.sender.broadcast(value).await {
            Ok(_) => {}
            Err(err) => {
                error!("Error sending push message: {}", err);
            }
        }
    }

    async fn subscribe(&self) -> Receiver<Value> {
        self.sender.new_receiver()
    }
}
