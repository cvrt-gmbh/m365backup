use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::chunk::ChunkRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub tenant: String,
    pub service: Service,
    pub user: Option<String>,
    pub parent: Option<String>,
    pub tree: Tree,
    pub delta_tokens: HashMap<String, String>,
    pub stats: BackupStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    OneDrive,
    Exchange,
    SharePoint,
    Teams,
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Service::OneDrive => write!(f, "onedrive"),
            Service::Exchange => write!(f, "exchange"),
            Service::SharePoint => write!(f, "sharepoint"),
            Service::Teams => write!(f, "teams"),
        }
    }
}

impl std::str::FromStr for Service {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "onedrive" => Ok(Service::OneDrive),
            "exchange" => Ok(Service::Exchange),
            "sharepoint" => Ok(Service::SharePoint),
            "teams" => Ok(Service::Teams),
            _ => anyhow::bail!("unknown service: {s}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tree {
    pub nodes: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub path: String,
    pub node_type: NodeType,
    pub size: u64,
    pub modified: Option<DateTime<Utc>>,
    pub chunks: Vec<ChunkRef>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    File,
    Directory,
    Mail,
    Calendar,
    Contact,
    Message,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupStats {
    pub total_items: u64,
    pub new_items: u64,
    pub unchanged_items: u64,
    pub total_bytes: u64,
    pub new_bytes: u64,
    pub deduplicated_bytes: u64,
    pub duration_secs: f64,
}

impl Snapshot {
    pub fn new(tenant: String, service: Service, user: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            tenant,
            service,
            user,
            parent: None,
            tree: Tree { nodes: Vec::new() },
            delta_tokens: HashMap::new(),
            stats: BackupStats::default(),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }

    pub fn short_id(&self) -> &str {
        &self.id[..8]
    }
}
