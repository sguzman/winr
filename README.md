# winr

`winr` is a Rust workspace for Windows 11 desktop automation. It ships a CLI-first core for real window operations, screenshots, multiple input backends, UI Automation, and a stdio MCP server that reuses the same core behavior.

## Purpose

The project is built around a simple rule: prove the Windows primitives locally from a normal CLI before exposing them to AI agents. `winr` keeps that boundary by putting Windows API work in `winr-core`, shared contracts in `winr-types`, and thin frontends on top.

## Current capabilities

- Enumerate top-level windows
- Filter windows by `HWND`, PID, title substring, class name, and executable name
- Inspect the foreground window or one resolved window
- Focus, restore, minimize, maximize, move, resize, and close windows
- Capture desktop screenshots with GDI
- Capture window screenshots with `PrintWindow` and GDI fallback
- Send text, key combos, and key sequences through `foreground`, `uia`, or classic-Win32-oriented `message` backends
- Send screen clicks and window-relative clicks
- Inspect UI Automation trees
- Find UI Automation elements by accessible metadata
- Invoke UIA controls and set UIA-backed text values
- Expose a safe first MCP tool surface over stdio
- Emit structured logs with `tracing`

## Workspace layout

- `crates/winr-types`: shared DTOs, selectors, JSON payloads, and error contracts
- `crates/winr-core`: Win32, screenshot, input, and UI Automation implementation
- `crates/winr-cli`: the `winr` binary
- `crates/winr-mcp`: stdio MCP server backed only by `winr-core`
- `docs/roadmap.md`: checkbox roadmap and milestone tracker
- `docs/safety.md`: config format, permission model, and integrity-level behavior
- `tmp/project.md`: current product spec driving implementation

## Architecture

`winr-cli` depends only on `winr-core` and `winr-types`. It parses flags, initializes logging, formats output, and never calls raw Win32 APIs directly.

`winr-mcp` also depends only on `winr-core` and `winr-types`. MCP tools are wrappers around the same typed core operations used by the CLI, so behavior stays aligned across human and agent usage.

This split leaves room for later safety policy, richer automation, and broader transports without duplicating Windows logic.

## Windows limitations

`winr` is intentionally honest about what Windows does and does not allow:

- `SetForegroundWindow` is subject to Windows focus restrictions
- elevated or protected targets may reject interaction from a normal process
- `foreground` input uses `SendInput` and is most reliable after restore and focus
- `message` input can work against background classic Win32 controls, but it is app-dependent and not universally reliable
- background or minimized-window interaction is still app-dependent and not treated as universally reliable
- UI Automation only works when the target application exposes useful accessibility patterns

When an action cannot be completed cleanly, `winr` returns structured errors such as `ForegroundDenied`, `WindowNotFound`, `AmbiguousWindow`, `UiaElementNotFound`, or `AmbiguousUiaElement`.

## Build

Requirements:

- Windows 11
- Rust stable with edition 2024 support

Verification commands:

```powershell
cargo check
cargo test
cargo run -p winr-cli -- windows list --json
```

## Install and run

Run the CLI directly from the workspace:

```powershell
cargo run -p winr-cli -- windows list
cargo run -p winr-cli -- window info --title Notepad --json
cargo run -p winr-cli -- window focus --hwnd 0x0012034A
```

Run the MCP server over stdio:

```powershell
cargo run -p winr-cli -- mcp serve
```

You can also launch the dedicated server binary:

```powershell
cargo run -p winr-mcp
```

## Command examples

Window inventory:

```powershell
winr windows list
winr windows list --visible --json
winr windows foreground --json
winr window info --title Notepad --json
```

Window actions:

```powershell
winr window focus --exe notepad.exe
winr window restore --title Notepad
winr window move --title Notepad --x 100 --y 100 --width 1280 --height 720
winr window resize --title Notepad --width 1280 --height 720
winr window close --hwnd 0x0012034A --force --json
```

Screenshots:

```powershell
winr screenshot desktop --out target\desktop.png
winr screenshot window --title Notepad --out target\notepad.png --backend auto
winr screenshot window --title Notepad --out target\notepad.png --backend print-window
```

Input:

```powershell
winr input text --title Notepad "hello world"
winr input keys --title Notepad --combo ctrl+l
winr input sequence --title Notepad --step ctrl+l --step text:https://example.com --step enter
winr input text --input-mode message --title Notepad "hello"
winr input keys --input-mode message --title Notepad --combo ctrl+a
winr input text --input-mode uia --title Notepad "hello from UIA"
winr mouse click --button left --x 100 --y 200
winr mouse click-window --title Notepad --x 40 --y 20
```

Profiles:

```powershell
winr profile run profile/roblox-grass-mower-simulator-auto-clicker.toml
winr profile run profile/roblox-grass-mower-simulator-auto-clicker.toml --focus-target
winr profile run profile/roblox-grass-mower-simulator-auto-clicker.toml --focus-target --arm-delay-ms 1500
winr profile run profile/roblox-grass-mower-simulator-auto-clicker.toml --max-clicks 100
```

Press `Ctrl+C` to stop a running profile cleanly.

