# Advanced Backend Phase 7 Decisions

This document records the concrete Phase 7 decisions for the perception and entity model.

## Phase 7 Checklist

- [x] Create a shared world model that represents what the automation system believes about the target
- [x] Support core entities such as player, camera, region, prompt, interactable object, collectible object, obstacle, waypoint, and detected visual marker
- [x] Support detector families including template matching, color cluster matching, OCR, object detection, memory-backed entity extraction, and render-backed overlay or object extraction
- [x] Add stable tracking across frames
- [x] Add confidence smoothing
- [x] Add lost target handling
- [x] Add reacquisition logic
- [x] Add object prioritization

## Core Decision

Phase 7 introduces a persistent belief-state layer in `winr-perception` instead of treating each `ObservationFrame` as the whole story.

- `WorldModel` stores the system's current belief about the target.
- `TrackedObservationEntity` stores the tracked state for each entity across frames.
- `WorldModelTracker` updates that belief state from incoming observations.

This gives workflows a stable target-selection surface that is stronger than raw per-frame detections.

## Entity Vocabulary

The shared entity vocabulary remains the core one defined by `EntityKind`.

- `player`
- `camera`
- `region`
- `prompt`
- `interactable`
- `collectible`
- `obstacle`
- `waypoint`
- `visual_marker`

Phase 7 explicitly treats these as world-model entities, not merely transient frame annotations.

## Detector Families

Detector-family support is represented through `DetectorKind` and carried into the world model.

- `template_match`
- `color_cluster`
- `ocr`
- `object_detection`
- `memory_entity`
- `render_entity`

`WorldModel.detector_kinds` records which detector families contributed to the current belief state so traces and future debugging can reason about where a belief came from.

## Tracking

Stable tracking is now modeled explicitly.

- `TrackedObservationEntity` stores `first_seen_frame_id`
- `TrackedObservationEntity` stores `last_seen_frame_id`
- `TrackedObservationEntity` stores `missed_frames`
- `TrackedObservationEntity` stores `status`

This lets the system carry entities across frames, not just rediscover them from scratch every update.

## Confidence Smoothing

Confidence is now smoothed over time rather than replaced on every frame.

- `TrackedObservationEntity.smoothed_confidence` is updated by `WorldModelTracker`
- `WorldModelTrackerConfig.confidence_alpha` controls the blend

That means one noisy low-confidence frame does not immediately destroy a useful belief, which is especially important for visual-only detections.

## Lost And Reacquired State

Phase 7 now has explicit lost-target and reacquisition behavior.

- `TrackedEntityStatus` can be `active`, `lost`, or `reacquired`
- `WorldModelTrackerConfig.lost_after_missed_frames` determines when an entity becomes lost
- `WorldModelTrackerConfig.drop_after_missed_frames` determines when an entity is dropped entirely
- `WorldModelDelta` records which entities were lost or reacquired on a given update

This gives later navigation logic a clean place to implement recovery and reacquisition strategies.

## Prioritization

Object prioritization is now part of the shared perception layer.

- Each tracked entity carries a `priority_score`
- `WorldModel.prioritized_entities()` returns active entities in best-first order
- `WorldModel.best_entity(...)` provides a convenience selector by entity kind
- `winr-workflows::select_prioritized_entity_id(...)` shows how higher-level code can consume that ordering

The scoring is intentionally simple for now: entity type, tags, and smoothed confidence all contribute.

## Current Limits

Phase 7 still stops short of full multi-hypothesis tracking or spatial association.

- Matching currently uses stable entity ids rather than spatial nearest-neighbor logic
- No temporal motion model is implemented yet
- No occlusion handling is implemented yet
- No cross-source fusion policy beyond detector-family recording is implemented yet

That is expected for this phase. The important result is that the system now has a shared persistent world model and tracked entity layer, which later navigation and Roblox-specialization work can build on directly.
