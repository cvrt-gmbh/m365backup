# Contributing to m365backup

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

```bash
git clone https://github.com/cvrt-gmbh/m365backup.git
cd m365backup
cargo build
cargo test
```

Requires Rust 1.85+ (edition 2024).

## Making Changes

1. Fork the repository and create a branch from `main`
2. Make your changes
3. Ensure all checks pass:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```
4. Open a pull request against `main`

## Code Style

- Run `cargo fmt` before committing
- Fix all `cargo clippy` warnings
- Follow existing patterns in the codebase
- Keep PRs focused — one feature or fix per PR

## Commit Messages

Use clear, descriptive commit messages:
- `Add Exchange calendar backup support`
- `Fix delta token expiry handling for large mailboxes`
- `Update dependencies to latest versions`

## Reporting Issues

- Use [GitHub Issues](https://github.com/cvrt-gmbh/m365backup/issues) to report bugs or request features
- Include steps to reproduce for bugs
- Include your Rust version and OS

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 License.
