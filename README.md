# Velocity

Fast, reliable mobile UI testing framework written in Rust.

Velocity is a cross-platform mobile testing tool that drives iOS simulators and Android emulators through a simple YAML-based test format. It's designed for speed, reliability, and seamless integration with AI-driven testing workflows via MCP.

## Features

- **Cross-platform** — test iOS (via WebDriverAgent) and Android (via ADB/UIAutomator) with one tool
- **YAML test format** — simple, readable test definitions with selectors, assertions, and flow control
- **Parallel execution** — run tests across multiple devices simultaneously with smart sharding
- **MCP server** — expose device interaction as tools for AI agents
- **Maestro migration** — migrate existing Maestro test suites to Velocity format
- **Resilient drivers** — built-in retry policies and circuit breakers for flaky device connections
- **Rich reporting** — JSON and JUnit reporters, test history tracking, artifact collection

## Quick Start

### Install

```bash
cargo install velocity-cli
```

Or build from source:

```bash
git clone https://github.com/nicvit/velocity.git
cd velocity
cargo build --release
```

### Write a Test

```yaml
# tests/login.yaml
name: Login Flow
platform: ios

steps:
  - tap:
      id: "email_field"
  - type:
      text: "user@example.com"
  - tap:
      id: "password_field"
  - type:
      text: "secret123"
  - tap:
      id: "login_button"
  - assert:
      visible:
        id: "welcome_screen"
```

### Run It

```bash
velocity run tests/login.yaml
```

### Run on Multiple Devices

```bash
velocity run tests/ --parallel --shards 4
```

## Commands

| Command | Description |
|---------|-------------|
| `velocity run <path>` | Run test files or directories |
| `velocity device list` | List available simulators/emulators |
| `velocity device boot <id>` | Boot a simulator/emulator |
| `velocity migrate maestro <path>` | Convert Maestro tests to Velocity |
| `velocity mcp` | Start the MCP server |
| `velocity validate <path>` | Validate test configuration |

## MCP Integration

Velocity includes a built-in MCP (Model Context Protocol) server that exposes device interaction as tools, enabling AI agents to drive mobile testing:

```bash
velocity mcp --port 3000
```

This exposes tools for tapping, typing, querying elements, and running flows — allowing LLMs to autonomously test mobile apps.

## Architecture

```
velocity-cli          CLI entry point
  ├── velocity-core     Test parsing, execution engine
  ├── velocity-runner   Orchestration, parallelism, reporting
  ├── velocity-android  Android driver (ADB)
  ├── velocity-ios      iOS driver (WDA/simctl)
  ├── velocity-mcp      MCP server
  ├── velocity-migrate  Framework migration tools
  └── velocity-common   Shared types, errors, traits
```

## Platform Requirements

### iOS
- macOS with Xcode installed
- Xcode Command Line Tools
- WebDriverAgent (auto-bootstrapped on first run)

### Android
- Android SDK with platform-tools
- `adb` available in PATH
- An emulator or connected device

## Configuration

Velocity looks for configuration in `velocity.yaml` or via CLI flags:

```yaml
# velocity.yaml
platform: ios
timeout: 30s
retry:
  max_attempts: 3
  backoff: exponential
parallel:
  max_workers: 4
reporters:
  - json
  - junit
```

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
