# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Exchange backup support: mail (EML), calendar events (JSON), contacts (JSON)
- Incremental delta sync for Exchange with automatic full-resync on 410 Gone
- Item-type breakdown in restore summary (mail, calendar, contacts, files)
- `get_json_or_gone` helper for Graph API delta token expiry handling

## [0.1.0] - 2025-05-01

### Added
- Initial release
- OneDrive backup and restore
- Content-addressable storage with BLAKE3 + FastCDC deduplication
- AES-256-GCM encryption with Argon2 key derivation
- Local filesystem and S3-compatible storage backends
- Incremental backups via Microsoft Graph delta queries
- Multi-tenant support
- Repository verification (`m365backup verify`)
- CI pipeline (check, test, clippy, fmt)
- Release pipeline with cross-compilation (Linux x86_64/aarch64, macOS x86_64/aarch64)

[Unreleased]: https://github.com/cvrt-gmbh/m365backup/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cvrt-gmbh/m365backup/releases/tag/v0.1.0
