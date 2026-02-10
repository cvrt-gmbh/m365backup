use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use tracing::info;

use m365backup_core::Repository;
use m365backup_core::backend::local::LocalBackend;
use m365backup_core::backend::s3::S3Backend;

use crate::config::{AppConfig, BackendConfig, RepoConfig};

#[derive(Args)]
pub struct InitArgs {
    /// Backend type: local or s3
    #[arg(long)]
    backend: String,

    /// Path for local backend
    #[arg(long)]
    path: Option<String>,

    /// S3 endpoint URL
    #[arg(long)]
    endpoint: Option<String>,

    /// S3 bucket name
    #[arg(long)]
    bucket: Option<String>,

    /// S3 region
    #[arg(long, default_value = "auto")]
    region: String,

    /// S3 access key
    #[arg(long)]
    access_key: Option<String>,

    /// S3 secret key
    #[arg(long)]
    secret_key: Option<String>,

    /// S3 path prefix
    #[arg(long)]
    prefix: Option<String>,
}

pub async fn run(args: InitArgs) -> Result<()> {
    let backend_config = match args.backend.as_str() {
        "local" => {
            let path = args
                .path
                .ok_or_else(|| anyhow::anyhow!("--path required for local backend"))?;
            BackendConfig::Local { path }
        }
        "s3" => {
            let endpoint = args
                .endpoint
                .ok_or_else(|| anyhow::anyhow!("--endpoint required for S3 backend"))?;
            let bucket = args
                .bucket
                .ok_or_else(|| anyhow::anyhow!("--bucket required for S3 backend"))?;
            let access_key = args
                .access_key
                .ok_or_else(|| anyhow::anyhow!("--access-key required for S3 backend"))?;
            let secret_key = args
                .secret_key
                .ok_or_else(|| anyhow::anyhow!("--secret-key required for S3 backend"))?;
            BackendConfig::S3 {
                endpoint,
                region: args.region,
                bucket,
                access_key,
                secret_key,
                prefix: args.prefix,
            }
        }
        other => anyhow::bail!("unknown backend: {other} (supported: local, s3)"),
    };

    let config = AppConfig {
        repository: RepoConfig {
            backend: backend_config.clone(),
        },
        tenants: Vec::new(),
    };

    // Create the backend and initialize the repo
    let backend: Arc<dyn m365backup_core::backend::Backend> = match &backend_config {
        BackendConfig::Local { path } => Arc::new(LocalBackend::init(path)?),
        BackendConfig::S3 {
            endpoint,
            region,
            bucket,
            access_key,
            secret_key,
            prefix,
        } => Arc::new(
            S3Backend::new(
                bucket,
                endpoint,
                region,
                access_key,
                secret_key,
                prefix.as_deref(),
            )
            .await?,
        ),
    };

    Repository::init(backend, &args.backend).await?;
    config.save()?;

    info!(
        config_path = %AppConfig::config_path().display(),
        "Repository initialized. Config saved."
    );
    println!("Repository initialized successfully.");
    println!("Config: {}", AppConfig::config_path().display());
    Ok(())
}
