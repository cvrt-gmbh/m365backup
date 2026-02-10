mod commands;
mod config;
mod progress;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "m365backup",
    version,
    about = "Open-source Microsoft 365 backup"
)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match cli.command {
        commands::Command::Init(args) => commands::init::run(args).await,
        commands::Command::Tenant(args) => commands::tenant::run(args).await,
        commands::Command::Backup(args) => commands::backup::run(args).await,
        commands::Command::Snapshots(args) => commands::snapshots::run(args).await,
        commands::Command::Restore(args) => commands::restore::run(args).await,
        commands::Command::Verify(args) => commands::verify::run(args).await,
    }
}
