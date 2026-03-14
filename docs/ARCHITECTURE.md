# Velocity — Architecture & Internals

## Project Tree

```
velocity/
├── Cargo.toml                          # Workspace root — defines all crate members
├── Cargo.lock
├── install.sh                          # One-line installer (curl | bash)
├── Formula/velocity.rb                 # Homebrew formula
├── LICENSE                             # Apache 2.0
├── README.md
├── CONTRIBUTING.md
│
├── velocity-cli/                       # CLI binary — entry point for everything
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                     # Arg parsing, command dispatch, exit codes
│       └── commands/
│           ├── mod.rs
│           ├── run.rs                  # `velocity run` — execute test suites
│           ├── device.rs               # `velocity device` — list/boot/shutdown
│           ├── validate.rs             # `velocity validate` — check YAML
│           ├── migrate.rs              # `velocity migrate maestro` — convert flows
│           └── mcp.rs                  # `velocity mcp` — start MCP server
│
├── crates/
│   ├── velocity-common/                # Shared foundation — types, traits, errors
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Re-exports everything
│   │       ├── types.rs                # Selector, Element, Rect, Action, Direction, Key
│   │       ├── test_types.rs           # TestCase, TestSuite, Flow, Step, SuiteConfig
│   │       ├── error.rs                # VelocityError, ErrorKind (transient/permanent)
│   │       ├── result.rs               # TestResult, StepResult, SuiteResult, TestStatus
│   │       ├── config.rs               # RuntimeConfig, ReportFormat
│   │       ├── traits.rs               # PlatformDriver trait, HealthStatus
│   │       └── resilience.rs           # RetryPolicy, CircuitBreaker, ResilientDriver
│   │
│   ├── velocity-core/                  # Test engine — parsing, execution, sync
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs              # YAML → TestSuite (serde + custom visitor)
│   │       ├── validator.rs           # Validates selectors, flow references
│   │       ├── env.rs                 # ${VAR}, ${VAR:-default}, ${VAR:?err}
│   │       ├── selector.rs            # SelectorEngine — LRU cache + generation
│   │       ├── sync.rs                # AdaptiveSyncEngine — UI idle detection
│   │       ├── streaming.rs           # Fast header-only parser for test discovery
│   │       ├── executor.rs            # TestExecutor — runs steps against driver
│   │       └── resolver.rs            # Resolves flow references into inline steps
│   │
│   ├── velocity-android/               # Android platform driver
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── adb.rs                 # ADB command wrapper (shell, tap, swipe, etc.)
│   │       ├── driver.rs              # AndroidDriver — PlatformDriver impl
│   │       ├── parser.rs              # UIAutomator XML → Element tree
│   │       └── selector.rs            # Selector → Element matching for Android
│   │
│   ├── velocity-ios/                   # iOS platform driver
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── simctl.rs              # xcrun simctl wrapper (boot, shutdown, list)
│   │       ├── wda.rs                 # WebDriverAgent HTTP client
│   │       ├── wda_bootstrap.rs       # Auto-builds and launches WDA on first run
│   │       ├── wda_manager.rs         # WDA session lifecycle + reconnection
│   │       ├── driver.rs              # IosDriver — PlatformDriver impl
│   │       └── parser.rs              # WDA XML → Element tree
│   │
│   ├── velocity-runner/                # Test orchestration layer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── runner.rs              # SuiteRunner — top-level test execution
│   │       ├── scheduler.rs           # Tag filter, name filter, shard assignment
│   │       ├── parallel.rs            # Multi-device parallel execution
│   │       ├── farm.rs                # DeviceFarm — pooled device management (semaphore)
│   │       ├── history.rs             # TestHistory — duration tracking for smart sharding
│   │       ├── artifacts.rs           # Screenshot + log collection
│   │       └── reporter/
│   │           ├── mod.rs
│   │           ├── json.rs            # JSON reporter
│   │           └── junit.rs           # JUnit XML reporter
│   │
│   ├── velocity-mcp/                   # MCP server for AI agents
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs              # JSON-RPC stdio server (stdin/stdout)
│   │       ├── session.rs             # McpSession — device state, caching
│   │       ├── tool_registry.rs       # ToolDefinition, ToolResponse (LLM-friendly)
│   │       └── tools/
│   │           ├── mod.rs
│   │           ├── device.rs          # list, boot, shutdown, select device
│   │           ├── interaction.rs     # tap, input_text, screenshot
│   │           ├── query.rs           # get_hierarchy, find_element
│   │           └── flow.rs            # run_flow, list_flows
│   │
│   └── velocity-migrate/               # Framework migration tools
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── maestro.rs             # Maestro YAML → Velocity YAML converter
│           └── compat.rs              # MigrationReport, severity, issue tracking
│
└── tests/
    ├── loyalty-app.yaml                # Real-world complex test suite
    └── fixtures/
        ├── simple_login.yaml           # Basic login flow with env vars
        ├── multi_flow.yaml             # Multiple reusable flows
        ├── sharding_test.yaml          # Sharding demonstration
        └── maestro_compat/
            ├── login.yaml              # Maestro migration input sample
            └── checkout.yaml           # Maestro migration input sample
```

