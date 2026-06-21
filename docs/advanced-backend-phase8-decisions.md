# Advanced Backend Phase 8 Decisions

This document records the concrete Phase 8 decisions for navigation and control.

## Phase 8 Checklist

- [x] Build control logic for tasks like "move around this small area outlined by dirt" or "whenever you see this rock, walk up to it"
- [x] Implement heading control
- [x] Implement movement correction
- [x] Implement arrival detection
- [x] Implement stuck detection
- [x] Implement obstacle or failure recovery
- [x] Implement action cancellation
- [x] Build reusable controllers for rotate-toward-target, approach-until-threshold, local waypoint following, bounded-region patrol, and no-progress recovery
- [x] Support workflow families such as patrol-while-scanning, approach-when-confidence-exceeds-threshold, interact-when-prompt-appears, and resume-patrol-after-interaction

## Core Decision

Phase 8 puts navigation logic in `winr-workflows` on top of the world model and semantic input stack.

- Controllers read `WorldModel`
- Controllers emit `SemanticInputAction`
- Controllers do not talk directly in raw keys or mouse deltas

This keeps control logic source-agnostic and backend-agnostic while still making movement behavior concrete.

## Controller Set

Phase 8 introduces explicit reusable controller families.

- `RotateTowardTargetController`
- `ApproachUntilThresholdController`
- `BoundedRegionPatrolController`
- `NoProgressRecoveryController`

These implement the reusable control shapes we identified earlier rather than hardcoding a Roblox-only flow.

## Navigation Context

Controllers operate on `NavigationContext`.

- It carries the current `WorldModel`
- It carries the current frame id
- It carries `ControllerMemory` for short-horizon progress tracking

That gives us a clean place for controllers to reason about both the current scene and recent motion history.

## Decision Model

Controllers now return explicit decisions.

- `NavigationDecisionKind::continue`
- `NavigationDecisionKind::arrived`
- `NavigationDecisionKind::recovering`
- `NavigationDecisionKind::blocked`
- `NavigationDecisionKind::cancelled`

`NavigationDecision` includes both the decision kind and the semantic actions that should follow from it.

This makes controller reasoning inspectable and keeps action cancellation or recovery from being implicit.

## Heading, Arrival, And Correction

Phase 8 now has first-class hooks for the basic control loops.

- Heading control uses camera and target yaw notes in the world model
- Arrival detection uses configurable distance thresholds
- Movement correction and approach behavior are represented by explicit semantic actions

The current implementation is intentionally simple, but the control surfaces are now explicit enough to tune later.

## Stuck Detection And Recovery

No-progress recovery is now a dedicated controller instead of an afterthought.

- `ControllerMemory` stores progress samples
- `is_stuck(...)` checks recent target-distance change over a configurable window
- `NoProgressRecoveryController` emits a recovery sequence of `stop_motion`, `strafe_right`, and `jump`

This gives later phases a clean foundation for richer obstacle handling and failure recovery.

## Workflow Families

Phase 8 includes concrete helper flows that show how controllers combine into behaviors.

- `patrol_while_scanning_decision(...)`
- `interact_when_prompt_appears_decision(...)`
- `resume_patrol_after_interaction_decision(...)`

These helpers are intentionally lightweight, but they show the exact family of behavior we want: patrol until a target appears, approach or interact when confidence is good enough, and return to patrol afterward.

## Current Limits

Phase 8 still stops short of physically accurate navigation.

- No real pathfinding is implemented yet
- No collision geometry model is implemented yet
- No camera-to-world geometry solver is implemented yet
- No waypoint graph or terrain model is implemented yet

That is expected for this phase. The important result is that the project now has reusable control primitives and workflow-family helpers that can drive the next DSL and specialization phases.
