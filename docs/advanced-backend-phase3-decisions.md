# Advanced Backend Phase 3 Decisions

This document records the concrete Phase 3 decisions for the observation stack.

## Phase 3 Checklist

- [x] Support desktop screenshot mode as one observation source
- [x] Support render-hook frame mode as one observation source
- [x] Support memory-backed state mode as one observation source
- [x] Support optional OCR and detector overlays
- [x] Normalize all observation sources into one internal observation frame model
- [x] Include timestamp, target identity, backend source, image or frame handle, camera hints, player-state hints, detected entities, confidence metrics, and freshness markers in that model
- [x] Keep workflows independent from the specific observation source that produced the frame

## Core Decision

Phase 3 keeps one normalized observation contract in `winr-perception` instead of letting each backend invent its own frame shape.

- `ObservationFrame` is the single workflow-facing DTO.
- `ObservationSourceData` carries source-specific payload details for desktop screenshots, render-hook frames, memory snapshots, and detector overlays.
- `ObservationMetadata` carries version, backend, source kind, frame id, timestamp, and freshness.
- `ObservationCaptureContext` gives sources one shared capture context for target identity and temporal metadata.

This means the workflow layer can reason over entities, detector outputs, camera hints, and player-state hints without caring whether they originated from pixels, hooked frames, or memory-backed state.

## Source Model

Phase 3 defines four first-class observation source families.

1. Desktop screenshots
   Represented by `ObservationSourceData::DesktopScreenshot` with `ObservationImageHandle`.

2. Render-hook frames
   Represented by `ObservationSourceData::RenderHookFrame` with `ObservationFrameHandle`.

3. Memory-backed state
   Represented by `ObservationSourceData::MemoryState` with a snapshot id and normalized state fields.

4. Detector overlays
   Represented by `ObservationSourceData::DetectorOverlay` and `DetectorOverlay`.

The source-specific details stay attached to the frame for debugging and future specialized processing, but the rest of the system still reads one normalized frame type.

## Binary Payloads

Image and frame bytes are not embedded directly into the normalized frame.

- `ObservationImageHandle` and `ObservationFrameHandle` reference `AdvancedBinaryPayloadRef`.
- This keeps large payload transfer aligned with the Phase 2 transport design.
- The normalized frame can travel separately from the actual pixel payload when needed.

That gives us a clean path for shared-memory frame transport later without changing workflow-facing observation DTOs.

## Hints And Confidence

Phase 3 includes optional semantic hints even before the memory observation phase is fully implemented.

- `CameraHints` models yaw, pitch, field of view, and camera mode.
- `PlayerStateHints` models world position, velocity, health, movement state, and active modes.
- `ObservationConfidenceSummary` gives a normalized place for overall and aggregate confidence values.

These are optional on purpose. Desktop capture may provide none of them, render observation may provide some of them, and memory-backed observation may provide stronger values later.

## Source Registration

`ObservationFrameSource` is the capture trait for the stack.

- It advertises source kind.
- It advertises backend capabilities.
- It describes detectors.
- It captures a normalized frame using `ObservationCaptureContext`.

`ObservationStack` owns a collection of `ObservationFrameSource` implementations and collects frames from all registered sources.

For now, `StaticObservationSource` exists as a test and protocol-development stand-in so the stack shape can be validated without needing real screenshot, hook, or memory implementations.

## Workflow Independence

Phase 3 treats source independence as something to verify, not merely something to claim.

- `winr-workflows` still plans against `ObservationFrame`.
- A workflow test now proves that equivalent desktop-backed and render-backed observations can produce the same plan when the entity content is the same.

That keeps the boundary honest: source-specific logic belongs in observation adapters and packs, not in the generic planner contract.

## Current Limits

Phase 3 deliberately stops before implementing real capture engines.

- Desktop screenshots are modeled as an observation source but not yet connected to a concrete `winr-core` capture adapter in `winr-perception`.
- Render-hook frames are modeled as an observation source but no Direct3D hook exists yet.
- Memory-backed state is modeled as an observation source but no reader is implemented yet.
- OCR and overlays are represented in the model, but no OCR pipeline is implemented yet.

That is the intended boundary for this phase. The stack contract is now explicit enough to plug real capture backends into later phases without redesigning the frame model again.