---

## Dependency Graph

```
                         ┌─────────────────┐
                         │  velocity-cli    │
                         │  (binary entry)  │
                         └────────┬─────────┘
                                  │
           ┌──────────┬───────────┼───────────┬───────────┐
           ▼          ▼           ▼           ▼           ▼
    ┌────────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌──────────┐
    │  velocity-  │ │velocity│ │velocity│ │velocity│ │ velocity- │
    │   runner    │ │  -mcp  │ │-android│ │  -ios  │ │ migrate   │
    │(orchestrate)│ │(AI svr)│ │ (ADB)  │ │ (WDA)  │ │(maestro)  │
    └──────┬──────┘ └───┬────┘ └───┬────┘ └───┬────┘ └─────┬─────┘
           │            │          │           │            │
           ▼            ▼          │           │            │
    ┌─────────────┐     │          │           │            │
    │ velocity-   │     │          │           │            │
    │    core     │◄────┘          │           │            │
    │  (engine)   │                │           │            │
    └──────┬──────┘                │           │            │
           │                       │           │            │
           ▼                       ▼           ▼            ▼
    ┌──────────────────────────────────────────────────────────┐
    │                     velocity-common                      │
    │        (types, traits, errors, resilience)                │
    └──────────────────────────────────────────────────────────┘
```

Every crate depends on `velocity-common`. The `core` crate is used by `runner` and `mcp`. The platform crates (`android`, `ios`) only depend on `common` and implement the `PlatformDriver` trait.

---

## Module Deep Dive

### 1. `velocity-common` — The Foundation

Everything starts here. This crate defines the vocabulary that every other crate speaks.

#### Types (`types.rs`)

```
Selector ─────────────────────────────────────────────────────
  │
  ├── Id(String)              match by resource ID / accessibility ID
  ├── Text(String)            exact text match
  ├── TextContains(String)    substring match
  ├── AccessibilityId(String) accessibility label
  ├── ClassName(String)       element type (UIButton, EditText, etc.)
  ├── Index { selector, index }   Nth match of inner selector
  └── Compound(Vec<Selector>)     AND combination of multiple selectors


Action ───────────────────────────────────────────────────────
  │
  ├── LaunchApp { app_id, clear_state }
  ├── StopApp { app_id }
  ├── Tap { selector }
  ├── DoubleTap { selector }
  ├── LongPress { selector, duration_ms }
  ├── InputText { selector, text }
  ├── ClearText { selector }
  ├── AssertVisible { selector }
  ├── AssertNotVisible { selector }
  ├── AssertText { selector, expected }
  ├── ScrollUntilVisible { selector, direction, max_scrolls }
  ├── Swipe { direction, from, to }
  ├── Screenshot { filename }
  ├── PressKey { key }
  ├── Wait { ms }
  └── RunFlow { flow_id }


Element ──────────────────────────────────────────────────────
  │
  ├── platform_id: String        driver-specific element ID
  ├── label: Option<String>      accessibility label
  ├── text: Option<String>       visible text
  ├── element_type: String       class name (UIButton, android.widget.TextView)
  ├── bounds: Rect { x, y, width, height }
  ├── enabled: bool
  ├── visible: bool
  └── children: Vec<Element>     recursive hierarchy
```

