use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::chunk::Chunk;

/// A pack file bundles multiple blobs into a single storage object.
/// Format: [blob1_data][blob2_data]...[blobN_data][pack_header_json][header_length: u32 LE]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackHeader {
    pub id: String,
    pub blobs: Vec<PackedBlob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedBlob {
    pub hash: [u8; 32],
    pub offset: u32,
    pub length: u32,
}

const TARGET_PACK_SIZE: usize = 16 * 1024 * 1024; // 16 MB

pub struct PackBuilder {
    id: String,
    data: Vec<u8>,
    blobs: Vec<PackedBlob>,
}

impl Default for PackBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PackBuilder {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            data: Vec::new(),
            blobs: Vec::new(),
        }
    }

    pub fn add(&mut self, chunk: &Chunk) {
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(&chunk.data);
        self.blobs.push(PackedBlob {
            hash: chunk.hash,
            offset,
            length: chunk.data.len() as u32,
        });
    }

    pub fn should_flush(&self) -> bool {
        self.data.len() >= TARGET_PACK_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    pub fn finalize(self) -> Result<PackFile> {
        let header = PackHeader {
            id: self.id,
            blobs: self.blobs,
        };
        let header_json = serde_json::to_vec(&header)?;
        let header_len = header_json.len() as u32;

        let mut data = self.data;
        data.extend_from_slice(&header_json);
        data.extend_from_slice(&header_len.to_le_bytes());

        Ok(PackFile { header, data })
    }
}

pub struct PackFile {
    pub header: PackHeader,
    pub data: Vec<u8>,
}

impl PackFile {
    pub fn id(&self) -> &str {
        &self.header.id
    }

    /// Parse a pack file from raw bytes.
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        if data.len() < 4 {
            anyhow::bail!("pack file too small");
        }
        let len = data.len();
        let header_len = u32::from_le_bytes(data[len - 4..].try_into().unwrap()) as usize;
        if len < 4 + header_len {
            anyhow::bail!("pack file corrupted: header length exceeds file size");
        }
        let header_start = len - 4 - header_len;
        let header: PackHeader = serde_json::from_slice(&data[header_start..len - 4])?;
        Ok(Self { header, data })
    }

    /// Extract a blob by its hash.
    pub fn extract_blob(&self, hash: &[u8; 32]) -> Option<&[u8]> {
        self.header
            .blobs
            .iter()
            .find(|b| &b.hash == hash)
            .map(|b| &self.data[b.offset as usize..(b.offset + b.length) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;

    #[test]
    fn pack_roundtrip() {
        let mut builder = PackBuilder::new();
        let data1 = b"hello world";
        let data2 = b"goodbye world";
        let chunk1 = Chunk {
            hash: crypto::hash_blake3(data1),
            offset: 0,
            length: data1.len() as u64,
            data: data1.to_vec(),
        };
        let chunk2 = Chunk {
            hash: crypto::hash_blake3(data2),
            offset: 0,
            length: data2.len() as u64,
            data: data2.to_vec(),
        };
        builder.add(&chunk1);
        builder.add(&chunk2);

        let pack = builder.finalize().unwrap();
        let parsed = PackFile::parse(pack.data).unwrap();

        assert_eq!(parsed.extract_blob(&chunk1.hash).unwrap(), data1);
        assert_eq!(parsed.extract_blob(&chunk2.hash).unwrap(), data2);
    }
}
