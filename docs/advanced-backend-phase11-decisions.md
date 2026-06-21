# Advanced Backend Phase 11 Decisions

This document records the concrete Phase 11 decisions for reliability and observability.

## Phase 11 Checklist

- [x] Add replayable traces of observations and actions
- [x] Add structured event logs
- [x] Add backend health summaries or dashboards
- [x] Add execution reasoning so operators can answer "why did it do that"
- [x] Add stale-state detection
- [x] Add frame freshness tracking
- [x] Add command acknowledgment tracking
- [x] Treat observability as a first-class requirement because injected and app-specific backends fail in subtler ways than ordinary Win32 automation

## Core Decision

Phase 11 makes observability a shared contract instead of a debugging afterthought.

- `winr-types` now owns replay, health, command-ack, structured-event, and reasoning DTOs
- `winr-inject` now records session-level command history, observation history, structured events, and health summaries
- `winr-perception` now evaluates frame freshness and stores replayable observation tapes
- `winr-workflows` now turns execution traces into operator-facing reasoning summaries

This keeps reliability signals available across host, agent, perception, and workflow boundaries.

## Replayable Traces

Replay is now an explicit data shape.

- `AdvancedReplayTrace` stores structured events, command records, and observation updates
- `AdvancedBackendSession::replay_trace()` exports the current session trace
- `ObservationReplayTape` stores normalized `ObservationFrame` instances for perception-side replay

The host can now preserve what it saw and what it asked the backend to do without depending on ad hoc log parsing.

## Structured Event Logs

We now log important runtime events with a stable schema.

- `AdvancedStructuredEventKind` covers command queueing, command acknowledgment, observation receipt, stale observations, lifecycle changes, reasoning, and errors
- `AdvancedStructuredEvent` records backend, event sequence, timestamp, and detail
- `AdvancedBackendSession` emits structured events as commands, responses, and observations pass through it

That gives later CLI, MCP, or dashboard work one event stream to consume.

## Health Summary

We now summarize backend health directly from session state.

- `AdvancedBackendHealthSummary` reports lifecycle state
- it reports last host and agent sequence numbers
- it reports the most recent observation frame id and freshness
- it reports pending, acknowledged, and rejected command counts
- it reports whether the latest observation is stale
- it includes the most recent recorded reasoning, when available

This is intentionally lightweight, but it is enough to answer whether the backend is alive, current, and responding.

## Frame Freshness And Stale-State Detection

Observation freshness is now first-class.

- `AdvancedObservationUpdate` now carries optional `timestamp_ms` and `freshness_ms`
- `ObservationFrame::from_update(...)` preserves freshness from the update when present
- `ObservationFreshnessPolicy` and `ObservationFreshnessAssessment` classify frames as `fresh`, `aging`, or `stale`
- `AdvancedBackendSession` marks observations stale when freshness crosses the configured threshold

This matters because an injected or custom-rendered backend can look healthy while still making decisions on old state.

## Command Acknowledgment Tracking

Host command delivery is now visible instead of assumed.

- `AdvancedCommandRecord` tracks each host command by sequence
- `AdvancedCommandAckStatus` distinguishes `pending`, `acked`, and `rejected`
- `AdvancedBackendSession::apply_response(...)` ties responses back to their originating command

That gives us the minimum reliable base for later retry logic, timeout handling, and operator diagnostics.

## Execution Reasoning

The system now records why it made a choice in a reusable format.

- `AdvancedExecutionReason` carries a summary plus supporting basis lines
- `WorkflowExecutionTrace::reasoning()` turns recent workflow events into an explanation
- `AdvancedBackendSession::record_reasoning(...)` stores the latest operator-facing reason in the session

This is the beginning of explainable workflow behavior rather than a full reasoning engine, which is the right boundary for this phase.

## Current Limit

Phase 11 does not yet include a user-facing dashboard or persisted trace storage.

- health summaries are in-memory DTOs
- replay traces are exported from session state, not yet written to disk automatically
- reasoning is still event-derived, not a full decision graph

That is acceptable for this phase. The important result is that the advanced backend now has explicit trace, freshness, acknowledgment, and reasoning contracts that later reliability work can build on.