#### Error Classification (`error.rs`)

Errors are classified by recoverability, which drives the retry engine:

```
                    ┌─────────────┐
                    │  ErrorKind  │
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │  Transient  │ │  Permanent  │ │  Ambiguous   │
    │ (auto-retry)│ │ (fail fast) │ │ (retry once) │
    └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
           │               │               │
    ConnectionLost   ElementNotFound    Timeout
    SessionExpired   InvalidSelector    Unknown
    ElementStale     AssertionFailed
    DeviceNotReady   PlatformNotSupported
                     ConfigError


Exit Codes:
  1 → Test failure (assertion, element not found)
  2 → Config error (bad YAML, invalid selector)
  3 → Device error (not found, WDA unhealthy)
  4 → Suite timeout
  5 → Internal error
```

#### PlatformDriver Trait (`traits.rs`)

The core abstraction. Both `AndroidDriver` and `IosDriver` implement this:

```
PlatformDriver (async_trait)
  │
  │  Lifecycle
  ├── prepare(device_id)         boot WDA, connect ADB, etc.
  ├── cleanup()                  tear down sessions
  ├── health_check() → HealthStatus
  ├── restart_session()          recover from session errors
  │
  │  Device Management
  ├── list_devices() → Vec<DeviceInfo>
  ├── boot_device(device_id)
  ├── shutdown_device(device_id)
  ├── install_app(device_id, app_path)
  ├── launch_app(device_id, app_id, clear_state)
  ├── stop_app(device_id, app_id)
  │
  │  Element Queries
  ├── find_element(device_id, selector) → Element
  ├── find_elements(device_id, selector) → Vec<Element>
  ├── get_hierarchy(device_id) → Element (full tree)
  │
  │  Interactions
  ├── tap(device_id, element)
  ├── double_tap(device_id, element)
  ├── long_press(device_id, element, duration_ms)
  ├── input_text(device_id, element, text)
  ├── clear_text(device_id, element)
  ├── swipe(device_id, direction)
  ├── swipe_coords(device_id, from, to)
  ├── press_key(device_id, key)
  │
  │  Screen
  ├── screenshot(device_id) → Vec<u8>    (PNG bytes)
  ├── screen_size(device_id) → (i32, i32)
  ├── get_element_text(device_id, element) → String
  └── is_element_visible(device_id, element) → bool
```

#### Resilience Layer (`resilience.rs`)

Wraps any `PlatformDriver` with automatic retry + circuit breaker:

```
┌───────────────────────────────────────────────────────────┐
│                    ResilientDriver                         │
│                                                           │
│  ┌─────────────┐    ┌─────────────────┐                   │
│  │ RetryPolicy │    │ CircuitBreaker  │                   │
│  │             │    │                 │                   │
│  │ max: 3      │    │ States:         │                   │
│  │ initial:100ms│   │  CLOSED → normal│                   │
│  │ max: 5s     │    │  OPEN → reject  │                   │
│  │ multiplier:2│    │  HALF_OPEN →    │                   │
│  └──────┬──────┘    │   test 1 req    │                   │
│         │           └────────┬────────┘                   │
│         ▼                    ▼                             │
│  ┌──────────────────────────────────────┐                  │
│  │           with_retry(action)         │                  │
│  │                                      │                  │
│  │  1. Check circuit breaker            │                  │
│  │  2. Execute action                   │                  │
│  │  3. On success → reset breaker       │                  │
│  │  4. On transient error →             │                  │
│  │     sleep(backoff + jitter)          │                  │
│  │     retry up to error.max_retries()  │                  │
│  │  5. On permanent error → fail fast   │                  │
│  │  6. On session error → restart_session│                 │
│  └──────────────────────────────────────┘                  │
│                         │                                  │
│                         ▼                                  │
│              ┌──────────────────┐                           │
│              │  Inner Driver    │                           │
│              │ (Android or iOS) │                           │
│              └──────────────────┘                           │
└───────────────────────────────────────────────────────────┘
```

---

### 2. `velocity-core` — The Engine

This is where YAML becomes test execution.

#### Parser (`parser.rs`)

