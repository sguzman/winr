# Advanced Backend Phase 4 Decisions

This document records the concrete Phase 4 decisions for render observation.

## Phase 4 Checklist

- [x] Treat render observation as one backend, not as the entire advanced architecture
- [x] Hook frame presentation or a similar rendering boundary
- [x] Extract frame timing and frame availability
- [x] Expose sampled image data or analysis hooks
- [x] Support overlay or debug visualization in development builds
- [x] Use render observation for visible-scene understanding, template or object detection, and action correlation
- [x] Avoid treating render observation as a supported game-state API
- [x] Avoid treating render observation as a true background input channel

## Core Decision

Render observation is modeled as one replaceable backend slice, not as the definition of the advanced backend.

- `winr-inject::RenderObservationBackend` is the process-side contract for render-backed capture.
- `winr-perception::RenderObservationDetails` is the normalized metadata attached to render-backed frames.
- The rest of the architecture still allows desktop screenshots, memory-backed state, and future semantic adapters to coexist beside render observation.

This keeps Direct3D or similar hooking from dominating the entire design.

## Presentation Boundary

Phase 4 now treats frame presentation as the canonical render capture boundary.

- `RenderHookBoundary` records whether capture is associated with `dxgi_present`, `d3d11_present`, `d3d12_present`, `vulkan_present`, `open_gl_swap_buffers`, or an unknown boundary.
- `StubRenderObserver` captures render-backed frames as if they were taken at a presentation boundary and stores that fact in normalized frame metadata.

This is the important design decision even before a real hook exists: the boundary is explicit, versioned, and inspectable.

## Timing And Availability

Render-backed frames now carry timing and readiness metadata.

- `RenderFrameTiming` records present timestamp plus optional frame interval and capture latency.
- `RenderFrameAvailability` records whether a frame is ready, how many presents have been observed, and how many were dropped since the last capture.
- `ObservationFrame.render_details` stores both timing and availability alongside the frame handle.

This gives later phases a place to reason about freshness, detector cadence, and correlation between observations and actions.

## Sampling And Analysis

Phase 4 exposes two separate mechanisms instead of mixing them together.

- `RenderSampleRegion` provides explicit sampled subregions that can reference separate payloads.
- `RenderFrameAnalyzer` provides analysis hooks that can consume normalized frames and return overlays.
- `RenderObservationBackend::sample_regions` and `RenderObservationBackend::analyze_frame` keep these responsibilities explicit.

This split matters because sampling raw frame regions and analyzing them are related but not the same thing.

## Debug Visualization

Development overlays are now first-class render metadata.

- `RenderDebugOverlaySurface` stores development-only overlay surfaces.
- `DebugOverlayCommand` stores normalized overlay instructions like bounding boxes or heatmaps.
- `RenderObservationBackend::debug_overlay_surface` exposes this as a development aid rather than production logic.

This keeps debugging support in the design without coupling workflows to overlays.

## Intended Uses

Render observation now declares what it is good for.

- `RenderSceneUseCase` explicitly lists visible-scene understanding, template detection, object detection, and action correlation.
- `RenderObservationDetails.intended_uses` records which of those purposes a capture path is serving.

That makes it clear why render observation exists: it is for seeing and interpreting the rendered scene, not for pretending the renderer is a stable gameplay API.

## Explicit Non-Claims

The render model now carries two explicit guardrails.

- `does_not_claim_game_state_api`
- `does_not_claim_background_input_channel`

These are intentionally affirmative fields in the normalized metadata so traces and tests can prove we are not silently treating render hooks as stronger than they are.

## Current Limits

Phase 4 still stops short of a live Direct3D or Vulkan hook.

- No real presentation hook is installed yet.
- No GPU readback path is implemented yet.
- No production overlay renderer is implemented yet.
- No game-specific scene extraction is implemented yet.

That is expected for this phase. The render backend boundary, metadata, sampling path, and debug surfaces are now explicit enough to support real hook work later without redesigning the observation model again.
