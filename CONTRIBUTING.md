# Contributing to acdc

Thank you for your interest in contributing! This guide covers the essentials. For detailed information, see the links below.

## Quick Start

1. **Fork and clone** the repository
2. **Install Rust** via [rustup](https://rustup.rs/) (the correct version is specified in `rust-toolchain.toml`)
3. **Install Zig 0.16.0** for the Ghostty-backed terminal converter tests:
   ```bash
   zig version
   ```
   Download version 0.16.0 from [ziglang.org/download](https://ziglang.org/download/)
   or install it with your package manager. Ensure that `zig version` reports
   `0.16.0`; an older `zig@0.15` installation can take precedence on `PATH`.
   The pinned Ghostty revision supports the macOS 27 SDK without selecting an
   older SDK.
4. **Build and test**:
   ```bash
   cargo build --workspace --all-features
   cargo nextest run --all-features
   ```

## Documentation

- [README.adoc](README.adoc) - Project overview, building, testing, and development workflow
- [ARCHITECTURE.adoc](ARCHITECTURE.adoc) - Design decisions and architecture
- [acdc-cli/README.adoc](acdc-cli/README.adoc) - CLI usage and feature flags
- [acdc-lint/README.adoc](acdc-lint/README.adoc) - lint support crate and lint-level model
- [acdc-parser/README.adoc](acdc-parser/README.adoc) - Parser features and details
- [acdc-lsp/README.md](acdc-lsp/README.md) - Language Server setup and supported LSP capabilities
- [acdc-editor-wasm/README.md](acdc-editor-wasm/README.md) - WASM live editor (embedding, API, syntax highlighting classes)
- [converters/README.adoc](converters/README.adoc) - Index of output backends (HTML, manpage, markdown, PDF, terminal)

## Code Quality

Before submitting, ensure:

- Code is formatted: `cargo fmt --all`
- Lints pass: `cargo clippy --all-targets --all-features -- --deny clippy::pedantic`
- Tests pass: `cargo nextest run --all-features`

`--all-features` clippy and converter tests build `libghostty-vt-sys`, which
uses Zig to compile Ghostty's virtual terminal library. Set `GHOSTTY_SOURCE_DIR`
only if you want to reuse an existing local Ghostty checkout.

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
3. Run checks: `cargo fmt --all && cargo clippy --all-targets --all-features -- --deny clippy::pedantic && cargo nextest run --all-features`
4. Commit using conventional commits
5. Push and open a Pull Request

## Getting Help

- Check existing issues and PRs
- Review the documentation linked above
- Open an issue for questions

Thank you for contributing! 🎉
