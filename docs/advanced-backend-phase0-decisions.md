# winr Advanced Backend Phase 0 Decisions

This document records the concrete architectural decisions made to complete Phase 0 of the advanced backend roadmap.

## Crate boundaries

- `winr-core` remains the standard desktop automation backend for Win32 windows, screenshots, foreground input, message input, and UI Automation.
- `winr-inject` owns advanced backend scaffolding, including backend routing, session state, protocol envelopes, and attachable-target discovery.
- `winr-types` owns shared DTOs, protocol messages, lifecycle state, backend selection metadata, and advanced backend error payloads.
- future crates:
  - `winr-perception` will own richer scene, detector, and observation models
  - `winr-workflows` will own higher-level workflow execution and navigation behaviors

## Frontend routing

- CLI and MCP should both use the same backend selection rules.
- frontends should not implement their own advanced backend routing logic.
- backend selection should be computed from shared code and carried as structured metadata.
- current frontend identifiers are:
  - `cli`
  - `mcp`

## Opt-in policy

- profile execution defaults to `[execution] backend = "auto"`.
- explicit profile backend values are:
  - `foreground`
  - `message`
  - `inject`
- `auto` currently infers:
  - `message` when a mouse-click profile explicitly uses `input_mode = "message"`
  - `foreground` otherwise
- `inject` is an explicit profile opt-in today.
- auto-detection is profile-driven, not process-driven, in the current milestone.

## Backend traits

The advanced backend contract is split into three responsibilities:

- observation backend:
  - target discovery
  - observation-facing target selection
- input backend:
  - session preparation
  - input-capable session establishment
- workflow backend:
  - execution-plan construction
  - coordinated use of observation and input backend behavior

These traits live in `winr-inject` for now because the only advanced backend implementation scaffold currently lives there.

## Error model

- advanced backend failures should be represented as structured advanced backend errors first
- those errors can still be surfaced through current `WinrError::Unsupported` plumbing until a richer top-level error integration is added
- current advanced backend error kinds include:
  - discovery failure
  - no attachable target
  - ambiguous attachable target
  - session mismatch
  - out-of-order sequence
  - invalid lifecycle transition
  - handshake mismatch
  - attach not implemented

## Lifecycle model

The shared advanced backend lifecycle states are:

- `discovered`
- `attachable`
- `attached`
- `degraded`
- `detached`

Allowed transitions are enforced by shared code instead of being left to individual backend implementations.