Converts YAML test files into typed `TestSuite` objects:

```
                    YAML File
                       │
                       ▼
              ┌─────────────────┐
              │   serde_yaml    │
              │  deserialize    │
              └────────┬────────┘
                       │
                       ▼
              ┌─────────────────┐
              │  Custom Visitor │  ← Handles flexible YAML structure
              │  (DslStep)      │    e.g. "tap:" vs "tapOn:" vs nested
              └────────┬────────┘
                       │
                       ▼
              ┌─────────────────┐
              │  SelectorDsl →  │  ← Converts flat fields to Selector enum
              │  Selector       │    { id: "foo" } → Selector::Id("foo")
              └────────┬────────┘
                       │
                       ▼
              ┌─────────────────┐
              │   TestSuite     │
              │  ├── app_id     │
              │  ├── config     │
              │  ├── flows[]    │
              │  └── tests[]    │
              └─────────────────┘
```

#### Validator (`validator.rs`)

Runs after parsing, before execution:

```
validate_suite(suite)
  │
  ├── For each test → For each step:
  │     ├── Selector actions → validate_selector()
  │     │     ├── Check value not empty
  │     │     ├── Index: validate inner selector
  │     │     └── Compound: validate all children
  │     │
  │     └── RunFlow actions → verify flow_id exists in suite.flows
  │
  └── Returns Ok(()) or ConfigError
```

#### Environment Resolution (`env.rs`)

Interpolates variables before execution:

```
Input:  "Hello ${USER}, env is ${ENV:-staging}"
                │                    │
                ▼                    ▼
         1. Check overrides    1. Check overrides
         2. Check std::env     2. Check std::env
         3. Error if missing   3. Use default "staging"
                │                    │
                ▼                    ▼
Output: "Hello omkar, env is staging"

Supported patterns:
  ${VAR}              → required, error if missing
  ${VAR:-default}     → fallback to "default"
  ${VAR:?Custom err}  → required, custom error message
```

#### Selector Engine (`selector.rs`)

LRU cache with generation-based invalidation:

```
┌─────────────────────────────────────────────┐
│              SelectorEngine                  │
│                                              │
│  cache: HashMap<key, CachedElement>          │
│  generation: AtomicU64                       │
│  max_size: 256 entries                       │
│  ttl: 30 seconds                             │
│                                              │
│  find_element(driver, device, selector):     │
│    │                                         │
│    ├── Cache HIT?                            │
│    │    ├── Generation matches?              │
│    │    ├── TTL not expired?                 │
│    │    ├── Still visible on screen?         │
│    │    └── YES to all → return cached       │
│    │                                         │
│    └── Cache MISS → query driver             │
│         ├── Store result in cache            │
│         └── LRU evict if full                │
│                                              │
│  invalidate_generation():                    │
│    └── Atomic increment → all entries stale  │
│                                              │
│  invalidate_cache():                         │
│    └── Full clear (after app launch/stop)    │
└─────────────────────────────────────────────┘
```

#### Adaptive Sync Engine (`sync.rs`)

The secret to flakiness-free testing. Detects when the UI is stable before acting:

```
wait_for_idle(driver, device_id)
  │
  ├── FAST PATH (if history available):
  │     predicted_time = average(recent_samples) * 1.2
  │     if predicted < 200ms:
  │       sleep(predicted)
  │       verify UI hash unchanged
  │       if stable → return (skip polling)
  │
  └── POLLING PATH:
        interval = min_interval (config.interval / 4)
        consecutive_stable = 0
        required = config.stability_count (default 3)

        loop:
          ┌─────────────────────────┐
          │  Get UI hierarchy hash  │
          └────────────┬────────────┘
                       │
               ┌───────┴────────┐
               │ Same as last?  │
               └───────┬────────┘
                  YES  │  NO
              ┌────────┤────────┐
              ▼                 ▼
        stable_count++     stable_count = 0
        interval *= 1.5    interval = min_interval
              │                 │
              ▼                 │
        count >= required?      │
          YES → return          │
          NO  ──────────────────┘
                       │
                       ▼
                 elapsed > timeout?
                   YES → SyncTimeout error
                   NO  → sleep(interval), continue loop

  Hash function:
    hash_element(element) → u64
      Combines: platform_id + text + bounds + child_count
      Recursively hashes entire subtree
      Identical trees → identical hash
```

