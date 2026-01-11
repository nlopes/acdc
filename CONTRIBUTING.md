# Contributing to acdc

Thank you for your interest in contributing! This guide covers the essentials. For detailed information, see the links below.

## Quick Start

1. **Fork and clone** the repository
2. **Install Rust** via [rustup](https://rustup.rs/) (the correct version is specified in `rust-toolchain.toml`)
3. **Build and test**:
   ```bash
   cargo build --all
   cargo nextest run
   ```

## Documentation

- [README.adoc](README.adoc) - Project overview, building, testing, and development workflow
- [ARCHITECTURE.adoc](ARCHITECTURE.adoc) - Design decisions and architecture
- [acdc-cli/README.adoc](acdc-cli/README.adoc) - CLI usage and feature flags
- [acdc-parser/README.adoc](acdc-parser/README.adoc) - Parser features and details

## Code Quality

Before submitting, ensure:

- Code is formatted: `cargo fmt --all`
- Lints pass: `cargo clippy --all-targets --all-features`
- Tests pass: `cargo nextest run`

The project uses strict linting (see `Cargo.toml` workspace lints). Key standards:
- No unsafe code
- Exhaustive enum matching
- Document public APIs
- Use `thiserror` for error types

## Commit Guidelines

Use **Conventional Commits**: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, etc.

Examples:
- `feat: add support for table row spanning`
- `fix: correct inline markup parsing in code spans`
- `docs: update README with new CLI options`

## Submitting Changes

1. Create a branch: `git checkout -b feat/your-feature-name`
2. Make your changes (with tests!)
3. Run checks: `cargo fmt --all && cargo clippy --all-targets --all-features && cargo nextest run`
4. Commit using conventional commits
5. Push and open a Pull Request

## Getting Help

- Check existing issues and PRs
- Review the documentation linked above
- Open an issue for questions

Thank you for contributing! 🎉
