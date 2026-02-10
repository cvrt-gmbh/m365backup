use crate::crypto;
use serde::{Deserialize, Serialize};

const MIN_CHUNK: u32 = 512 * 1024; // 512 KB
const AVG_CHUNK: u32 = 1024 * 1024; // 1 MB
const MAX_CHUNK: u32 = 8 * 1024 * 1024; // 8 MB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRef {
    pub hash: [u8; 32],
    pub length: u64,
    pub offset: u64,
}

impl ChunkRef {
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

pub struct Chunker;

impl Chunker {
    pub fn chunk(data: &[u8]) -> Vec<Chunk> {
        let chunker = fastcdc::v2020::FastCDC::new(data, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK);
        chunker
            .map(|fc| {
                let slice = &data[fc.offset..fc.offset + fc.length];
                let hash = crypto::hash_blake3(slice);
                Chunk {
                    hash,
                    offset: fc.offset as u64,
                    length: fc.length as u64,
                    data: slice.to_vec(),
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub hash: [u8; 32],
    pub offset: u64,
    pub length: u64,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }

    pub fn to_ref(&self) -> ChunkRef {
        ChunkRef {
            hash: self.hash,
            length: self.length,
            offset: self.offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_deterministic() {
        let data = vec![42u8; 4 * 1024 * 1024];
        let chunks1 = Chunker::chunk(&data);
        let chunks2 = Chunker::chunk(&data);
        assert_eq!(chunks1.len(), chunks2.len());
        for (a, b) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(a.hash, b.hash);
        }
    }

    #[test]
    fn small_data_single_chunk() {
        let data = vec![1u8; 1024];
        let chunks = Chunker::chunk(&data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data.len(), 1024);
    }
}