#### Executor (`executor.rs`)

The core execution loop that runs each test:

```
execute_test(test, device_id, app_id)
  │
  │  For each Step in test.steps:
  │    │
  │    ├── 1. PRE-SYNC
  │    │     └── sync_engine.wait_for_idle()
  │    │         (skip for Wait/Screenshot actions)
  │    │
  │    ├── 2. EXECUTE ACTION
  │    │     ├── LaunchApp → driver.launch_app()
  │    │     ├── Tap → selector_engine.find() → driver.tap()
  │    │     ├── InputText → find() → driver.clear() → driver.input()
  │    │     ├── AssertVisible → find() → is_visible?
  │    │     ├── AssertText → find() → get_text() → compare
  │    │     ├── ScrollUntilVisible → loop { find → swipe → sync }
  │    │     ├── Screenshot → driver.screenshot() → save to disk
  │    │     └── ... (all Action variants handled)
  │    │
  │    ├── 3. POST-SYNC (if action mutates UI)
  │    │     ├── selector_engine.invalidate_generation()
  │    │     └── sync_engine.wait_for_idle() (best-effort)
  │    │
  │    └── 4. COLLECT RESULT
  │          └── StepResult { status, duration, error?, screenshot? }
  │
  └── Return TestResult { status, steps[], duration, retries }
```

#### Streaming Parser (`streaming.rs`)

Lightweight header-only parser for fast test discovery (used by scheduler):

```
Full Parser:     Reads entire YAML, allocates all steps    ~5ms per file
Streaming Parser: Single pass, extracts headers only       ~0.1ms per file

TestHeader:
  name, tags, isolated, step_count, byte_offset

Used by: scheduler to determine test count + tags
         before doing expensive full parse
```

---

### 3. `velocity-android` — Android Driver

```
┌──────────────────────────────────────────────────────┐
│                   AndroidDriver                       │
│                                                       │
│  ┌─────────┐   ┌───────────────┐   ┌──────────────┐  │
│  │   Adb   │   │ XML Parser    │   │  Selector    │  │
│  │ wrapper │   │ (UIAutomator) │   │  Matcher     │  │
│  └────┬────┘   └───────┬───────┘   └──────┬───────┘  │
│       │                │                   │          │
│       ▼                ▼                   ▼          │
│  adb -s <id>    quick_xml →         DFS matching     │
│  shell <cmd>    Element tree        with visibility   │
│                                     checks            │
│  Commands:      Attributes:         Strategies:       │
│  - input tap    - resource-id       - ID (substring)  │
│  - input text   - text              - Text (exact)    │
│  - input swipe  - content-desc      - TextContains    │
│  - screencap    - class             - AccessibilityId │
│  - uiautomator  - bounds [x,y][x,y] - ClassName      │
│    dump         - enabled/visible   - Compound (AND)  │
│                                     - Index (Nth)     │
│  Hierarchy Cache:                                     │
│  TTL = 500ms (avoids repeated dumps)                  │
└──────────────────────────────────────────────────────┘
```

**How Android element finding works:**

```
find_element(device_id, Selector::Id("login_btn"))
  │
  ├── Check hierarchy cache (500ms TTL)
  │     MISS → adb shell uiautomator dump
  │            parse XML → Element tree
  │            store in cache
  │
  ├── DFS traverse Element tree
  │     For each element:
  │       is_visible(element, screen_bounds)?
  │       resource_id contains "login_btn"?
  │       OR label == "login_btn"?
  │
  └── Return first match or ElementNotFound
```

---

### 4. `velocity-ios` — iOS Driver

