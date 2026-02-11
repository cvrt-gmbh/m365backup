# m365backup

[![CI](https://img.shields.io/github/actions/workflow/status/cvrt-gmbh/m365backup/ci.yml?branch=main&label=CI)](https://github.com/cvrt-gmbh/m365backup/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/cvrt-gmbh/m365backup)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)](https://www.rust-lang.org/)

Open-source Microsoft 365 backup. Your data, your storage, your control.

## Why m365backup?

Microsoft 365 has limited built-in retention and recovery. If an employee deletes their mailbox, a ransomware attack encrypts your OneDrive, or you need to meet compliance retention requirements — you need real backups on storage you control.

m365backup gives you:
- **Full incremental backups** of OneDrive, Exchange mail, calendar, and contacts
- **Your choice of storage** — local filesystem or any S3-compatible provider
- **Content-addressable deduplication** — same data is stored only once across all users
- **Encryption at rest** — AES-256-GCM with Argon2 key derivation
- **Multi-tenant support** — a single tool for all your customer tenants (MSP-ready)

## Features

| Service | What's backed up | Status |
|---------|-----------------|--------|
| **OneDrive** | All files and folders | Available |
| **Exchange** | Mail (EML), calendar events, contacts | Available |
| **SharePoint** | Sites and document libraries | Planned |
| **Teams** | Channel and chat messages | Planned |

Core capabilities:
- Incremental backups via Microsoft Graph delta queries
- Content-addressable storage with deduplication (BLAKE3 + FastCDC)
- Local filesystem and S3-compatible storage backends
- AES-256-GCM encryption with Argon2 key derivation
- Multi-tenant support (MSP-friendly)

## Quick Start

### Install

Download a pre-built binary from [Releases](https://github.com/cvrt-gmbh/m365backup/releases), or build from source:

```bash
git clone https://github.com/cvrt-gmbh/m365backup.git
cd m365backup
cargo build --release
# Binary at target/release/m365backup
```

Requires Rust 1.85+.

### Setup

```bash
# Initialize a local backup repository
m365backup init --backend local --path /path/to/backups

# Add a Microsoft 365 tenant
m365backup tenant add \
  --name "My Company" \
  --tenant-id YOUR_TENANT_ID \
  --client-id YOUR_CLIENT_ID \
  --client-secret YOUR_CLIENT_SECRET
```

### Backup

```bash
# Backup OneDrive for all users
m365backup backup --tenant "My Company" --service onedrive

# Backup Exchange (mail, calendar, contacts)
m365backup backup --tenant "My Company" --service exchange

# Backup a specific user only
m365backup backup --tenant "My Company" --service exchange --user user@company.com
```

### Restore

```bash
# List snapshots
m365backup snapshots

# Restore a snapshot to a local directory
m365backup restore --snapshot SNAPSHOT_ID --target ./restore/

# Verify repository integrity
m365backup verify
```

## Azure App Registration

Before using m365backup, register an application in Microsoft Entra ID (Azure AD):

1. Go to [Azure Portal](https://portal.azure.com) > Microsoft Entra ID > App registrations
2. Click **New registration**
3. Set "Supported account types" to **Accounts in any organizational directory** (for multi-tenant)
4. Under API permissions, add the following **Application** permissions for Microsoft Graph:
   - `User.Read.All`
   - `Files.Read.All`
   - `Mail.Read`
   - `Calendars.Read`
   - `Contacts.Read`
   - `Sites.Read.All` *(for future SharePoint support)*
5. Click **Grant admin consent**
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
├── graph/   # Microsoft Graph API client (auth, OneDrive, Exchange)
└── cli/     # CLI binary (clap-based commands)
```

The storage engine uses a restic-inspired content-addressable design:
- Files are split into variable-size chunks using FastCDC
- Each chunk is identified by its BLAKE3 hash (automatic deduplication)
- Chunks are packed into larger pack files for efficient storage
- Snapshots store point-in-time metadata referencing chunk trees
- Incremental backups use Microsoft Graph delta queries — only changed items are fetched

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Security

To report a security vulnerability, please see [SECURITY.md](SECURITY.md).

## License

Apache-2.0 — see [LICENSE](LICENSE) for details.
