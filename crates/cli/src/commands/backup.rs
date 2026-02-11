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
use m365backup_graph::exchange::ExchangeClient;
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
        Service::Exchange => {
            backup_exchange(&graph, &mut repo, &tenant.name, args.user.as_deref()).await
        }
        other => {
            anyhow::bail!("{other} backup not yet implemented")
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

// ---------------------------------------------------------------------------
// Exchange backup
// ---------------------------------------------------------------------------

async fn backup_exchange(
    graph: &GraphClient,
    repo: &mut Repository,
    tenant_name: &str,
    user_filter: Option<&str>,
) -> Result<()> {
    let od = OneDriveClient::new(graph);
    let ex = ExchangeClient::new(graph);
    let start = Instant::now();

    // Get users (reuse OneDriveClient's user enumeration)
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
        info!(user = %user.user_principal_name, "Backing up Exchange");

        let prev = repo
            .find_latest_snapshot(
                tenant_name,
                Service::Exchange,
                Some(&user.user_principal_name),
            )
            .await?;

        let mut nodes = Vec::new();
        let mut delta_tokens = HashMap::new();
        let mut stats = BackupStats::default();

        // 1. Mail
        backup_mail(
            &ex,
            repo,
            &user.id,
            prev.as_ref(),
            &mut nodes,
            &mut delta_tokens,
            &mut stats,
        )
        .await?;

        // 2. Calendar
        backup_calendar(
            &ex,
            repo,
            &user.id,
            prev.as_ref(),
            &mut nodes,
            &mut delta_tokens,
            &mut stats,
        )
        .await?;

        // 3. Contacts
        backup_contacts(
            &ex,
            repo,
            &user.id,
            prev.as_ref(),
            &mut nodes,
            &mut delta_tokens,
            &mut stats,
        )
        .await?;

        stats.duration_secs = start.elapsed().as_secs_f64();

        let mut snapshot = Snapshot::new(
            tenant_name.to_string(),
            Service::Exchange,
            Some(user.user_principal_name.clone()),
        );
        snapshot.parent = prev.map(|s| s.id);
        snapshot.tree = Tree { nodes };
        snapshot.delta_tokens = delta_tokens;
        snapshot.stats = stats;

        repo.save_snapshot(&snapshot).await?;

        let mail_count = snapshot
            .tree
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Mail)
            .count();
        let cal_count = snapshot
            .tree
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Calendar)
            .count();
        let contact_count = snapshot
            .tree
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Contact)
            .count();
        println!(
            "  Snapshot {} saved ({} mail, {} calendar, {} contacts)",
            snapshot.short_id(),
            mail_count,
            cal_count,
            contact_count,
        );
    }

    println!("Exchange backup complete.");
    Ok(())
}