```
┌──────────────────────────────────────────────────────┐
│                     IosDriver                         │
│                                                       │
│  ┌──────────┐   ┌──────────────┐   ┌──────────────┐  │
│  │  Simctl  │   │  WdaClient   │   │WdaBootstrap  │  │
│  │ (xcrun)  │   │  (HTTP API)  │   │(auto-build)  │  │
│  └────┬─────┘   └──────┬───────┘   └──────┬───────┘  │
│       │                │                   │          │
│       ▼                ▼                   ▼          │
│  xcrun simctl   POST /session/     Clone WDA repo    │
│  list/boot/     {id}/element/      xcodebuild        │
│  shutdown       {eid}/click        xcrun simctl spawn │
│                                    Health check loop  │
│                                                       │
│  Selector Translation:                                │
│  ┌──────────────────┬──────────────────────────────┐  │
│  │ Velocity         │ WDA                          │  │
│  ├──────────────────┼──────────────────────────────┤  │
│  │ Id("foo")        │ ("accessibility id", "foo")  │  │
│  │ Text("bar")      │ ("name", "bar")              │  │
│  │ TextContains("x")│ ("class chain", predicate)   │  │
│  │ AccessibilityId  │ ("accessibility id", aid)    │  │
│  │ ClassName("Btn") │ ("class name", "Btn")        │  │
│  │ Compound(a,b)    │ ("class chain", combined)    │  │
│  └──────────────────┴──────────────────────────────┘  │
│                                                       │
│  WDA Lifecycle:                                       │
│  prepare() → ensure WDA running on device             │
│  cleanup() → kill WDA process                         │
│  restart_session() → create new WDA session           │
└──────────────────────────────────────────────────────┘
```

**WDA Bootstrap flow (first run):**

```
ensure_running(device_id)
  │
  ├── Is WDA already responding? → YES → done
  │
  └── NO
       ├── Clone WebDriverAgent repo (if missing)
       ├── xcodebuild build-for-testing
       ├── xcrun simctl spawn <device_id> WDA
       └── Poll health endpoint until ready
```

---

### 5. `velocity-runner` — Orchestration

#### SuiteRunner — End-to-end execution

```
SuiteRunner::run(suite, config, driver)
  │
  ├── 1. Parse YAML → TestSuite
  ├── 2. Validate structure
  ├── 3. Interpolate env vars
  ├── 4. Resolve flow references (inline steps)
  ├── 5. Filter tests
  │       ├── filter_by_tags(tests, config.tags)
  │       └── filter_by_name(tests, config.test_filter)
  ├── 6. Shard tests (if config.shard_index set)
  │       └── shard_tests(tests, index, total, history)
  ├── 7. Select device
  │       ├── config.device_id if set
  │       └── or first booted device from list_devices()
  ├── 8. Execute tests sequentially
  │       for test in tests:
  │         ├── Check suite timeout
  │         ├── TestExecutor::execute_test()
  │         ├── If failed + retries configured → retry
  │         └── Collect TestResult
  └── 9. Return SuiteResult
```

#### Scheduler — Smart test distribution

```
Tag Filtering:
  tests: [A(tags:["smoke"]), B(tags:["regression"]), C(tags:["smoke","e2e"])]
  filter: ["smoke"]
  result: [A, C]

Name Filtering (glob):
  tests: ["Login_Basic", "Login_SSO", "Checkout_Cart"]
  pattern: "Login*"
  result: ["Login_Basic", "Login_SSO"]

Sharding:

  WITHOUT history (hash-based):
    tests: [A, B, C, D, E]
    shards: 3
    shard 0: [A, D]      (hash(name) % 3 == 0)
    shard 1: [B, E]      (hash(name) % 3 == 1)
    shard 2: [C]          (hash(name) % 3 == 2)

  WITH history (duration-balanced bin packing):
    tests: [A(10s), B(5s), C(8s), D(3s), E(12s)]
    shards: 2
    Sort descending: [E(12s), A(10s), C(8s), B(5s), D(3s)]
    Greedy assign to lightest shard:
      shard 0: E(12s) → E+C(20s) → E+C+D(23s)
      shard 1: A(10s) → A+B(15s)
    Result: Near-equal execution time per shard
```

#### DeviceFarm — Pooled device management

