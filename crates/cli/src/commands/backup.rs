use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use clap::Args;
use tracing::info;

use m365backup_core::Repository;
use m365backup_core::snapshot::{BackupStats, NodeType, Service, Snapshot, Tree, TreeNode};
use m365backup_graph::auth::{AuthProvider, ClientCredentials};
use m365backup_graph::client::GraphClient;
use m365backup_graph::delta::DeltaState;
use m365backup_graph::onedrive::OneDriveClient;

use crate::config::AppConfig;
use crate::progress;

#[derive(Args)]
pub struct BackupArgs {
    /// Tenant name
    #[arg(long)]
    tenant: String,

    /// Service to backup (onedrive, exchange, sharepoint, teams)
    #[arg(long)]
    service: Option<String>,

    /// Specific user to backup (default: all users)
    #[arg(long)]
    user: Option<String>,
}

pub async fn run(args: BackupArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let tenant = config
        .find_tenant(&args.tenant)
        .ok_or_else(|| anyhow::anyhow!("tenant '{}' not found", args.tenant))?
        .clone();

    let service: Service = args.service.as_deref().unwrap_or("onedrive").parse()?;

    let auth = AuthProvider::new(ClientCredentials {
        client_id: tenant.client_id.clone(),
        client_secret: tenant.client_secret.clone(),
        tenant_id: tenant.tenant_id.clone(),
    });
    let graph = GraphClient::new(auth);

    let backend = config.open_backend().await?;
    let mut repo = Repository::open(backend).await?;

    match service {
        Service::OneDrive => {
            backup_onedrive(&graph, &mut repo, &tenant.name, args.user.as_deref()).await
        }
        other => {
            anyhow::bail!("{other} backup not yet implemented (coming in Phase 2)")
        }
    }
}

async fn backup_onedrive(
    graph: &GraphClient,
    repo: &mut Repository,
    tenant_name: &str,
    user_filter: Option<&str>,
) -> Result<()> {
    let od = OneDriveClient::new(graph);
    let start = Instant::now();

    // Get users
    let spinner = progress::create_spinner("Enumerating users...");
    let all_users = od.list_users().await?;
    spinner.finish_with_message(format!("Found {} users", all_users.len()));

    let users: Vec<_> = if let Some(filter) = user_filter {
        all_users
            .into_iter()
            .filter(|u| {
                u.user_principal_name.eq_ignore_ascii_case(filter)
                    || u.mail
                        .as_deref()
                        .is_some_and(|m| m.eq_ignore_ascii_case(filter))
            })
            .collect()
    } else {
        all_users
    };

    if users.is_empty() {
        anyhow::bail!("no matching users found");
    }

    for user in &users {
        info!(user = %user.user_principal_name, "Backing up OneDrive");

        // Check for previous snapshot to get delta token
        let prev = repo
            .find_latest_snapshot(
                tenant_name,
                Service::OneDrive,
                Some(&user.user_principal_name),
            )
            .await?;
        let delta_key = DeltaState::key("onedrive", &user.id, "drive");
        let prev_token = prev
            .as_ref()
            .and_then(|s| s.delta_tokens.get(&delta_key))
            .map(|s| s.as_str());

        if prev_token.is_some() {
            info!("Using incremental delta sync");
        } else {
            info!("Running full sync (no previous snapshot)");
        }

        // Delta query
        let spinner = progress::create_spinner(&format!(
            "Fetching drive items for {}...",
            user.user_principal_name
        ));
        let (items, new_delta_token) = od.get_drive_delta(&user.id, prev_token).await?;
        spinner.finish_with_message(format!("Found {} items", items.len()));

        // Filter to files only
        let files: Vec<_> = items.iter().filter(|i| i.file.is_some()).collect();
        if files.is_empty() {
            println!("  No files to backup for {}", user.user_principal_name);
            continue;
        }

        let pb = progress::create_progress(files.len() as u64, "files");
        let mut nodes = Vec::new();
        let mut stats = BackupStats::default();

        for item in &files {
            let path = OneDriveClient::item_path(item);
            let size = item.size.unwrap_or(0);
            stats.total_items += 1;
            stats.total_bytes += size;

            // Download file content
            let data = if let Some(ref url) = item.download_url {
                od.download_url(url).await?
            } else {
                od.download_file(&user.id, &item.id).await?
            };

            // Store with dedup
            let chunk_refs = repo.store_data(&data).await?;
            let is_new = chunk_refs.iter().any(|_| true); // simplified
            if is_new {
                stats.new_items += 1;
                stats.new_bytes += size;
            } else {
                stats.unchanged_items += 1;
                stats.deduplicated_bytes += size;
            }

            let mut metadata = HashMap::new();
            metadata.insert(
                "graph_id".to_string(),
                serde_json::Value::String(item.id.clone()),
            );
            if let Some(ref modified) = item.last_modified {
                metadata.insert(
                    "last_modified".to_string(),
                    serde_json::Value::String(modified.clone()),
                );
            }

            nodes.push(TreeNode {
                path,
                node_type: NodeType::File,
                size,
                modified: item
                    .last_modified
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                chunks: chunk_refs,
                metadata,
            });

            pb.inc(1);
        }
        pb.finish_with_message("done");

        stats.duration_secs = start.elapsed().as_secs_f64();

        // Create snapshot
        let mut snapshot = Snapshot::new(
            tenant_name.to_string(),
            Service::OneDrive,
            Some(user.user_principal_name.clone()),
        );
        snapshot.parent = prev.map(|s| s.id);
        snapshot.tree = Tree { nodes };
        snapshot.stats = stats;
        if let Some(token) = new_delta_token {
            snapshot.delta_tokens.insert(delta_key, token);
        }

        repo.save_snapshot(&snapshot).await?;
        println!(
            "  Snapshot {} saved ({} files, {} new)",
            snapshot.short_id(),
            snapshot.stats.total_items,
            snapshot.stats.new_items,
        );
    }

    println!("Backup complete.");
    Ok(())
}
