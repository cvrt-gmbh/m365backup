pub mod backup;
pub mod init;
pub mod restore;
pub mod snapshots;
pub mod tenant;
pub mod verify;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new backup repository
    Init(init::InitArgs),
    /// Manage tenants
    Tenant(tenant::TenantArgs),
    /// Run a backup
    Backup(backup::BackupArgs),
    /// List and inspect snapshots
    Snapshots(snapshots::SnapshotsArgs),
    /// Restore data from a snapshot
    Restore(restore::RestoreArgs),
    /// Verify repository integrity
    Verify(verify::VerifyArgs),
}
