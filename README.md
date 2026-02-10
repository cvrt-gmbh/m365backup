# m365backup

Open-source Microsoft 365 backup. Your data, your storage, your control.

## Features

- **OneDrive** backup and restore (Phase 1 — available now)
- **Exchange** mailbox, calendar, contacts (Phase 2 — planned)
- **SharePoint** sites and document libraries (Phase 2 — planned)
- **Teams** channel and chat messages (Phase 3 — planned)
- Content-addressable storage with deduplication (BLAKE3 + FastCDC)
- Incremental backups via Microsoft Graph delta queries
- Local filesystem and S3-compatible storage backends
- AES-256-GCM encryption with Argon2 key derivation
- Multi-tenant support (MSP-friendly)

## Quick Start

```bash
# Initialize a local backup repository
m365backup init --backend local --path /path/to/backups

# Add a Microsoft 365 tenant
m365backup tenant add \
  --name "My Company" \
  --tenant-id YOUR_TENANT_ID \
  --client-id YOUR_CLIENT_ID \
  --client-secret YOUR_CLIENT_SECRET

# Backup all OneDrive data
m365backup backup --tenant "My Company" --service onedrive

# Backup a specific user
m365backup backup --tenant "My Company" --service onedrive --user user@company.com

# List snapshots
m365backup snapshots

# Restore to a local directory
m365backup restore --snapshot SNAPSHOT_ID --target ./restore/

# Verify repository integrity
m365backup verify
```

## Azure App Registration

Before using m365backup, you need to register an application in Azure AD:

1. Go to [Azure Portal](https://portal.azure.com) > Azure Active Directory > App registrations
2. Click "New registration"
3. Set "Supported account types" to **Accounts in any organizational directory** (for multi-tenant)
4. Under API permissions, add the following **Application** permissions for Microsoft Graph:
   - `User.Read.All`
   - `Files.Read.All`
   - `Mail.Read`
   - `Calendars.Read`
   - `Contacts.Read`
   - `Sites.Read.All`
5. Click "Grant admin consent"
6. Under Certificates & secrets, create a new client secret

## S3 Backend

```bash
m365backup init \
  --backend s3 \
  --endpoint https://fsn1.your-objectstorage.com \
  --bucket m365-backups \
  --access-key YOUR_KEY \
  --secret-key YOUR_SECRET
```

Compatible with any S3 provider: Hetzner Object Storage, MinIO, AWS S3, Wasabi, Backblaze B2.

## Architecture

```
crates/
├── core/    # Storage engine (chunking, packing, encryption, backends)
├── graph/   # Microsoft Graph API client (auth, OneDrive, Exchange, etc.)
└── cli/     # CLI binary (clap-based commands)
```

The storage engine uses a restic-inspired content-addressable design:
- Files are split into variable-size chunks using FastCDC
- Each chunk is identified by its BLAKE3 hash (automatic deduplication)
- Chunks are packed into larger pack files for efficient storage
- Snapshots store point-in-time metadata referencing chunk trees

## Building from Source

```bash
git clone https://github.com/cvrt-gmbh/m365backup.git
cd m365backup
cargo build --release
```

Binary will be at `target/release/m365backup`.

## License

Apache-2.0
