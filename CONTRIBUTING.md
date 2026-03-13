# Contributing to Velocity

Thanks for your interest in contributing to Velocity! This document provides guidelines for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<your-username>/velocity.git`
3. Create a branch: `git checkout -b my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- For Android testing: Android SDK with `adb` in PATH
- For iOS testing: Xcode with command line tools, WebDriverAgent

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test --workspace
```

### Running the CLI

```bash
cargo run -- run tests/fixtures/simple_login.yaml
```

## Code Guidelines

- Follow standard Rust idioms and conventions
- Run `cargo clippy` before submitting
- Run `cargo fmt` to format code
- Add tests for new functionality
- Keep commits focused and atomic

## Architecture

Velocity is organized as a Cargo workspace:

| Crate | Purpose |
|-------|---------|
| `velocity-common` | Shared types, errors, config, traits |
| `velocity-core` | Test parsing, execution, selectors |
| `velocity-android` | Android platform driver (ADB) |
| `velocity-ios` | iOS platform driver (WDA/simctl) |
| `velocity-runner` | Test orchestration, parallelism, reporting |
| `velocity-mcp` | MCP server for AI-driven testing |
| `velocity-migrate` | Migration from other frameworks (Maestro) |
| `velocity-cli` | Command-line interface |

## Reporting Issues

- Use GitHub Issues
- Include reproduction steps
- Include platform details (OS, device/simulator info)
- Include Velocity version (`velocity version`)

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
