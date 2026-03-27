# MCP Tools Update — Agent Productivity Tools

Three new tools added to the Velocity MCP server to improve agent efficiency and debugging workflows.

---

## 1. `get_screen_summary`

Returns a concise, LLM-friendly summary of the current screen instead of dumping the full accessibility tree.

**Why:** `list_elements` returns every element as raw JSON — often 100+ nodes. Agents burn tokens parsing data they don't need. This tool extracts only what matters for decision-making.

**Parameters:** None

**Response:**
```json
{
  "screen_title": "Login",
  "screen_text": ["Welcome back", "Enter your credentials", "Forgot password?"],
  "interactive": [
    { "type": "TextField", "label": "Email", "enabled": true },
    { "type": "TextField", "label": "Password", "enabled": true },
    { "type": "Button", "label": "Sign In", "enabled": true }
  ],
  "navigation": ["Home", "Profile", "Settings"],
  "counts": {
    "total": 47,
    "visible": 32,
    "interactive": 5,
    "text_fields": 2,
    "buttons": 3
  }
}
```

**What it extracts:**
- `screen_title` — first NavigationBar/Header element's text
- `screen_text` — all visible text/labels, deduplicated
- `interactive` — buttons, text fields, switches, sliders, checkboxes, links
- `navigation` — tab bar / bottom nav / toolbar child labels
- `counts` — element totals for quick orientation

---

## 2. `wait_for_element`

Blocks until a matching element becomes visible, or times out. Replaces manual poll loops.

**Why:** Without this, agents must loop `list_elements` or `get_element` with sleep calls — burning tool calls and tokens on each retry. This tool handles polling internally and returns once the element is ready.

**Parameters:**

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| `selector` | object | Yes | — | Element selector (e.g. `{"text": "Welcome"}`, `{"id": "home_screen"}`) |
| `timeout_ms` | integer | No | 10000 | Max wait time (capped at 30000ms) |
| `poll_interval_ms` | integer | No | 500 | Polling interval (minimum 100ms) |

**Success response:**
```json
{
  "found": true,
  "waited_ms": 1200,
  "element": {
    "label": "Welcome",
    "text": "Welcome to the app",
    "bounds": { "x": 0, "y": 100, "width": 375, "height": 44 },
    "enabled": true,
    "visible": true
  }
}
```

**Timeout:** Returns `ElementNotFound` error with the selector and timeout duration.

**Error handling:**
- `ElementNotFound` during polling is silently retried (expected — element not yet loaded)
- Real driver errors (connection lost, session expired) propagate immediately

---

## 3. `open_inspector` / `close_inspector`

Launch or stop the Inspector web UI from within an MCP session.

**Why:** When agents hit selector issues or unexpected screen states, there's no way to visually debug mid-session. This bridges MCP and Inspector — agents can open the Inspector for a human to inspect, or for screenshot-based debugging.

### `open_inspector`

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| `port` | integer | No | 9876 | Port for the web UI. Auto-increments if busy (tries up to 10 ports). |

**Response:**
```json
{
  "status": "started",
  "url": "http://localhost:9876",
  "port": 9876
}
```

**Idempotent:** Calling again while running returns `"status": "already_running"` with the existing URL.

**Implementation:** Spawns the Inspector as a background tokio task sharing the same `PlatformDriver` — no second device connection needed.

### `close_inspector`

**Parameters:** None

**Response:**
```json
{ "status": "stopped" }
```

Returns `"status": "not_running"` if no inspector is active.

---

## Agent workflow example

```
1. tap_element(selector: {text: "Login"})
2. wait_for_element(selector: {id: "login_screen"}, timeout_ms: 5000)
3. get_screen_summary()
   → sees 2 text fields (Email, Password) and a Sign In button
4. type_text(selector: {label: "Email"}, text: "user@test.com")
5. type_text(selector: {label: "Password"}, text: "password123")
6. tap_element(selector: {text: "Sign In"})
7. wait_for_element(selector: {text: "Welcome"}, timeout_ms: 10000)
   → blocks until home screen loads
8. get_screen_summary()
   → confirms we're on the home screen with expected navigation
```

If something goes wrong at step 7:
```
8. open_inspector()
   → returns http://localhost:9876 for visual debugging
9. ... human inspects, agent retries ...
10. close_inspector()
```
