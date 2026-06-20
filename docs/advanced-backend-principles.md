# winr Advanced Backend Core Principles

This document records how the advanced backend core principles are reflected in the current codebase.

## Separate rendering, state-reading, and input

- `winr-perception` owns generic observation, detector, and entity models
- `winr-inject` owns advanced backend routing, session lifecycle, and attachable-target discovery
- `winr-workflows` owns task, intent, planning, and execution-trace structures

## Prefer capability detection over app-specific assumptions

- advanced backend routing uses backend selection metadata and capability-oriented descriptors
- `AdvancedBackendDescriptor` and `AdvancedBackendCapabilities` describe what a backend can do without requiring target-specific branching

## Represent automation in terms of intents and observations

- `ObservationFrame` models what the system sees
- `WorkflowIntentDefinition` and `WorkflowIntentKind` model what the system wants to do
- workflow plans depend on intents and detected entity kinds instead of raw keystroke loops

## Keep app-specific behavior in adapters or packs

- target-specific manifests live under `packs/`
- generic crates define pack interfaces and registries, but not target-specific behavior
- `packs/roblox/pack.toml` is the first pack scaffold

## Treat every low-level hook as fragile and replaceable

- `AdvancedBackendDescriptor` includes stability and replaceability metadata
- the current injected backend scaffold marks itself as `fragile` and `replaceable`

## Build for traceability

- protocol envelopes use session ids and sequence numbers
- workflow execution has trace event and execution-trace structures
- advanced backend errors carry structured machine-readable payloads even before richer top-level error integration exists
