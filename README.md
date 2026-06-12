# winr

`winr` is a Windows 11 desktop automation toolkit in Rust. The current milestone provides a strong CLI foundation for discovering windows, inspecting metadata, manipulating top-level windows, and capturing screenshots with honest error reporting.

## Current capabilities

- Enumerate top-level desktop windows
- Filter windows by HWND, PID, title substring, class name, and executable name
- Inspect one resolved window in human-readable or JSON form
- Report the current foreground window
- Attempt to focus a selected window
- Restore, minimize, maximize, move, resize, and close selected windows
- Capture desktop screenshots with GDI
- Capture window screenshots with `PrintWindow` or GDI fallback
- Emit extensive structured logs with `tracing`

## Project layout

- `crates/winr-types`: shared DTOs, selectors, HWND helpers, and error payloads
- `crates/winr-core`: Win32-backed window enumeration and focus logic
- `crates/winr-cli`: the `winr` command-line interface
- `docs/roadmap.md`: milestone tracker and upcoming work
- `tmp/project.md`: product spec currently guiding implementation

## Architecture

`winr` keeps Windows API work in `winr-core` and exposes typed operations to the CLI. The CLI is responsible for argument parsing, log initialization, formatting results, and returning stable JSON contracts.

This crate boundary is intentional. Future screenshot, input, UI Automation, and MCP work can reuse the same core behavior instead of re-implementing Win32 calls in multiple frontends.

## Windows limitations

`winr` is designed to be honest about Windows foreground restrictions:

- `SetForegroundWindow` is constrained by Windows focus rules
- some windows cannot be focused programmatically from another process
- elevated windows may reject interaction from non-elevated callers
- minimized and background interaction beyond focus is intentionally out of scope for this milestone

When focus fails, `winr` returns a structured `ForegroundDenied` or related error instead of pretending the action succeeded.

## Build and run

Requirements:

- Windows 11
- Rust nightly or stable with edition 2024 support

Build:

```powershell
cargo check
cargo test
```

Run:

```powershell
cargo run -p winr-cli -- windows list
cargo run -p winr-cli -- windows list --json
cargo run -p winr-cli -- windows foreground --json
cargo run -p winr-cli -- window info --title Notepad --json
cargo run -p winr-cli -- window focus --hwnd 0x0012034A
cargo run -p winr-cli -- window restore --title Notepad
cargo run -p winr-cli -- window move --title Notepad --x 100 --y 100 --width 1280 --height 720
cargo run -p winr-cli -- screenshot desktop --out target\\desktop.png
cargo run -p winr-cli -- screenshot window --title Notepad --out target\\notepad.png --backend auto
```

## Command reference

List windows:

```powershell
winr windows list
winr windows list --visible
winr windows list --exe Code.exe --json
winr windows list --title Notepad --json
```

Inspect a single window:

```powershell
winr window info --hwnd 0x0012034A --json
winr window info --pid 1234
winr window info --class Notepad
```

Get the foreground window:

```powershell
winr windows foreground
winr windows foreground --json
```

Focus a window:

```powershell
winr window focus --title Notepad
winr window focus --exe Code.exe --json
```

Window actions:

```powershell
winr window restore --title Notepad
winr window minimize --exe Code.exe
winr window maximize --class CabinetWClass
winr window move --title Notepad --x 100 --y 100
winr window move --title Notepad --x 100 --y 100 --width 1280 --height 720
winr window resize --title Notepad --width 1280 --height 720
winr window close --hwnd 0x0012034A --json
```

Screenshots:

```powershell
winr screenshot desktop --out target\desktop.png
winr screenshot desktop --out target\desktop.jpg --backend gdi
winr screenshot window --title Notepad --out target\notepad.png
winr screenshot window --title Notepad --out target\notepad.png --backend print-window
winr screenshot window --exe Code.exe --out target\code.jpg --backend gdi
```

## JSON output

Successful commands return:

```json
{
  "ok": true,
  "data": {
    "hwnd": "0x000000000012034A"
  }
}
```

Failed commands return:

```json
{
  "ok": false,
  "error": "WindowNotFound",
  "message": "no windows matched the provided selector",
  "matches": []
}
```

`HWND` values are always serialized as uppercase hexadecimal strings.

## Logging

`winr` uses `tracing` throughout the CLI and core library. By default, it logs at `info`. Increase verbosity with `RUST_LOG`.

Examples:

```powershell
$env:RUST_LOG = "debug"
cargo run -p winr-cli -- windows list --json

$env:RUST_LOG = "winr_core=trace,winr_cli=debug"
cargo run -p winr-cli -- window focus --title Notepad
```

The current logging coverage includes:

- startup and parsed command routing
- selector normalization and match counts
- Win32 enumeration and foreground checks
- focus attempts and failures
- screenshot backend selection and fallback behavior
- JSON error conversion

## Roadmap summary

The current milestone delivers the CLI-first foundation. Planned follow-up work includes:

- screenshots for desktop and windows
- keyboard and mouse input
- Windows UI Automation support
- permissions and safety policy
- MCP server integration after the core behavior is proven locally

The current command tree is:

```text
winr windows list
winr windows foreground
winr screenshot desktop
winr screenshot window
winr window info
winr window focus
winr window restore
winr window minimize
winr window maximize
winr window move
winr window resize
winr window close
```

See [docs/roadmap.md](docs/roadmap.md) for the tracked checklist.