```
┌───────────────────────────────────────────────────┐
│                  DeviceFarm                        │
│                                                    │
│  ┌─────────────┐     ┌───────────────────┐         │
│  │  Semaphore   │     │    FarmState      │         │
│  │ (max_devices)│     │                   │         │
│  └──────┬──────┘     │ devices:           │         │
│         │            │   "iPhone-15": ✓   │         │
│         │            │   "iPhone-14": ✗   │ (in_use)│
│         │            │   "Pixel-7":   ✓   │         │
│         │            └───────────────────┘         │
│         ▼                                          │
│  acquire():                                        │
│    1. Acquire semaphore permit (blocks if full)    │
│    2. Find device where in_use == false            │
│    3. Mark in_use = true                           │
│    4. Return Lease { device_id, permit }           │
│                                                    │
│  Lease (RAII):                                     │
│    On drop → mark in_use = false                   │
│              release semaphore permit               │
└───────────────────────────────────────────────────┘
```

---

### 6. `velocity-mcp` — AI Agent Interface

```
┌─────────────────────────────────────────────────────────┐
│                      MCP Server                          │
│                                                          │
│   stdin ──────► JSON-RPC Parser ──────► Tool Router      │
│                                              │           │
│                ┌─────────────────────────────┬┘           │
│                ▼              ▼              ▼            │
│         ┌──────────┐  ┌───────────┐  ┌──────────┐       │
│         │  Device   │  │Interaction│  │  Query   │       │
│         │  Tools    │  │  Tools    │  │  Tools   │       │
│         ├──────────┤  ├───────────┤  ├──────────┤       │
│         │list      │  │tap        │  │hierarchy │       │
│         │boot      │  │input_text │  │find_elem │       │
│         │shutdown  │  │screenshot │  │          │       │
│         │select    │  │           │  │          │       │
│         └──────────┘  └───────────┘  └──────────┘       │
│                ▼              ▼              ▼            │
│         ┌──────────┐                                     │
│         │  Flow    │  McpSession:                        │
│         │  Tools   │    current_device                   │
│         ├──────────┤    device_list_cache (30s TTL)      │
│         │run_flow  │    last_screenshot                  │
│         │list_flows│    config_path                      │
│         └──────────┘                                     │
│                                                          │
│   ToolResponse ──────► JSON-RPC Response ──────► stdout  │
│     summary: "Tapped login button at (120, 340)"         │
│     data: { element: { ... } }                           │
│     next_steps: ["Assert welcome screen visible"]        │
└─────────────────────────────────────────────────────────┘
```

**How an LLM uses it:**

```
LLM                           Velocity MCP Server
 │                                    │
 ├── tools/list ─────────────────────►│ Returns available tools
 │◄── [{tap, input_text, ...}] ──────│
 │                                    │
 ├── tools/call {                     │
 │     "tap": {selector:{id:"login"}} │
 │   } ─────────────────────────────►│ Finds element, taps
 │◄── {summary:"Tapped login at      │
 │     (120,340)", next_steps:[...]}──│
 │                                    │
 ├── tools/call {                     │
 │     "screenshot": {}               │
 │   } ─────────────────────────────►│ Captures screen
 │◄── {summary:"Screenshot saved",   │
 │     data:{path:"/tmp/shot.png"}} ──│
```

---

### 7. `velocity-migrate` — Framework Migration

```
velocity migrate maestro ./maestro-flows/ ./velocity-tests/

Input (Maestro):                    Output (Velocity):
┌──────────────────────┐            ┌──────────────────────┐
│ appId: com.app       │            │ app_id: com.app      │
│ ---                  │     ──►    │ config:              │
│ - launchApp          │            │   platform: ios      │
│ - tapOn:             │            │ tests:               │
│     id: "login"      │            │   - name: login      │
│ - inputText: "hello" │            │     steps:           │
│ - assertVisible:     │            │       - launchApp:   │
│     id: "welcome"    │            │           id: com.app│
└──────────────────────┘            │       - tap:         │
                                    │           id: "login"│
                                    │       - inputText:   │
                                    │           text:"hello│
                                    │       - assertVisible│
                                    │           id:"welcome│
                                    └──────────────────────┘

Unsupported Maestro constructs (logged as warnings):
  runScript, evalScript, onFlowStart, onFlowComplete,
  repeat, condition, copyTextFrom, eraseText,
  hideKeyboard, openLink, setLocation,
  startRecording, stopRecording, waitForAnimationToEnd
```

---

