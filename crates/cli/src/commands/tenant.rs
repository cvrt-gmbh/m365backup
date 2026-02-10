use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::AppConfig;
use crate::config::TenantConfig;

#[derive(Args)]
pub struct TenantArgs {
    #[command(subcommand)]
    action: TenantAction,
}

#[derive(Subcommand)]
enum TenantAction {
    /// Add a new tenant
    Add {
        /// Tenant display name
        #[arg(long)]
        name: String,
        /// Azure AD tenant ID
        #[arg(long)]
        tenant_id: String,
        /// App registration client ID
        #[arg(long)]
        client_id: String,
        /// App registration client secret
        #[arg(long)]
        client_secret: String,
    },
    /// List configured tenants
    List,
    /// Remove a tenant
    Remove {
        /// Tenant name to remove
        name: String,
    },
}

pub async fn run(args: TenantArgs) -> Result<()> {
    match args.action {
        TenantAction::Add {
            name,
            tenant_id,
            client_id,
            client_secret,
        } => {
            let mut config = AppConfig::load()?;
            if config.find_tenant(&name).is_some() {
                anyhow::bail!("tenant '{name}' already exists");
            }
            config.tenants.push(TenantConfig {
                name: name.clone(),
                tenant_id,
                client_id,
                client_secret,
            });
            config.save()?;
            println!("Tenant '{name}' added.");
        }
        TenantAction::List => {
            let config = AppConfig::load()?;
            if config.tenants.is_empty() {
                println!("No tenants configured.");
            } else {
                println!("{:<20} {:<40}", "NAME", "TENANT ID");
                println!("{}", "-".repeat(60));
                for t in &config.tenants {
                    println!("{:<20} {:<40}", t.name, t.tenant_id);
                }
            }
        }
        TenantAction::Remove { name } => {
            let mut config = AppConfig::load()?;
            let before = config.tenants.len();
            config
                .tenants
                .retain(|t| !t.name.eq_ignore_ascii_case(&name));
            if config.tenants.len() == before {
                anyhow::bail!("tenant '{name}' not found");
            }
            config.save()?;
            println!("Tenant '{name}' removed.");
        }
    }
    Ok(())
}