async fn backup_mail(
    ex: &ExchangeClient<'_>,
    repo: &mut Repository,
    user_id: &str,
    prev: Option<&Snapshot>,
    nodes: &mut Vec<TreeNode>,
    delta_tokens: &mut HashMap<String, String>,
    stats: &mut BackupStats,
) -> Result<()> {
    let spinner = progress::create_spinner("Listing mail folders...");
    let folders = ex.list_all_mail_folders(user_id).await?;
    spinner.finish_with_message(format!("Found {} mail folders", folders.len()));

    for folder in &folders {
        let delta_key = DeltaState::key("exchange", user_id, &format!("mail:{}", folder.id));
        let prev_token = prev
            .and_then(|s| s.delta_tokens.get(&delta_key))
            .map(|s| s.as_str());

        let result = ex
            .get_mail_folder_delta(user_id, &folder.id, prev_token)
            .await?;

        // On 410 Gone, retry without token (full resync)
        let (messages, new_delta) = match result {
            Some(r) => r,
            None => {
                info!(folder = %folder.display_name, "Delta expired, running full resync");
                ex.get_mail_folder_delta(user_id, &folder.id, None)
                    .await?
                    .expect("full resync should not return 410")
            }
        };

        if let Some(token) = new_delta {
            delta_tokens.insert(delta_key, token);
        }

        // Filter out deleted items
        let active: Vec<_> = messages.iter().filter(|m| m.removed.is_none()).collect();
        if active.is_empty() {
            continue;
        }

        let pb = progress::create_progress(
            active.len() as u64,
            &format!("mail/{}", folder.display_name),
        );

        for msg in &active {
            let subject = msg.subject.as_deref().unwrap_or("(no subject)");
            let id_prefix = &msg.id[..msg.id.len().min(8)];
            let filename = format!("{}_{}.eml", sanitize_filename(subject), id_prefix);
            let path = format!(
                "mail/{}/{}",
                sanitize_filename(&folder.display_name),
                filename
            );

            match ex.download_mime(user_id, &msg.id).await {
                Ok(data) => {
                    let size = data.len() as u64;
                    stats.total_items += 1;
                    stats.total_bytes += size;
                    stats.new_items += 1;
                    stats.new_bytes += size;

                    let chunk_refs = repo.store_data(&data).await?;

                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "graph_id".to_string(),
                        serde_json::Value::String(msg.id.clone()),
                    );
                    if let Some(ref dt) = msg.received_date_time {
                        metadata.insert(
                            "received".to_string(),
                            serde_json::Value::String(dt.clone()),
                        );
                    }

                    nodes.push(TreeNode {
                        path,
                        node_type: NodeType::Mail,
                        size,
                        modified: msg
                            .received_date_time
                            .as_ref()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc)),
                        chunks: chunk_refs,
                        metadata,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        message_id = %msg.id,
                        subject = %subject,
                        error = %e,
                        "Failed to download message MIME, skipping"
                    );
                }
            }

            pb.inc(1);
        }
        pb.finish_with_message("done");
    }

    Ok(())
}

async fn backup_calendar(
    ex: &ExchangeClient<'_>,
    repo: &mut Repository,
    user_id: &str,
    prev: Option<&Snapshot>,
    nodes: &mut Vec<TreeNode>,
    delta_tokens: &mut HashMap<String, String>,
    stats: &mut BackupStats,
) -> Result<()> {
    let delta_key = DeltaState::key("exchange", user_id, "calendar");
    let prev_token = prev
        .and_then(|s| s.delta_tokens.get(&delta_key))
        .map(|s| s.as_str());

    let spinner = progress::create_spinner("Fetching calendar events...");
    let result = ex.get_calendar_delta(user_id, prev_token).await?;

    let (events, new_delta) = match result {
        Some(r) => r,
        None => {
            info!("Calendar delta expired, running full resync");
            ex.get_calendar_delta(user_id, None)
                .await?
                .expect("full resync should not return 410")
        }
    };
    spinner.finish_with_message(format!("Found {} calendar events", events.len()));

    if let Some(token) = new_delta {
        delta_tokens.insert(delta_key, token);
    }

    let active: Vec<_> = events.iter().filter(|e| e.removed.is_none()).collect();
    if active.is_empty() {
        return Ok(());
    }

    let pb = progress::create_progress(active.len() as u64, "calendar events");

    for event in &active {
        let subject = event.subject.as_deref().unwrap_or("(no subject)");
        let id_prefix = &event.id[..event.id.len().min(8)];
        let filename = format!("{}_{}.json", sanitize_filename(subject), id_prefix);
        let path = format!("calendar/{}", filename);

        match ex.get_event(user_id, &event.id).await {
            Ok(data) => {
                let size = data.len() as u64;
                stats.total_items += 1;
                stats.total_bytes += size;
                stats.new_items += 1;
                stats.new_bytes += size;

                let chunk_refs = repo.store_data(&data).await?;

                let mut metadata = HashMap::new();
                metadata.insert(
                    "graph_id".to_string(),
                    serde_json::Value::String(event.id.clone()),
                );

                nodes.push(TreeNode {
                    path,
                    node_type: NodeType::Calendar,
                    size,
                    modified: event
                        .start
                        .as_ref()
                        .and_then(|s| s.date_time.as_ref())
                        .and_then(|s| {
                            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
                                .ok()
                                .or_else(|| {
                                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                                        .ok()
                                })
                        })
                        .map(|ndt| ndt.and_utc()),
                    chunks: chunk_refs,
                    metadata,
                });
            }
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    subject = %subject,
                    error = %e,
                    "Failed to fetch event, skipping"
                );
            }
        }

        pb.inc(1);
    }
    pb.finish_with_message("done");

    Ok(())
}