## Full Execution Flow

End-to-end: from `velocity run tests/login.yaml` to exit code.

```
┌─────────────┐
│  CLI Parse   │  velocity run tests/login.yaml --platform ios --parallel
└──────┬──────┘
       ▼
┌─────────────┐
│  Load YAML  │  velocity_core::parse_suite("tests/login.yaml")
│  (Parser)   │  → TestSuite { app_id, config, flows, tests }
└──────┬──────┘
       ▼
┌─────────────┐
│  Validate   │  velocity_core::validate_suite(&suite)
│             │  Check selectors valid, flow refs exist
└──────┬──────┘
       ▼
┌─────────────┐
│  Env Resolve│  velocity_core::interpolate_suite(&mut suite, overrides)
│             │  ${API_URL:-localhost} → "localhost"
└──────┬──────┘
       ▼
┌─────────────┐
│  Flow Inline│  Replace RunFlow { flow_id } → actual flow steps
│  (Resolver) │
└──────┬──────┘
       ▼
┌─────────────┐
│  Create     │  Platform::Ios → IosDriver → ResilientDriver
│  Driver     │  Platform::Android → AndroidDriver → ResilientDriver
└──────┬──────┘
       ▼
┌─────────────┐
│  Prepare    │  driver.prepare(device_id)
│  Device     │  iOS: WDA bootstrap + session
│             │  Android: verify ADB connection
└──────┬──────┘
       ▼
┌─────────────┐
│  Filter &   │  filter_by_tags() → filter_by_name() → shard_tests()
│  Shard      │
└──────┬──────┘
       ▼
┌─────────────────────────────────────────────────┐
│  Execute Tests                                   │
│                                                  │
│  for each test:                                  │
│    ┌─────────────────────────────────────┐       │
│    │  for each step:                     │       │
│    │    ├── wait_for_idle()   (sync)     │       │
│    │    ├── execute_action()  (action)   │       │
│    │    │    ├── find_element (cache)    │       │
│    │    │    ├── driver.tap/type/assert  │       │
│    │    │    └── retry if transient err  │       │
│    │    ├── invalidate_cache (if mutating)│      │
│    │    └── wait_for_idle()  (post-sync) │       │
│    └─────────────────────────────────────┘       │
│                                                  │
│    If failed + retries > 0: re-run test          │
│    Collect TestResult                            │
└──────────────────────┬──────────────────────────┘
                       ▼
              ┌─────────────┐
              │  Report     │  write_junit() or write_json()
              │  Generate   │  → ./velocity-results/report.xml
              └──────┬──────┘
                     ▼
              ┌─────────────┐
              │  Cleanup    │  driver.cleanup()
              │  & Exit     │  iOS: kill WDA
              └──────┬──────┘  Android: no-op
                     ▼
              exit(suite_result.exit_code())
              0 = all passed, 1 = failures
```

---

## Usage Examples

### Basic test run

```bash
velocity run tests/login.yaml
```

### Specify platform and device

```bash
velocity run tests/ --platform android --device emulator-5554
```

### Parallel with sharding (CI)

```bash
# Machine 1:
velocity run tests/ --parallel --shard-index 0 --shard-total 3

# Machine 2:
velocity run tests/ --parallel --shard-index 1 --shard-total 3

# Machine 3:
velocity run tests/ --parallel --shard-index 2 --shard-total 3
```

### Filter by tags

```bash
velocity run tests/ --tags smoke,critical
```

### Filter by name

```bash
velocity run tests/ --filter "Login*"
```

### With retries and JUnit output

```bash
velocity run tests/ --retries 2 --report junit --artifacts-dir ./results
```

### Environment variable overrides

```bash
velocity run tests/ --env API_URL=https://staging.example.com --env USER=testuser
```

### Validate without running

```bash
velocity validate --config tests/login.yaml
```

### Migrate from Maestro

```bash
velocity migrate maestro ./maestro-flows/ ./velocity-tests/
```

### Start MCP server for AI agents

```bash
velocity mcp --device "iPhone 15 Pro"
```

### Device management

```bash
velocity device list
velocity device boot "iPhone 15 Pro"
velocity device shutdown "iPhone 15 Pro"
```
