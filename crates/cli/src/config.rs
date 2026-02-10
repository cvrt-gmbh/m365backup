use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use m365backup_core::backend::Backend;
use m365backup_core::backend::local::LocalBackend;
use m365backup_core::backend::s3::S3Backend;

const CONFIG_FILE: &str = "m365backup.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub repository: RepoConfig,
    #[serde(default)]
    pub tenants: Vec<TenantConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub backend: BackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BackendConfig {
    #[serde(rename = "local")]
    Local { path: String },
    #[serde(rename = "s3")]
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        prefix: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub name: String,
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("m365backup")
            .join(CONFIG_FILE)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("config not found at {}", path.display()))?;
        toml::from_str(&content).context("failed to parse config")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write config to {}", path.display()))?;
        Ok(())
    }

    pub fn find_tenant(&self, name: &str) -> Option<&TenantConfig> {
        self.tenants
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    pub async fn open_backend(&self) -> Result<Arc<dyn Backend>> {
        match &self.repository.backend {
            BackendConfig::Local { path } => Ok(Arc::new(LocalBackend::new(path)?)),
            BackendConfig::S3 {
                endpoint,
                region,
                bucket,
                access_key,
                secret_key,
                prefix,
            } => {
                let backend = S3Backend::new(
                    bucket,
                    endpoint,
                    region,
                    access_key,
                    secret_key,
                    prefix.as_deref(),
                )
                .await?;
                Ok(Arc::new(backend))
            }
        }
    }
}
