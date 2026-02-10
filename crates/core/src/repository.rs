use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::backend::Backend;
use crate::chunk::Chunker;
use crate::index::Index;
use crate::pack::PackBuilder;
use crate::snapshot::Snapshot;

const CONFIG_PATH: &str = "config.json";
const INDEX_PATH: &str = "index.json";
const SNAPSHOTS_DIR: &str = "snapshots";
const PACKS_DIR: &str = "packs";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub version: u32,
    pub created: String,
    pub backend_type: String,
}

pub struct Repository {
    backend: Arc<dyn Backend>,
    index: Index,
}

impl Repository {
    /// Initialize a new repository.
    pub async fn init(backend: Arc<dyn Backend>, backend_type: &str) -> Result<Self> {
        if backend.exists(CONFIG_PATH).await? {
            anyhow::bail!("repository already initialized");
        }

        let config = RepoConfig {
            version: 1,
            created: chrono::Utc::now().to_rfc3339(),
            backend_type: backend_type.to_string(),
        };
        let config_bytes = serde_json::to_vec_pretty(&config)?;
        backend.write(CONFIG_PATH, &config_bytes).await?;

        let index = Index::new();
        backend.write(INDEX_PATH, &index.to_bytes()?).await?;

        info!("Repository initialized");
        Ok(Self { backend, index })
    }

    /// Open an existing repository.
    pub async fn open(backend: Arc<dyn Backend>) -> Result<Self> {
        if !backend.exists(CONFIG_PATH).await? {
            anyhow::bail!("not a valid repository (config.json missing)");
        }

        let config_bytes = backend.read(CONFIG_PATH).await?;
        let config: RepoConfig =
            serde_json::from_slice(&config_bytes).context("failed to parse repository config")?;
        if config.version != 1 {
            anyhow::bail!("unsupported repository version: {}", config.version);
        }

        let index = if backend.exists(INDEX_PATH).await? {
            let data = backend.read(INDEX_PATH).await?;
            Index::from_bytes(&data)?
        } else {
            Index::new()
        };

        Ok(Self { backend, index })
    }

    /// Store chunked data, deduplicating against existing blobs.
    pub async fn store_data(&mut self, data: &[u8]) -> Result<Vec<crate::chunk::ChunkRef>> {
        let chunks = Chunker::chunk(data);
        let mut refs = Vec::new();
        let mut builder = PackBuilder::new();

        for chunk in &chunks {
            refs.push(chunk.to_ref());

            if self.index.contains(&chunk.hash) {
                continue;
            }

            builder.add(chunk);

            if builder.should_flush() {
                self.flush_pack(builder).await?;
                builder = PackBuilder::new();
            }
        }

        if !builder.is_empty() {
            self.flush_pack(builder).await?;
        }

        Ok(refs)
    }

    /// Read data back from stored chunk references.
    pub async fn read_data(&self, chunk_refs: &[crate::chunk::ChunkRef]) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        for chunk_ref in chunk_refs {
            let location = self.index.lookup(&chunk_ref.hash).with_context(|| {
                format!("blob not found in index: {:?}", hex::encode(chunk_ref.hash))
            })?;
            let pack_path = format!("{PACKS_DIR}/{}", location.pack_id);
            let pack_data = self.backend.read(&pack_path).await?;
            let blob =
                &pack_data[location.offset as usize..(location.offset + location.length) as usize];
            result.extend_from_slice(blob);
        }
        Ok(result)
    }

    /// Save a snapshot.
    pub async fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let path = format!("{SNAPSHOTS_DIR}/{}.json", snapshot.id);
        let data = snapshot.to_bytes()?;
        self.backend.write(&path, &data).await?;
        info!(id = %snapshot.short_id(), tenant = %snapshot.tenant, "Snapshot saved");
        Ok(())
    }

    /// List all snapshots.
    pub async fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let paths = self.backend.list(SNAPSHOTS_DIR).await?;
        let mut snapshots = Vec::new();
        for path in paths {
            if path.ends_with(".json") {
                let data = self.backend.read(&path).await?;
                let snap = Snapshot::from_bytes(&data)?;
                snapshots.push(snap);
            }
        }
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(snapshots)
    }

    /// Get a specific snapshot by ID (full or prefix match).
    pub async fn get_snapshot(&self, id: &str) -> Result<Snapshot> {
        let paths = self.backend.list(SNAPSHOTS_DIR).await?;
        for path in paths {
            if path.contains(id) && path.ends_with(".json") {
                let data = self.backend.read(&path).await?;
                return Snapshot::from_bytes(&data);
            }
        }
        anyhow::bail!("snapshot not found: {id}")
    }

    /// Find the latest snapshot for a given tenant/service/user combo.
    pub async fn find_latest_snapshot(
        &self,
        tenant: &str,
        service: crate::snapshot::Service,
        user: Option<&str>,
    ) -> Result<Option<Snapshot>> {
        let snapshots = self.list_snapshots().await?;
        Ok(snapshots
            .into_iter()
            .find(|s| s.tenant == tenant && s.service == service && s.user.as_deref() == user))
    }

    /// Verify repository integrity: check that all indexed blobs exist in packs.
    pub async fn verify(&self) -> Result<VerifyResult> {
        let mut result = VerifyResult::default();
        let pack_paths = self.backend.list(PACKS_DIR).await?;
        let mut found_packs = std::collections::HashSet::new();

        for path in &pack_paths {
            if let Some(name) = path.rsplit('/').next() {
                found_packs.insert(name.to_string());
            }
            result.packs_checked += 1;
        }

        for (hash, location) in &self.index.entries {
            result.blobs_checked += 1;
            if !found_packs.contains(&location.pack_id) {
                result.errors.push(format!(
                    "blob {} references missing pack {}",
                    hex::encode(hash),
                    location.pack_id
                ));
            }
        }

        let snapshots = self.list_snapshots().await?;
        result.snapshots_checked = snapshots.len() as u64;

        Ok(result)
    }

    pub fn blob_count(&self) -> usize {
        self.index.len()
    }

    async fn flush_pack(&mut self, builder: PackBuilder) -> Result<()> {
        let pack = builder.finalize()?;
        let pack_id = pack.id().to_string();
        let pack_path = format!("{PACKS_DIR}/{pack_id}");

        for blob in &pack.header.blobs {
            self.index
                .add(blob.hash, pack_id.clone(), blob.offset, blob.length);
        }

        self.backend.write(&pack_path, &pack.data).await?;
        self.save_index().await?;
        Ok(())
    }

    async fn save_index(&self) -> Result<()> {
        self.backend
            .write(INDEX_PATH, &self.index.to_bytes()?)
            .await
    }
}

#[derive(Debug, Default)]
pub struct VerifyResult {
    pub packs_checked: u64,
    pub blobs_checked: u64,
    pub snapshots_checked: u64,
    pub errors: Vec<String>,
}

impl VerifyResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}
