# Velocity

**The fastest mobile UI testing framework. Written in Rust.**

[Install](#installation) | [Quick Start](#quick-start) | [Why Velocity](#why-velocity) | [Contributing](CONTRIBUTING.md)

---

Velocity is a cross-platform mobile testing framework built for speed and reliability. It drives iOS simulators and Android emulators through a simple YAML-based test format — no compilation, no flaky waits, no Java runtime required.

## Installation

### Homebrew (macOS / Linux)

```bash
brew tap omkar-dev/velocity https://github.com/omkar-dev/velocity.git
brew install velocity
```

### Shell script

```bash
curl -fsSL "https://raw.githubusercontent.com/omkar-dev/velocity/main/install.sh" | bash
```

Works on **macOS** and **Linux** (x86_64 and ARM64). No JVM required.

### Cargo

```bash
cargo install --git https://github.com/omkar-dev/velocity.git velocity-cli
```

## Quick Start

**Write a test** in simple YAML:

```yaml
# login_test.yaml
name: Login Flow
platform: ios

steps:
  - launchApp:
      id: "com.example.app"
  - tapOn:
      id: "email_field"
  - inputText:
      text: "user@example.com"
  - tapOn:
      id: "password_field"
  - inputText:
      text: "secret123"
  - tapOn:
      id: "login_button"
  - assertVisible:
      id: "welcome_screen"
```

**Run it:**

```bash
velocity run login_test.yaml
```

**That's it.** No setup, no boilerplate, no waiting.

## Why Velocity

### Built on the lessons of Appium, Espresso, UITest, and Maestro

We studied every major mobile testing tool and built something faster.

| Feature | Velocity | Maestro | Appium |
| --- | :---: | :---: | :---: |
| Language | Rust | Kotlin/JVM | Node.js/Java |
| Startup time | **~50ms** | ~3s | ~10s |
| No JVM required | **Yes** | No | No |
| YAML test format | **Yes** | Yes | No |
| Built-in flakiness tolerance | **Yes** | Yes | No |
| Parallel execution | **Yes** | Cloud only | Plugin |
| Built-in sharding | **Yes** | Cloud only | No |
| AI-native (MCP server) | **Yes** | Studio only | No |
| Circuit breaker / retry | **Yes** | Basic | No |
| Real-time streaming | **Yes** | No | No |
| JUnit + JSON reports | **Yes** | Yes | Plugin |
| Migrate from Maestro | **Yes** | — | — |
| Single binary, zero deps | **Yes** | No | No |
| Memory footprint | **~15MB** | ~200MB+ | ~500MB+ |

### Rust-native performance

Velocity is a single static binary with no runtime dependencies. No JVM warmup, no garbage collection pauses, no `node_modules`. Tests start executing in milliseconds, not seconds.

### Built-in flakiness tolerance

Every interaction automatically waits for elements to become available with configurable timeouts. The driver layer includes retry policies with exponential backoff and circuit breakers that prevent cascading failures when a device becomes unresponsive.

### Parallel-first execution

Run tests across multiple devices simultaneously with intelligent sharding — locally, not just in the cloud:

```bash
# Run across 4 devices in parallel
velocity run tests/ --parallel --shards 4
```

### AI-native testing via MCP

Velocity includes a built-in MCP (Model Context Protocol) server that exposes device interaction as tools. Connect any LLM agent to autonomously explore and test your mobile app:

```bash
velocity mcp --port 3000
```

Exposed tools: `tap`, `type`, `swipe`, `scroll`, `query_elements`, `assert_visible`, `run_flow`, `screenshot`, `device_info` — everything an AI agent needs to drive your app.

### Migrate from Maestro in one command

Already using Maestro? Switch in seconds:

```bash
velocity migrate maestro ./maestro-flows/
```

Converts your existing Maestro YAML flows to Velocity format automatically.

## Features

- **Cross-platform** — iOS (WebDriverAgent + simctl) and Android (ADB + UIAutomator)
- **YAML test format** — human-readable, no compilation needed
- **Parallel execution** — local multi-device sharding with smart scheduling
- **Resilient drivers** — circuit breakers, retry policies, exponential backoff
- **MCP server** — AI agents can drive mobile devices as tools
- **Real-time streaming** — live test output and device state streaming
- **Device farm** — manage and orchestrate pools of simulators/emulators
- **Rich reporting** — JSON, JUnit XML, test history, artifact collection
- **Framework migration** — one-command migration from Maestro
- **Config validation** — validate test files before execution
- **Environment support** — variable substitution and per-environment configs
- **Smart selectors** — ID, text, accessibility label, XPath, class, and index selectors

## Commands

```text
velocity run <path>                   Run test files or directories
velocity run <path> --parallel        Run tests in parallel
velocity run <path> --shards N        Split tests across N devices
velocity device list                  List available simulators/emulators
velocity device boot <id>             Boot a simulator/emulator
velocity device shutdown <id>         Shut down a device
velocity migrate maestro <path>       Convert Maestro flows to Velocity
velocity mcp                          Start the MCP server
velocity validate <path>              Validate test configuration
velocity version                      Print version info
```

## Configuration

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
artifacts:
  screenshots: on_failure
  logs: always
```

## Architecture

```text
velocity-cli            CLI entry point (single binary)
  ├── velocity-core       Test parsing, execution engine, selectors
  ├── velocity-runner     Orchestration, parallel sharding, reporters
  ├── velocity-android    Android driver (ADB / UIAutomator)
  ├── velocity-ios        iOS driver (WebDriverAgent / simctl)
  ├── velocity-mcp        MCP server for AI-driven testing
  ├── velocity-migrate    Framework migration (Maestro → Velocity)
  └── velocity-common     Shared types, errors, traits, resilience
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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.
