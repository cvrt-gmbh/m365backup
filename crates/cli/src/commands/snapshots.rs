use anyhow::Result;
use clap::Args;

use m365backup_core::Repository;

use crate::config::AppConfig;

#[derive(Args)]
pub struct SnapshotsArgs {
    /// Filter by tenant name
    #[arg(long)]
    tenant: Option<String>,

    /// Show details for a specific snapshot ID
    #[arg(long)]
    inspect: Option<String>,
}

pub async fn run(args: SnapshotsArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let backend = config.open_backend().await?;
    let repo = Repository::open(backend).await?;

    if let Some(id) = args.inspect {
        let snap = repo.get_snapshot(&id).await?;
        println!("Snapshot:  {}", snap.id);
        println!("Tenant:    {}", snap.tenant);
        println!("Service:   {}", snap.service);
        println!("User:      {}", snap.user.as_deref().unwrap_or("-"));
        println!(
            "Timestamp: {}",
            snap.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        );
        if let Some(ref parent) = snap.parent {
            println!("Parent:    {}", &parent[..8]);
        }
        println!();
        println!("Stats:");
        println!("  Total items:  {}", snap.stats.total_items);
        println!("  New items:    {}", snap.stats.new_items);
        println!("  Unchanged:    {}", snap.stats.unchanged_items);
        println!("  Total bytes:  {}", format_bytes(snap.stats.total_bytes));
        println!("  New bytes:    {}", format_bytes(snap.stats.new_bytes));
        println!(
            "  Dedup saved:  {}",
            format_bytes(snap.stats.deduplicated_bytes)
        );
        println!("  Duration:     {:.1}s", snap.stats.duration_secs);
        println!();
        println!("Files ({}):", snap.tree.nodes.len());
        for node in snap.tree.nodes.iter().take(50) {
            println!("  {} ({})", node.path, format_bytes(node.size));
        }
        if snap.tree.nodes.len() > 50 {
            println!("  ... and {} more", snap.tree.nodes.len() - 50);
        }
    } else {
        let snapshots = repo.list_snapshots().await?;
        let snapshots: Vec<_> = if let Some(ref tenant) = args.tenant {
            snapshots
                .into_iter()
                .filter(|s| s.tenant.eq_ignore_ascii_case(tenant))
                .collect()
        } else {
            snapshots
        };

        if snapshots.is_empty() {
            println!("No snapshots found.");
            return Ok(());
        }

        println!(
            "{:<10} {:<20} {:<12} {:<30} {:<8} {:<10}",
            "ID", "TENANT", "SERVICE", "USER", "FILES", "SIZE"
        );
        println!("{}", "-".repeat(90));
        for snap in &snapshots {
            println!(
                "{:<10} {:<20} {:<12} {:<30} {:<8} {:<10}",
                snap.short_id(),
                snap.tenant,
                snap.service.to_string(),
                snap.user.as_deref().unwrap_or("-"),
                snap.stats.total_items,
                format_bytes(snap.stats.total_bytes),
            );
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
