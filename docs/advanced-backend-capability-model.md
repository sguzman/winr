# winr Advanced Backend Capability Model

This document describes how the advanced backend capability model currently works.

## Shared capability vocabulary

Capabilities are represented by `AdvancedBackendCapabilities` in `winr-types`.

Current capability flags:

- `foreground_input`
- `message_input`
- `uia_input`
- `injected_input`
- `render_observation`
- `memory_observation`
- `semantic_navigation`
- `entity_tracking`
- `internal_interaction`

## Requirements

Capability requirements are represented by `AdvancedCapabilityRequirements`.

These requirements describe what a workflow or profile needs from a backend instead of naming a target-specific implementation directly.

## Backend descriptors

Backends advertise themselves through `AdvancedBackendDescriptor`.

Each descriptor includes:

- backend identity
- stability
- capability flags
- replaceability metadata
- whether the backend is app-pack specific
- human notes

## Catalog and selection

`winr-inject` currently provides:

- a frontend-scoped backend catalog
- a capability-based selector
- capability matching and scoring

Selection works by:

1. building a capability catalog for the active frontend
2. deriving capability requirements from the profile or workflow
3. finding descriptors whose capabilities satisfy the requirements
4. selecting the highest-ranked satisfying backend

## Current mappings

Current profile-derived requirements:

- mouse-click profile with `input_mode = "message"` requires `message_input`
- mouse-click profile with default or foreground input requires `foreground_input`

Current backend descriptors:

- `foreground`
  - stable
  - supports `foreground_input`
- `message`
  - experimental
  - supports `message_input`
- `inject`
  - fragile
  - currently scaffolded with no enabled capabilities yet

## Why this matters

This model lets `winr` choose backends based on what they can do instead of hardcoding behavior around one app family.
