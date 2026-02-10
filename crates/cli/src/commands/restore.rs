use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use tracing::info;

use m365backup_core::Repository;

use crate::config::AppConfig;
use crate::progress;

#[derive(Args)]
pub struct RestoreArgs {
    /// Snapshot ID (or prefix)
    #[arg(long)]
    snapshot: String,

    /// Service to restore (onedrive, exchange)
    #[arg(long)]
    service: Option<String>,

    /// User to restore
    #[arg(long)]
    user: Option<String>,

    /// Target directory for restored files
    #[arg(long)]
    target: String,
}

pub async fn run(args: RestoreArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let backend = config.open_backend().await?;
    let repo = Repository::open(backend).await?;

    let snap = repo.get_snapshot(&args.snapshot).await?;
    info!(
        snapshot = %snap.short_id(),
        tenant = %snap.tenant,
        service = %snap.service,
        "Restoring from snapshot"
    );

    let target = PathBuf::from(&args.target);
    tokio::fs::create_dir_all(&target).await?;

    // Filter nodes by user if specified
    let nodes: Vec<_> = snap
        .tree
        .nodes
        .iter()
        .filter(|_n| {
            if let Some(ref user) = args.user {
                // For OneDrive, all nodes belong to the snapshot's user
                snap.user
                    .as_ref()
                    .is_some_and(|u| u.eq_ignore_ascii_case(user))
            } else {
                true
            }
        })
        .collect();

    if nodes.is_empty() {
        println!("No items to restore.");
        return Ok(());
    }

    let pb = progress::create_progress(nodes.len() as u64, "files");
    let mut restored = 0u64;

    for node in &nodes {
        let file_path = target.join(&node.path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let data = repo.read_data(&node.chunks).await?;
        tokio::fs::write(&file_path, &data).await?;
        restored += 1;
        pb.inc(1);
    }

    pb.finish_with_message("done");
    println!("Restored {restored} files to {}", target.display());
    Ok(())
}
