use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Manages delta query tokens for incremental sync.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DeltaState {
    /// Map of "service:user:resource" → delta token
    pub tokens: HashMap<String, String>,
}

impl DeltaState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn key(service: &str, user: &str, resource: &str) -> String {
        format!("{service}:{user}:{resource}")
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.tokens.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: String, token: String) {
        self.tokens.insert(key, token);
    }
}
