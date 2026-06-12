# winr Safety Model

`winr` applies the same safety policy to CLI and MCP calls because both frontends use the same `winr-core` operations.

## Config path

Default config path on Windows:

```text
%APPDATA%\winr\config.toml
```

Override the path for testing or custom setups with:

```powershell
$env:WINR_CONFIG = "C:\path\to\winr-config.toml"
```

## Example config

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

[mcp]
bind = "127.0.0.1"
transport = "stdio"
log_tool_calls = true
```

## Current enforcement

- `allow_screenshots` gates desktop and window screenshots
- `allow_input` gates `input text`, `input keys`, `input sequence`, `uia invoke`, and `uia set-text`
- `allow_mouse` gates `mouse click` and `mouse click-window`
- `allow_window_close` gates `window close`
- `require_confirm_for_close` requires `winr window close --force`
- `allowlist.processes` limits risky target actions to explicit executables when non-empty
- `denylist.processes` blocks risky target actions for explicit executables
- `denylist.titles` blocks risky target actions when the window title contains a sensitive substring

Window listing and metadata inspection are not blocked by allowlists or denylists.

## Integrity levels

Before risky target actions, `winr` compares the current process integrity level with the target process integrity level. If the target is higher, `winr` returns `IntegrityLevelDenied` instead of pretending input or control succeeded.

This applies to:

- `input text`
- `input keys`
- `input sequence`
- `mouse click-window`
- `uia invoke`
- `uia set-text`
- `window close`

## Structured errors

Common safety-related JSON errors:

```json
{
  "ok": false,
  "error": "PermissionDenied",
  "message": "permission denied: screenshots are disabled by config",
  "matches": [],
  "uia_matches": []
}
```

```json
{
  "ok": false,
  "error": "IntegrityLevelDenied",
  "message": "integrity level mismatch prevented the operation",
  "matches": [],
  "uia_matches": []
}
```

```json
{
  "ok": false,
  "error": "UnsupportedForMinimizedWindow",
  "message": "unsupported for minimized window during mouse_click_window",
  "matches": [],
  "uia_matches": []
}
```

## Notes

- `foreground` input uses `SendInput` and is only reliable for the actual foreground window
- `message` input is still subject to the same config, allowlist, denylist, and integrity checks
- `message` input is classic-Win32-oriented and may work against background `Edit`-style controls, but not against many modern custom-rendered apps
- `uia` input depends on the target application exposing useful accessibility patterns
- minimized targets are rejected for risky direct-input flows rather than producing misleading success
