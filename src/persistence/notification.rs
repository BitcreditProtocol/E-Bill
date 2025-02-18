use async_trait::async_trait;

use super::Result;
use crate::data::notification::{Notification, NotificationType};
use crate::service::notification_service::ActionType;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait NotificationStoreApi: Send + Sync {
    /// Stores a new notification into the database
    async fn add(&self, notification: Notification) -> Result<Notification>;
    /// Returns all currently active notifications from the database
    async fn list(&self, filter: NotificationFilter) -> Result<Vec<Notification>>;
    /// Returns the latest active notification for the given reference and notification type
    async fn get_latest_by_reference(
        &self,
        reference: &str,
        notification_type: NotificationType,
    ) -> Result<Option<Notification>>;
    /// Returns all notifications for the given reference and notification type that are active
    #[allow(unused)]
    async fn list_by_type(&self, notification_type: NotificationType) -> Result<Vec<Notification>>;
    /// Marks an active notification as done
    async fn mark_as_done(&self, notification_id: &str) -> Result<()>;
    /// deletes a notification from the database
    #[allow(unused)]
    async fn delete(&self, notification_id: &str) -> Result<()>;
    /// marks a notification with specific type as sent for the current block of given bill
    async fn set_bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action_type: ActionType,
    ) -> Result<()>;
    /// lookup whether a notification has been sent for the given bill and block height
    async fn bill_notification_sent(
        &self,
        bill_id: &str,
        block_height: i32,
        action_type: ActionType,
    ) -> Result<bool>;
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct NotificationFilter {
    pub active: Option<bool>,
    pub reference_id: Option<String>,
    pub notification_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl NotificationFilter {
    pub fn filters(&self) -> String {
        let mut parts = vec![];
        if self.active.is_some() {
            parts.push("active = $active");
        }
        if self.reference_id.is_some() {
            parts.push("reference_id = $reference_id");
        }
        if self.notification_type.is_some() {
            parts.push("notification_type = $notification_type");
        }

        let filters = parts.join(" AND ");
        if filters.is_empty() {
            filters
        } else {
            format!("WHERE {}", filters)
        }
    }

    pub fn get_limit(&self) -> i64 {
        self.limit.unwrap_or(200)
    }

    pub fn get_offset(&self) -> i64 {
        self.offset.unwrap_or(0)
    }

    pub fn get_active(&self) -> Option<(String, bool)> {
        self.active.map(|active| ("active".to_string(), active))
    }

    pub fn get_reference_id(&self) -> Option<(String, String)> {
        self.reference_id
            .as_ref()
            .map(|reference_id| ("reference_id".to_string(), reference_id.to_string()))
    }

    pub fn get_notification_type(&self) -> Option<(String, String)> {
        self.notification_type.as_ref().map(|notification_type| {
            (
                "notification_type".to_string(),
                notification_type.to_string(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_query_filters() {
        let empty = super::NotificationFilter::default();
        assert_eq!(empty.filters(), "");

        let active = super::NotificationFilter {
            active: Some(true),
            ..Default::default()
        };
        assert_eq!(active.filters(), "WHERE active = $active");

        let all = super::NotificationFilter {
            active: Some(true),
            reference_id: Some("123".to_string()),
            notification_type: Some("Bill".to_string()),
            ..Default::default()
        };

        assert_eq!(
            all.filters(),
            "WHERE active = $active AND reference_id = $reference_id AND notification_type = $notification_type"
        );
    }
}
