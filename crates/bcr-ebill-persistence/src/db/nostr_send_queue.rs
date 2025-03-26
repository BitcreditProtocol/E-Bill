use super::Result;
#[cfg(target_arch = "wasm32")]
use super::get_new_surreal_db;
use crate::{
    constants::{DB_IDS, DB_LIMIT, DB_TABLE},
    util::date::{self, DateTimeUtc},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{Surreal, engine::any::Any, sql::Thing};

use crate::nostr::{NostrQueuedMessage, NostrQueuedMessageStoreApi};

#[derive(Clone)]
pub struct SurrealNostrEventQueueStore {
    #[allow(dead_code)]
    db: Surreal<Any>,
}

impl SurrealNostrEventQueueStore {
    const TABLE: &'static str = "nostr_event_send_queue";

    #[allow(dead_code)]
    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn set_processing(&self, ids: Vec<Thing>) -> Result<()> {
        self.db()
            .await?
            .query("UPDATE type::table($table) SET processing = true WHERE id IN $ids")
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_IDS, ids))
            .await?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    async fn db(&self) -> Result<Surreal<Any>> {
        get_new_surreal_db().await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn db(&self) -> Result<Surreal<Any>> {
        Ok(self.db.clone())
    }
}

#[async_trait]
impl NostrQueuedMessageStoreApi for SurrealNostrEventQueueStore {
    /// Adds a new retry message
    async fn add_message(&self, message: NostrQueuedMessage, max_retries: i32) -> Result<()> {
        let id = message.id.to_owned();
        let message = QueuedMessageDb::from(message, max_retries);
        let _: Option<QueuedMessageDb> = self
            .db()
            .await?
            .create((Self::TABLE, id.to_owned()))
            .content(message)
            .await?;
        Ok(())
    }
    /// Selects all messages that are ready to be retried
    async fn get_retry_messages(&self, limit: u64) -> Result<Vec<NostrQueuedMessage>> {
        let items: Vec<QueuedMessageDb> = self
            .db().await?
            .query("SELECT * FROM type::table($table) WHERE completed = false AND processing = false ORDER BY last_try ASC LIMIT $limit")
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_LIMIT, limit))
            .await?
            .take(0)?;
        let ids = items.iter().map(|i| i.id.to_owned()).collect();
        let results: Vec<NostrQueuedMessage> = items.into_iter().map(|i| i.into()).collect();
        self.set_processing(ids).await?;
        Ok(results)
    }

    /// Fail a retry attempt, schedules a new retry or fails the message if
    /// all retries have been exhausted.
    async fn fail_retry(&self, id: &str) -> Result<()> {
        let current: Option<QueuedMessageDb> = self
            .db()
            .await?
            .select((Self::TABLE, id.to_owned()))
            .await?;
        if let Some(mut msg) = current {
            msg.num_retries += 1;
            msg.last_try = date::now();
            msg.completed = msg.num_retries >= msg.max_retries;
            msg.processing = false;
            let _: Option<QueuedMessageDb> = self
                .db()
                .await?
                .update((Self::TABLE, id.to_owned()))
                .content(msg)
                .await?;
        }
        Ok(())
    }
    /// Flags a retry as successful
    async fn succeed_retry(&self, id: &str) -> Result<()> {
        let current: Option<QueuedMessageDb> = self
            .db()
            .await?
            .select((Self::TABLE, id.to_owned()))
            .await?;
        if let Some(mut msg) = current {
            msg.completed = true;
            msg.last_try = date::now();
            msg.processing = false;
            let _: Option<QueuedMessageDb> = self
                .db()
                .await?
                .update((Self::TABLE, id.to_owned()))
                .content(msg)
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueuedMessageDb {
    pub id: Thing,
    pub sender_id: String,
    pub node_id: String,
    pub payload: Value,
    pub created: DateTimeUtc,
    pub last_try: DateTimeUtc,
    pub num_retries: i32,
    pub max_retries: i32,
    pub completed: bool,
    pub processing: bool,
}

impl QueuedMessageDb {
    fn from(value: NostrQueuedMessage, max_retries: i32) -> Self {
        QueuedMessageDb {
            id: Thing::from((
                SurrealNostrEventQueueStore::TABLE.to_owned(),
                value.id.to_owned(),
            )),
            sender_id: value.sender_id,
            node_id: value.node_id,
            payload: value.payload,
            created: date::now(),
            last_try: date::seconds(0),
            num_retries: 0,
            max_retries,
            completed: false,
            processing: false,
        }
    }
}

impl From<QueuedMessageDb> for NostrQueuedMessage {
    fn from(value: QueuedMessageDb) -> Self {
        NostrQueuedMessage {
            id: value.id.id.to_raw(),
            sender_id: value.sender_id,
            node_id: value.node_id,
            payload: value.payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::get_memory_db;

    #[tokio::test]
    async fn test_insert_query_and_mark_succeeded() {
        let store = get_store().await;
        store
            .add_message(get_test_message("test_message"), 3)
            .await
            .expect("could not add message");

        let messages = store
            .get_retry_messages(1)
            .await
            .expect("could not get messages");
        assert!(!messages.is_empty(), "should have gotten a queued message");

        let messages_empty = store
            .get_retry_messages(1)
            .await
            .expect("could not get messages");

        assert!(
            messages_empty.is_empty(),
            "should not have gotten a queued message"
        );

        store
            .succeed_retry(&messages[0].id)
            .await
            .expect("could not mark message as succeeded");

        let messages_done = store
            .get_retry_messages(1)
            .await
            .expect("could not get messages");
        assert!(
            messages_done.is_empty(),
            "should not have gotten a queued message"
        );
    }

    #[tokio::test]
    async fn test_insert_query_and_mark_failed() {
        let store = get_store().await;
        store
            .add_message(get_test_message("test_message"), 2)
            .await
            .expect("could not add message");

        let messages = store
            .get_retry_messages(1)
            .await
            .expect("could not get messages");
        assert!(!messages.is_empty(), "should have gotten a queued message");

        let messages_empty = store
            .get_retry_messages(1)
            .await
            .expect("could not get messages");

        assert!(
            messages_empty.is_empty(),
            "should not have gotten a queued message"
        );

        store
            .fail_retry(&messages[0].id)
            .await
            .expect("could not mark message as failed");

        let messages_failed = store
            .get_retry_messages(1)
            .await
            .expect("could not get failed messages");

        assert!(
            !messages_failed.is_empty(),
            "should have gotten a failed message"
        );

        store
            .fail_retry(&messages_failed[0].id)
            .await
            .expect("could not mark message as failed");

        let messages_failed_again = store
            .get_retry_messages(1)
            .await
            .expect("could not get failed messages");

        assert!(
            messages_failed_again.is_empty(),
            "should have exceeded retry limit"
        );
    }

    async fn get_store() -> SurrealNostrEventQueueStore {
        let mem_db = get_memory_db("test", "nostr_event_queue")
            .await
            .expect("could not create memory db");
        SurrealNostrEventQueueStore::new(mem_db)
    }

    fn get_test_message(id: &str) -> NostrQueuedMessage {
        NostrQueuedMessage {
            id: id.to_string(),
            sender_id: "test_sender".to_string(),
            node_id: "test_node".to_string(),
            payload: serde_json::json!({"foo": "bar"}),
        }
    }
}
