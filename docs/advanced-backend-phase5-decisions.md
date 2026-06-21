# Advanced Backend Phase 5 Decisions

This document records the concrete Phase 5 decisions for memory and internal state observation.

## Phase 5 Checklist

- [x] Build a separate state-reading layer for targets that need richer automation than visual heuristics alone
- [x] Support potential signals such as player position, camera orientation, movement state, active tool or mode, nearby interactables, prompt state, and object lists
- [x] Keep memory-backed state optional
- [x] Version observations so changes can be detected across target updates
- [x] Prevent workflow code from depending directly on raw offsets or raw memory layouts
- [x] Translate low-level state into stable internal DTOs first

## Core Decision

Memory observation is now its own backend slice rather than just an unstructured `state_fields` bag.

- `winr-inject::MemoryObservationBackend` is the process-side contract for normalized state reads.
- `winr-perception::MemoryObservationDetails` is the workflow-facing DTO for versioned memory snapshots.
- `ObservationFrame` can now carry `memory_details` in parallel with `render_details`.

This keeps memory-backed observation as a sibling of render observation instead of mixing the two concerns together.

## Separate State-Reading Layer

Phase 5 introduces a dedicated state-reading path in `winr-inject`.

- `StubMemoryObserver` is the first concrete backend stand-in.
- It owns snapshot sequencing and normalized state extraction.
- It exposes focused accessors for player state, camera state, and nearby objects.

This is the architectural boundary we wanted before any real pointer walks, signature scans, or process-specific readers are introduced.

## Normalized DTOs

Low-level state is translated into stable DTOs before anything reaches workflow code.

- `MemoryObservationDetails`
- `MemoryPlayerState`
- `MemoryCameraState`
- `MemoryPromptState`
- `MemoryObjectState`

These DTOs intentionally describe automation-relevant meaning such as world position, prompt visibility, interactability, and active tool state. They do not expose raw addresses, offsets, or engine-specific layouts.

## Versioning

Memory snapshots are now explicitly versioned.

- `MemorySchemaVersion::V1` marks the normalized schema version.
- `snapshot_id` identifies the specific captured snapshot.
- `raw_layout_hidden` is stored explicitly to document that the DTO is a translation layer rather than a raw memory dump.

That gives us a stable place to evolve field mappings over time as targets update.

## Supported Signals

The Phase 5 model now has explicit normalized fields for the signals we know we will need.

- Player state: position, velocity, movement state, active tool, and active modes
- Camera state: yaw, pitch, field of view, and camera mode
- Prompt state: visible prompts and optional distances
- Object state: nearby objects, object category, labels, optional world positions, distances, and interactability

These are enough to support later behaviors like "move around this dirt patch" or "walk up to this rock" without forcing workflows to decode target-specific memory layouts.

## Optionality

Memory-backed state remains optional.

- `ObservationFrame.memory_details` is optional.
- The generic observation stack still supports desktop and render-only flows.
- Workflows can continue operating on visual-only observations when memory state is unavailable.

This keeps the system general and avoids making memory reading a requirement for every target.

## Projection Boundary

Phase 5 introduces `MemoryStateProjector` as a translation seam between memory snapshots and higher-level entities.

- The memory backend can project entities from normalized state.
- Workflow code still consumes `ObservationEntity` and `ObservationFrame`.
- Memory readers stay responsible for translation, not the planner.

That keeps raw state interpretation where it belongs and avoids teaching the workflow engine about target-specific internals.

## Current Limits

Phase 5 still stops short of a live process memory reader.

- No real pointer walking or signature scanning is implemented yet.
- No anti-update resilience strategy is implemented yet beyond schema versioning.
- No target-specific layout adapters are implemented yet.

That is expected for this phase. The important part is that memory observation is now represented as a separate, versioned, normalized backend contract that can grow without leaking raw layouts into the rest of the system.
