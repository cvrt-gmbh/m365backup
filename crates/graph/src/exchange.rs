use anyhow::Result;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::client::GraphClient;

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MailFolder {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "childFolderCount")]
    pub child_folder_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: String,
    pub subject: Option<String>,
    #[serde(rename = "receivedDateTime")]
    pub received_date_time: Option<String>,
    /// Present in delta responses for deleted items.
    #[serde(rename = "@removed")]
    pub removed: Option<RemovedMarker>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemovedMarker {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeltaResponse<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    pub subject: Option<String>,
    pub start: Option<DateTimeTimeZone>,
    pub end: Option<DateTimeTimeZone>,
    #[serde(rename = "@removed")]
    pub removed: Option<RemovedMarker>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DateTimeTimeZone {
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
    #[serde(rename = "timeZone")]
    pub time_zone: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactFolder {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Contact {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "@removed")]
    pub removed: Option<RemovedMarker>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct ExchangeClient<'a> {
    graph: &'a GraphClient,
}

impl<'a> ExchangeClient<'a> {
    pub fn new(graph: &'a GraphClient) -> Self {
        Self { graph }
    }

    // -- Mail ---------------------------------------------------------------

    /// List all mail folders for a user, including nested child folders.
    pub async fn list_all_mail_folders(&self, user_id: &str) -> Result<Vec<MailFolder>> {
        info!(user = %user_id, "Listing mail folders");
        let top_level: Vec<MailFolder> = self
            .graph
            .get_all_pages(&format!(
                "/users/{user_id}/mailFolders?$top=250&$select=id,displayName,childFolderCount"
            ))
            .await?;

        let mut all = Vec::new();
        for folder in &top_level {
            all.push(folder.clone());
            if folder.child_folder_count.unwrap_or(0) > 0 {
                self.collect_child_folders(user_id, &folder.id, &mut all)
                    .await?;
            }
        }
        debug!(count = all.len(), "total mail folders");
        Ok(all)
    }

    /// Recursively collect child folders.
    async fn collect_child_folders(
        &self,
        user_id: &str,
        parent_id: &str,
        out: &mut Vec<MailFolder>,
    ) -> Result<()> {
        let children: Vec<MailFolder> = self
            .graph
            .get_all_pages(&format!(
                "/users/{user_id}/mailFolders/{parent_id}/childFolders?$top=250&$select=id,displayName,childFolderCount"
            ))
            .await?;

        for child in &children {
            out.push(child.clone());
            if child.child_folder_count.unwrap_or(0) > 0 {
                Box::pin(self.collect_child_folders(user_id, &child.id, out)).await?;
            }
        }
        Ok(())
    }

    /// Delta query for messages in a mail folder.
    /// Returns (messages, new_delta_link). On 410 Gone returns `Ok(None)`.
    pub async fn get_mail_folder_delta(
        &self,
        user_id: &str,
        folder_id: &str,
        delta_token: Option<&str>,
    ) -> Result<Option<(Vec<Message>, Option<String>)>> {
        let url = match delta_token {
            Some(token) => token.to_string(),
            None => format!(
                "/users/{user_id}/mailFolders/{folder_id}/messages/delta?$select=id,subject,receivedDateTime"
            ),
        };

        self.paginate_delta::<Message>(&url).await
    }

    /// Download a message as MIME/EML bytes.
    pub async fn download_mime(&self, user_id: &str, message_id: &str) -> Result<bytes::Bytes> {
        let url = format!("/users/{user_id}/messages/{message_id}/$value");
        self.graph.get_bytes(&url).await
    }

    // -- Calendar -----------------------------------------------------------

    /// Delta query for calendar events (calendarView).
    /// Uses a wide window: 5 years back, 5 years forward.
    /// Returns (events, new_delta_link). On 410 Gone returns `Ok(None)`.
    pub async fn get_calendar_delta(
        &self,
        user_id: &str,
        delta_token: Option<&str>,
    ) -> Result<Option<(Vec<Event>, Option<String>)>> {
        let url = match delta_token {
            Some(token) => token.to_string(),
            None => {
                let now = chrono::Utc::now();
                let start = (now - chrono::Duration::days(5 * 365)).format("%Y-%m-%dT%H:%M:%SZ");
                let end = (now + chrono::Duration::days(5 * 365)).format("%Y-%m-%dT%H:%M:%SZ");
                format!(
                    "/users/{user_id}/calendarView/delta?startDateTime={start}&endDateTime={end}&$select=id,subject,start,end"
                )
            }
        };

        self.paginate_delta::<Event>(&url).await
    }

    /// Get full event JSON.
    pub async fn get_event(&self, user_id: &str, event_id: &str) -> Result<bytes::Bytes> {
        let url = format!("/users/{user_id}/events/{event_id}");
        let value: serde_json::Value = self.graph.get_json(&url).await?;
        Ok(bytes::Bytes::from(serde_json::to_vec_pretty(&value)?))
    }

    // -- Contacts -----------------------------------------------------------

    /// List all contact folders for a user.
    pub async fn list_contact_folders(&self, user_id: &str) -> Result<Vec<ContactFolder>> {
        info!(user = %user_id, "Listing contact folders");
        self.graph
            .get_all_pages(&format!(
                "/users/{user_id}/contactFolders?$top=250&$select=id,displayName"
            ))
            .await
    }

    /// Delta query for contacts in a folder.
    /// Returns (contacts, new_delta_link). On 410 Gone returns `Ok(None)`.
    pub async fn get_contacts_delta(
        &self,
        user_id: &str,
        folder_id: &str,
        delta_token: Option<&str>,
    ) -> Result<Option<(Vec<Contact>, Option<String>)>> {
        let url = match delta_token {
            Some(token) => token.to_string(),
            None => format!(
                "/users/{user_id}/contactFolders/{folder_id}/contacts/delta?$select=id,displayName"
            ),
        };

        self.paginate_delta::<Contact>(&url).await
    }

    /// Get full contact JSON.
    pub async fn get_contact(&self, user_id: &str, contact_id: &str) -> Result<bytes::Bytes> {
        let url = format!("/users/{user_id}/contacts/{contact_id}");
        let value: serde_json::Value = self.graph.get_json(&url).await?;
        Ok(bytes::Bytes::from(serde_json::to_vec_pretty(&value)?))
    }

    // -- Shared helpers -----------------------------------------------------

    /// Generic paginated delta query. Returns `Ok(None)` on HTTP 410 (token expired).
    async fn paginate_delta<T: serde::de::DeserializeOwned>(
        &self,
        initial_url: &str,
    ) -> Result<Option<(Vec<T>, Option<String>)>> {
        let mut items = Vec::new();
        let mut current_url = initial_url.to_string();

        loop {
            let page: DeltaResponse<T> = match self.graph.get_json_or_gone(&current_url).await? {
                Some(p) => p,
                None => {
                    warn!("delta token expired (410 Gone)");
                    return Ok(None);
                }
            };

            let count = page.value.len();
            items.extend(page.value);
            debug!(items = count, "fetched delta page");

            if let Some(next) = page.next_link {
                current_url = next;
            } else {
                return Ok(Some((items, page.delta_link)));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_mail_folder() {
        let json = r#"{"id":"abc","displayName":"Inbox","childFolderCount":2}"#;
        let f: MailFolder = serde_json::from_str(json).unwrap();
        assert_eq!(f.id, "abc");
        assert_eq!(f.display_name, "Inbox");
        assert_eq!(f.child_folder_count, Some(2));
    }

    #[test]
    fn deserialize_message() {
        let json = r#"{"id":"msg1","subject":"Hello","receivedDateTime":"2024-01-15T10:00:00Z"}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "msg1");
        assert_eq!(m.subject.as_deref(), Some("Hello"));
        assert!(m.removed.is_none());
    }

    #[test]
    fn deserialize_message_removed() {
        let json = r#"{"id":"msg2","subject":null,"@removed":{"reason":"deleted"}}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "msg2");
        assert!(m.removed.is_some());
        assert_eq!(m.removed.unwrap().reason.as_deref(), Some("deleted"));
    }

    #[test]
    fn deserialize_event() {
        let json = r#"{
            "id":"evt1",
            "subject":"Meeting",
            "start":{"dateTime":"2024-01-15T10:00:00","timeZone":"UTC"},
            "end":{"dateTime":"2024-01-15T11:00:00","timeZone":"UTC"}
        }"#;
        let e: Event = serde_json::from_str(json).unwrap();
        assert_eq!(e.id, "evt1");
        assert_eq!(e.subject.as_deref(), Some("Meeting"));
        assert!(e.start.is_some());
        assert!(e.removed.is_none());
    }

    #[test]
    fn deserialize_event_removed() {
        let json = r#"{"id":"evt2","subject":null,"start":null,"end":null,"@removed":{"reason":"deleted"}}"#;
        let e: Event = serde_json::from_str(json).unwrap();
        assert!(e.removed.is_some());
    }

    #[test]
    fn deserialize_contact() {
        let json = r#"{"id":"ct1","displayName":"John Doe"}"#;
        let c: Contact = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, "ct1");
        assert_eq!(c.display_name.as_deref(), Some("John Doe"));
        assert!(c.removed.is_none());
    }

    #[test]
    fn deserialize_contact_removed() {
        let json = r#"{"id":"ct2","displayName":null,"@removed":{"reason":"deleted"}}"#;
        let c: Contact = serde_json::from_str(json).unwrap();
        assert!(c.removed.is_some());
    }

    #[test]
    fn deserialize_contact_folder() {
        let json = r#"{"id":"cf1","displayName":"Contacts"}"#;
        let f: ContactFolder = serde_json::from_str(json).unwrap();
        assert_eq!(f.id, "cf1");
        assert_eq!(f.display_name, "Contacts");
    }

    #[test]
    fn deserialize_delta_response_with_next_link() {
        let json = r#"{
            "value":[{"id":"msg1","subject":"Hi","receivedDateTime":null}],
            "@odata.nextLink":"https://graph.microsoft.com/v1.0/next"
        }"#;
        let resp: DeltaResponse<Message> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.value.len(), 1);
        assert!(resp.next_link.is_some());
        assert!(resp.delta_link.is_none());
    }

    #[test]
    fn deserialize_delta_response_with_delta_link() {
        let json = r#"{
            "value":[],
            "@odata.deltaLink":"https://graph.microsoft.com/v1.0/delta?token=abc"
        }"#;
        let resp: DeltaResponse<Message> = serde_json::from_str(json).unwrap();
        assert!(resp.value.is_empty());
        assert!(resp.next_link.is_none());
        assert!(resp.delta_link.is_some());
    }
}
