# winr Advanced Backend Phase 1 Decisions

This document records the concrete implementation decisions used to complete Phase 1 of the advanced backend roadmap.

## Discovery

- attachable targets are discovered from top-level HWND enumeration
- selectors match by HWND, PID, title hint, class name, and executable name
- candidate ordering is deterministic:
  - foreground windows first
  - visible windows before non-visible windows
  - non-minimized windows before minimized windows
  - then title and HWND as tie-breakers

## Process metadata

Each attachable target now carries process metadata including:

- process architecture
- integrity level
- loaded module names
- executable path
- likely rendering window

This metadata is attached to `AdvancedAttachableTarget` so later attach logic can make better decisions without needing to rediscover basic process facts.

## Attachment lifecycle

Phase 1 attachment does not perform injection yet. Instead, it defines the host-side lifecycle around an attachable target:

- attach
- heartbeat
- detach
- reattach when the matched process changes and policy allows it

This lifecycle currently lives in `AttachmentSupervisor` inside `winr-inject`.

## Heartbeat and health

Attachment health is tracked using:

- `healthy`
- `stale`
- `lost`

Heartbeat failures accumulate against a threshold. Before the threshold is reached the attachment becomes `stale`; after the threshold it becomes `lost`.

## Restart and reattach policy

Reattach behavior is explicit through `AdvancedReattachMode`:

- `never`
- `if_process_restarted`

When reattach is enabled and a matching target is rediscovered with a different PID, the host-side attachment updates to the new target and emits a reattach event.

## Failure observability

Attachment failures are surfaced through:

- structured attachment events
- attachment health state
- last-error detail strings

That gives later CLI, MCP, or workflow layers something concrete to report without pretending injection has already happened.
