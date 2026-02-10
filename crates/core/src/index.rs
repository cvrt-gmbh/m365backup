use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Maps blob hashes to their pack file location.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub entries: HashMap<[u8; 32], BlobLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobLocation {
    pub pack_id: String,
    pub offset: u32,
    pub length: u32,
}

impl Index {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, hash: [u8; 32], pack_id: String, offset: u32, length: u32) {
        self.entries.insert(
            hash,
            BlobLocation {
                pack_id,
                offset,
                length,
            },
        );
    }

    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.entries.contains_key(hash)
    }

    pub fn lookup(&self, hash: &[u8; 32]) -> Option<&BlobLocation> {
        self.entries.get(hash)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }
}