async fn backup_contacts(
    ex: &ExchangeClient<'_>,
    repo: &mut Repository,
    user_id: &str,
    prev: Option<&Snapshot>,
    nodes: &mut Vec<TreeNode>,
    delta_tokens: &mut HashMap<String, String>,
    stats: &mut BackupStats,
) -> Result<()> {
    let spinner = progress::create_spinner("Listing contact folders...");
    let folders = ex.list_contact_folders(user_id).await?;
    spinner.finish_with_message(format!("Found {} contact folders", folders.len()));

    for folder in &folders {
        let delta_key = DeltaState::key("exchange", user_id, &format!("contacts:{}", folder.id));
        let prev_token = prev
            .and_then(|s| s.delta_tokens.get(&delta_key))
            .map(|s| s.as_str());

        let result = ex
            .get_contacts_delta(user_id, &folder.id, prev_token)
            .await?;

        let (contacts, new_delta) = match result {
            Some(r) => r,
            None => {
                info!(folder = %folder.display_name, "Contacts delta expired, running full resync");
                ex.get_contacts_delta(user_id, &folder.id, None)
                    .await?
                    .expect("full resync should not return 410")
            }
        };

        if let Some(token) = new_delta {
            delta_tokens.insert(delta_key, token);
        }

        let active: Vec<_> = contacts.iter().filter(|c| c.removed.is_none()).collect();
        if active.is_empty() {
            continue;
        }

        let pb = progress::create_progress(
            active.len() as u64,
            &format!("contacts/{}", folder.display_name),
        );

        for contact in &active {
            let name = contact.display_name.as_deref().unwrap_or("(unnamed)");
            let id_prefix = &contact.id[..contact.id.len().min(8)];
            let filename = format!("{}_{}.json", sanitize_filename(name), id_prefix);
            let path = format!(
                "contacts/{}/{}",
                sanitize_filename(&folder.display_name),
                filename
            );

            match ex.get_contact(user_id, &contact.id).await {
                Ok(data) => {
                    let size = data.len() as u64;
                    stats.total_items += 1;
                    stats.total_bytes += size;
                    stats.new_items += 1;
                    stats.new_bytes += size;

                    let chunk_refs = repo.store_data(&data).await?;

                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "graph_id".to_string(),
                        serde_json::Value::String(contact.id.clone()),
                    );

                    nodes.push(TreeNode {
                        path,
                        node_type: NodeType::Contact,
                        size,
                        modified: None,
                        chunks: chunk_refs,
                        metadata,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        contact_id = %contact.id,
                        name = %name,
                        error = %e,
                        "Failed to fetch contact, skipping"
                    );
                }
            }

            pb.inc(1);
        }
        pb.finish_with_message("done");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sanitize a string for use as a filesystem path component.
/// Strips characters that are unsafe on Windows/macOS/Linux, truncates to 100 chars.
fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Trim leading/trailing dots and spaces (Windows compat)
    let trimmed = sanitized.trim_matches(|c: char| c == '.' || c == ' ');

    // Truncate to 100 chars (preserving unicode boundaries)
    if trimmed.len() > 100 {
        let mut end = 100;
        while !trimmed.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        trimmed[..end].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_basic() {
        assert_eq!(sanitize_filename("Hello World"), "Hello World");
    }

    #[test]
    fn sanitize_strips_unsafe_chars() {
        assert_eq!(sanitize_filename("Re: Hello/World"), "Re_ Hello_World");
    }

    #[test]
    fn sanitize_strips_control_chars() {
        assert_eq!(sanitize_filename("Hello\x00World"), "Hello_World");
    }

    #[test]
    fn sanitize_trims_dots_and_spaces() {
        assert_eq!(sanitize_filename("...test..."), "test");
        assert_eq!(sanitize_filename("  test  "), "test");
    }

    #[test]
    fn sanitize_truncates_long_names() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_filename(&long).len(), 100);
    }

    #[test]
    fn sanitize_preserves_unicode() {
        assert_eq!(sanitize_filename("Ünïcödé Tëst"), "Ünïcödé Tëst");
    }
}
