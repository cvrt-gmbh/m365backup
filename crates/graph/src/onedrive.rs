use anyhow::Result;
use serde::Deserialize;
use tracing::{debug, info};

use crate::client::GraphClient;

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: String,
    pub mail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DriveItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "parentReference")]
    pub parent_reference: Option<ParentRef>,
    pub size: Option<u64>,
    pub file: Option<FileInfo>,
    pub folder: Option<FolderInfo>,
    #[serde(rename = "lastModifiedDateTime")]
    pub last_modified: Option<String>,
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    pub download_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParentRef {
    pub path: Option<String>,
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileInfo {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub hashes: Option<Hashes>,
}

#[derive(Debug, Deserialize)]
pub struct Hashes {
    #[serde(rename = "quickXorHash")]
    pub quick_xor_hash: Option<String>,
    #[serde(rename = "sha256Hash")]
    pub sha256_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FolderInfo {
    #[serde(rename = "childCount")]
    pub child_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DeltaResponse {
    pub value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta_link: Option<String>,
}

pub struct OneDriveClient<'a> {
    graph: &'a GraphClient,
}

impl<'a> OneDriveClient<'a> {
    pub fn new(graph: &'a GraphClient) -> Self {
        Self { graph }
    }

    /// List all licensed users.
    pub async fn list_users(&self) -> Result<Vec<User>> {
        info!("Enumerating users");
        self.graph
            .get_all_pages("/users?$select=id,displayName,userPrincipalName,mail&$top=999")
            .await
    }

    /// Get all drive items using delta query (initial or incremental).
    pub async fn get_drive_delta(
        &self,
        user_id: &str,
        delta_token: Option<&str>,
    ) -> Result<(Vec<DriveItem>, Option<String>)> {
        let url = match delta_token {
            Some(token) => token.to_string(),
            None => format!(
                "/users/{user_id}/drive/root/delta?$select=id,name,parentReference,size,file,folder,lastModifiedDateTime"
            ),
        };

        let mut all_items = Vec::new();
        let mut current_url = url;

        let new_delta_token = loop {
            let page: DeltaResponse = self.graph.get_json(&current_url).await?;
            let count = page.value.len();
            all_items.extend(page.value);
            debug!(items = count, "fetched delta page");

            if let Some(next) = page.next_link {
                current_url = next;
            } else {
                break page.delta_link;
            }
        };

        info!(total_items = all_items.len(), "delta sync complete");
        Ok((all_items, new_delta_token))
    }

    /// Download a file's content.
    pub async fn download_file(&self, user_id: &str, item_id: &str) -> Result<bytes::Bytes> {
        let url = format!("/users/{user_id}/drive/items/{item_id}/content");
        self.graph.get_bytes(&url).await
    }

    /// Download from a pre-authenticated URL.
    pub async fn download_url(&self, url: &str) -> Result<bytes::Bytes> {
        // Download URLs are pre-authenticated, no auth header needed
        let resp = reqwest::get(url).await?;
        if !resp.status().is_success() {
            anyhow::bail!("download failed: {}", resp.status());
        }
        Ok(resp.bytes().await?)
    }

    /// Build the full path for a drive item.
    pub fn item_path(item: &DriveItem) -> String {
        if let Some(ref parent) = item.parent_reference {
            if let Some(ref path) = parent.path {
                // path looks like "/drive/root:/folder/subfolder"
                let clean = path.split(":/").nth(1).unwrap_or("");
                if clean.is_empty() {
                    item.name.clone()
                } else {
                    format!("{clean}/{}", item.name)
                }
            } else {
                item.name.clone()
            }
        } else {
            item.name.clone()
        }
    }
}
