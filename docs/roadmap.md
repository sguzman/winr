# winr Roadmap

## Foundation

- [x] Convert the repo into a Rust workspace with `winr-types`, `winr-core`, and `winr-cli`
- [x] Preserve existing ignore rules while restructuring the project
- [x] Add shared dependency management in the workspace manifest
- [x] Wire in `tracing` and `tracing-subscriber` from the first milestone

## Window inventory

- [x] Enumerate top-level windows
- [x] Capture window title, class, PID, visibility, minimized state, foreground state, and bounds
- [x] Support filtering by HWND, PID, title substring, class name, and executable name
- [x] Expose JSON output for list and inspect commands

## Window actions

- [x] Report the current foreground window
- [x] Focus a selected window with structured error handling
- [x] Restore, minimize, maximize, move, resize, and close windows

## Screenshots

- [x] Capture desktop screenshots
- [x] Capture window screenshots
- [x] Add backend selection for GDI and `PrintWindow`, with room for future modern capture

## Input

- [x] Send text to the foreground window
- [x] Send keyboard combos and key sequences
- [x] Send mouse clicks and window-relative clicks

## UI Automation

- [x] Inspect UI Automation trees
- [x] Find UI elements by accessible metadata
- [x] Invoke controls and set text through UI Automation

## MCP

- [x] Add an MCP server crate
- [x] Expose a safe initial tool surface
- [x] Reuse `winr-core` behavior without direct Win32 calls in the MCP layer

## Safety model

- [ ] Add configurable permissions and process allow/deny lists
- [ ] Detect integrity-level mismatch and return clear errors
- [ ] Expand structured error coverage for dangerous or unsupported actions

## Docs and quality

- [x] Create a comprehensive `README.md`
- [x] Add unit tests for selectors, HWND parsing, and JSON serialization
- [x] Add CLI integration tests for JSON output and error flows
- [x] Document UI Automation and MCP workflows
- [ ] Add screenshot and input validation coverage
- [ ] Document future safety policy and permission workflows in more depth