For foreground-only clicker profiles, `--focus-target` is the easiest way to start from the terminal without manually alt-tabbing first. It asks Windows to bring the matched target forward before the loop begins.

Mouse-click profiles now click a stable client-area point inside the target window instead of blindly clicking wherever the cursor happens to be. If the profile does not specify `x` and `y`, `winr` will try to capture the current cursor position inside the target window and otherwise fall back to the window center.

Profile click actions can also use named presets such as `click_point = "center"` or `click_point = "current_cursor"`. Named presets cannot be combined with explicit `x` and `y` coordinates.

## Input backends

`winr` now supports three distinct input modes on `input text`, `input keys`, and `input sequence`:

- `foreground`: default mode. Uses `SendInput`, honors `focus_first`, and is the most universal path.
- `uia`: available for compatible text-entry scenarios. Best when the target app exposes useful accessibility value patterns.
- `message`: background-capable for classic Win32 windows and common child controls such as `Edit` or dialog-hosted text fields. It does not claim broad compatibility with Chromium, Electron, WinUI, or custom-rendered apps.

Quick guidance:

- classic Win32 apps and dialogs: likely good `message` candidates
- Notepad and standard `Edit`-based controls: good first targets
- Electron, Chromium, VS Code, Edge, and modern custom UI shells: likely unsupported in `message` mode
- if you need broad compatibility, use `foreground`

UI Automation:

```powershell
winr uia tree --title Notepad --max-depth 3 --json
winr uia find --title Notepad --name OK --json
winr uia invoke --title Calculator --name Equals
winr uia set-text --title Notepad --uia-class Edit --text "hello from winr"
```

MCP:

```powershell
winr mcp serve
```

## Command tree

```text
winr
  windows
    list
    foreground
  window
    info
    focus
    restore
    minimize
    maximize
    move
    resize
    close
  screenshot
    desktop
    window
  input
    text
    keys
    sequence
  mouse
    click
    click-window
  profile
    run
  uia
    tree
    find
    invoke
    set-text
  mcp
    serve
```

## JSON output

Success payloads always use:

```json
{
  "ok": true,
  "data": {}
}
```

Error payloads always use:

```json
{
  "ok": false,
  "error": "WindowNotFound",
  "message": "no windows matched the provided selector",
  "matches": [],
  "uia_matches": []
}
```

Notes:

- `HWND` values are serialized as uppercase hexadecimal strings like `0x000000000012034A`
- ambiguous top-level window selection uses `matches`
- ambiguous UIA selection uses `uia_matches`
- `InputActionResult` includes `mode` so callers can see which backend executed
- MCP tools return the same structured success and error shapes in `structuredContent`

## Logging

`winr` uses `tracing` throughout startup, selector resolution, Win32 calls, screenshot backend selection, input routing, backend selection, child-target heuristics, UIA operations, and MCP responses.

Default logging is `info`. Raise verbosity with `RUST_LOG`.

```powershell
$env:RUST_LOG = "debug"
cargo run -p winr-cli -- windows list --json

$env:RUST_LOG = "winr_core=trace,winr_cli=debug,winr_mcp=debug"
cargo run -p winr-cli -- uia tree --title Notepad --max-depth 2
```

## MCP tool surface

The current MCP server exposes:

- `windows_list`
- `window_info`
- `window_focus`
- `window_restore`
- `window_move`
- `window_screenshot`
- `input_send_keys`
- `input_send_text`
- `mouse_click`
- `uia_tree`
- `uia_find`
- `uia_invoke`
- `uia_set_text`

The MCP layer does not expose arbitrary shell execution.

`input_send_keys` and `input_send_text` accept an optional `input_mode` field with `foreground`, `uia`, or `message`. `uia` is intended for text-compatible flows.

## Safety and permissions

`winr` reads a config file from `%APPDATA%\winr\config.toml` by default, or from `WINR_CONFIG` when that environment variable is set.

Example:

```toml
[permissions]
allow_input = true
allow_mouse = true
allow_screenshots = true
allow_window_close = false
require_confirm_for_close = true

[allowlist]
processes = ["notepad.exe", "Code.exe"]

[denylist]
processes = ["KeePassXC.exe", "1Password.exe", "Bitwarden.exe"]
titles = ["Bank", "Password", "Authenticator"]
```

Current behavior:

- screenshots, input, mouse actions, and window close can be disabled by config
- risky target actions honor executable allowlists and process/title denylists
- `window close` requires `--force` when `require_confirm_for_close = true`
- risky actions against higher-integrity targets return `IntegrityLevelDenied`
- minimized targets now return `UnsupportedForMinimizedWindow` for unsafe direct-input flows
- `profile run` waits for a matching target window before starting and currently supports interval-based foreground mouse-click profiles

See [docs/safety.md](docs/safety.md) for the full workflow.

## Roadmap summary

Completed so far:

- CLI-first workspace foundation
- window inventory and actions
- screenshots
- foreground input
- UI Automation
- stdio MCP server

Still ahead:

- deeper UI Automation patterns and broader desktop compatibility

See [docs/roadmap.md](docs/roadmap.md) for the tracked checklist.
