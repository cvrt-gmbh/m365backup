use anyhow::Result;
use clap::Args;

use m365backup_core::Repository;

use crate::config::AppConfig;
use crate::progress;

#[derive(Args)]
pub struct VerifyArgs;

pub async fn run(_args: VerifyArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let backend = config.open_backend().await?;
    let repo = Repository::open(backend).await?;

    let spinner = progress::create_spinner("Verifying repository integrity...");
    let result = repo.verify().await?;
    spinner.finish_with_message("done");

    println!("Packs checked:     {}", result.packs_checked);
    println!("Blobs checked:     {}", result.blobs_checked);
    println!("Snapshots checked: {}", result.snapshots_checked);

    if result.is_ok() {
        println!("\nRepository integrity OK.");
    } else {
        println!("\nErrors found ({}):", result.errors.len());
        for err in &result.errors {
            println!("  - {err}");
        }
        anyhow::bail!("repository integrity check failed");
    }

    Ok(())
}
