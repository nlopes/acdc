# Contributing to acdc

Thank you for your interest in contributing to acdc! This document provides guidelines and instructions for getting started.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Testing](#testing)
- [Code Style](#code-style)
- [Commit Guidelines](#commit-guidelines)
- [Project Structure](#project-structure)
- [Debugging](#debugging)
- [Submitting Changes](#submitting-changes)

## Prerequisites

Before you begin, ensure you have:

- **Rust toolchain**: The project uses a specific Rust version defined in `rust-toolchain.toml`. Install Rust using [rustup](https://rustup.rs/), and the correct version will be automatically used.
- **Git**: For version control
- **Basic familiarity with Rust**: This is a Rust project using Cargo

The required Rust components (rust-analyzer, clippy, rustfmt) are specified in `rust-toolchain.toml` and will be installed automatically.

## Getting Started

1. **Fork and clone the repository**:
   ```bash
   git clone https://github.com/your-username/acdc.git
   cd acdc
   ```

2. **Verify your setup**:
   ```bash
   # Check Rust version
   rustc --version
   
   # Build all crates
   cargo build --all
   
   # Run tests to ensure everything works
   cargo nextest run
   ```

3. **Familiarize yourself with the project**:
   - Read the [README.adoc](README.adoc) for an overview
   - Check [ARCHITECTURE.adoc](ARCHITECTURE.adoc) for design decisions
   - Review the project structure below

## Development Workflow

### Building the Project

```bash
# Build all crates
cargo build --all

# Build with all features
cargo build --all-features

# Build a specific crate
cargo build -p acdc-cli
```

### Running the CLI

```bash
# Parse and convert AsciiDoc to HTML
cargo run --bin acdc -- convert document.adoc

# See all available options
cargo run --bin acdc -- --help
```

## Testing

The project uses [nextest](https://nexte.st/) for running tests. Install it if needed:

```bash
cargo install cargo-nextest
```

### Running Tests

```bash
# Run all tests
cargo nextest run

# Run tests with detailed output (no fail-fast)
RUST_LOG=error cargo nextest run --no-fail-fast

# Run tests for a specific crate
cargo nextest run -p acdc-parser

# Run property-based tests
PROPTEST_CASES=1000 cargo test --package acdc-parser --lib proptests
```

### Test Types

1. **Fixture tests**: Compare against known good outputs in `acdc-parser/fixtures/`
2. **Property tests**: Verify invariants hold for any input (proptest)
3. **TCK tests**: Check specification compliance
4. **Integration tests**: End-to-end conversion testing

### Adding New Tests

- Add fixture tests in `acdc-parser/fixtures/tests/` (paired `.adoc` and `.json` files)
- Add unit tests alongside the code they test
- Add integration tests in the appropriate `tests/` directory

## Code Style

### Formatting

The project uses `rustfmt`. Format your code before committing:

```bash
# Format all code
cargo fmt --all

# Check formatting without making changes
cargo fmt --all -- --check
```

### Linting

The project uses `clippy` with pedantic lints. Run it before submitting:

```bash
# Run clippy with pedantic lints
cargo clippy --all-targets --all-features -- --deny clippy::pedantic

# Or allow some pedantic warnings if needed
cargo clippy --all-targets --all-features
```

### Code Quality Standards

- **No unsafe code**: The workspace forbids unsafe code (`unsafe_code = "forbid"`)
- **Exhaustive matching**: Use `wildcard_enum_match_arm = "deny"` to catch missing enum cases
- **Documentation**: Add doc comments for public APIs
- **Error handling**: Use `thiserror` for error types

## Commit Guidelines

This project uses **Conventional Commits**. Format your commit messages as:

```
<type>: <description>

[optional body]

[optional footer]
```

### Commit Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Examples

```
feat: add support for table row spanning

fix: correct inline markup parsing in code spans

docs: update README with new CLI options

test: add fixture tests for verbatim blocks
```

## Project Structure

```
acdc/
├── acdc-cli/                 # Command-line interface
│   └── src/
│       └── main.rs          # CLI entry point
├── acdc-lsp/                 # Language Server Protocol (early/experimental)
│   └── src/
│       ├── capabilities/    # LSP features (diagnostics, hover, definition)
│       └── state/           # Document and workspace state management
├── acdc-parser/             # Core parser and AST
│   ├── src/
│   │   ├── grammar/         # PEG grammar definitions
│   │   ├── model/           # AST data structures
│   │   ├── preprocessor/    # Include and conditional handling
│   │   └── proptests/       # Property-based testing
│   └── fixtures/            # Test fixtures
└── converters/              # Output converters
    ├── core/               # Shared traits (Processable, Visitor)
    ├── dev/                # Development utilities (unpublished)
    ├── html/               # HTML5 converter
    ├── manpage/            # Native roff/troff manpage output
    └── terminal/           # Rich terminal output
```

## Debugging

### Debug Parser Issues

Enable trace logging for the grammar module:

```bash
RUST_LOG=acdc_parser::grammar::document=trace cargo run --bin acdc -- convert file.adoc
```

### Compare with Reference Implementation

Compare output with asciidoctor (the reference implementation):

```bash
# Generate reference output
asciidoctor -o file.asciidoctor.html file.adoc

# Generate acdc output
cargo run --bin acdc -- convert file.adoc

# Compare
diff -u file.asciidoctor.html file.html
```

### Inspect AST

Use the inspect command to see the parsed AST:

```bash
cargo run --bin acdc -- inspect document.adoc
```

## Submitting Changes

1. **Create a branch**:
   ```bash
   git checkout -b feat/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Make your changes**:
   - Write code following the style guidelines
   - Add tests for new features or bug fixes
   - Update documentation as needed

3. **Run checks before committing**:
   ```bash
   # Format code
   cargo fmt --all
   
   # Run linter
   cargo clippy --all-targets --all-features
   
   # Run tests
   cargo nextest run
   ```

4. **Commit your changes**:
   - Use conventional commit format
   - Write clear, descriptive commit messages

5. **Push and create a Pull Request**:
   - Push your branch to your fork
   - Create a PR with a clear description of your changes
   - Reference any related issues

### Pull Request Checklist

Before submitting a PR, ensure:

- [ ] All tests pass (`cargo nextest run`)
- [ ] Code is formatted (`cargo fmt --all`)
- [ ] Clippy passes (`cargo clippy --all-targets --all-features`)
- [ ] Commit messages follow conventional commits
- [ ] Documentation is updated if needed
- [ ] New features have tests

## Getting Help

- Check existing issues and PRs
- Review the [README.adoc](README.adoc) and [ARCHITECTURE.adoc](ARCHITECTURE.adoc)
- Open an issue for questions or discussions

## Additional Resources

- [AsciiDoc documentation](https://docs.asciidoctor.org/asciidoc/latest/)
- [Language Specification](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc?ref_type=heads)
- [asciidoctor](https://asciidoctor.org) - Reference implementation

Thank you for contributing to acdc! 🎉
