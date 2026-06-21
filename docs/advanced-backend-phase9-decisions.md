# Advanced Backend Phase 9 Decisions

This document records the concrete Phase 9 decisions for the workflow DSL.

## Phase 9 Checklist

- [x] Design a richer workflow language beyond the current profile model
- [x] Support declarative detectors
- [x] Support action graphs
- [x] Support conditions
- [x] Support retries
- [x] Support branching
- [x] Support cooldowns
- [x] Support recovery steps
- [x] Support backend preferences
- [x] Support task concepts such as `search_for`, `approach`, `patrol_region`, `interact_until`, `wait_for_prompt`, `resume_previous_task`, and `recover_if_stuck`
- [x] Support target behaviors such as patrol within a detected dirt patch, approach a high-confidence rock, interact until completion, and return to patrol if the target disappears

## Core Decision

Phase 9 introduces a declarative workflow document model in `winr-workflows` instead of extending the old profile format indefinitely.

- `WorkflowDslDocument` is the top-level document
- `WorkflowTaskRecipe` is the task-level recipe
- `WorkflowNode` and `WorkflowStep` model action-graph execution

This creates a real DSL boundary without requiring a full runtime interpreter in the same phase.

## Declarative Detectors

Detector requirements are now first-class DSL nodes rather than only implicit code expectations.

- `DeclarativeDetector::template_match`
- `DeclarativeDetector::color_cluster`
- `DeclarativeDetector::ocr`
- `DeclarativeDetector::object_detection`
- `DeclarativeDetector::memory_entity`
- `DeclarativeDetector::render_entity`

Each detector declaration includes both the detector id and the entity kind it is expected to produce.

## Action Graphs

Task behavior is now modeled as a graph rather than a flat list.

- `WorkflowNode` has an id, kind, steps, retry policy, cooldown, and next-node links
- `WorkflowNodeKind` includes `detect`, `act`, `branch`, `wait`, `recover`, and `complete`
- `next_nodes(...)` provides a simple graph traversal helper

This is the shape needed for richer control flow without forcing everything into one linear script.

## Conditions, Retries, And Cooldowns

The DSL now includes the control-flow primitives we were missing.

- `WorkflowCondition`
- `WorkflowConditionOperator`
- `WorkflowRetryPolicy`
- `WorkflowCooldown`

These allow a recipe to say things like "wait until a prompt exists", "retry detection three times", or "cool down before polling again" without hardcoding that logic into controller code.

## Recovery Steps

Recovery is now part of the DSL instead of only the navigation-controller layer.

- `WorkflowRecoveryStep::retry_current_node`
- `WorkflowRecoveryStep::run_controller`
- `WorkflowRecoveryStep::emit_action`
- `WorkflowRecoveryStep::resume_previous_task`

That gives task authors an explicit place to express what should happen when a step fails or a target disappears.

## Backend Preferences

Task-level backend preference is now part of the DSL.

- `WorkflowBackendPreference` records ordered backend choices
- `WorkflowTaskRecipe.backend_preference` carries that preference into each recipe

This keeps backend selection visible at the workflow layer without mixing it into low-level controller code.

## Compilation Boundary

Phase 9 deliberately includes a lightweight compile step instead of a full executor.

- `WorkflowTaskRecipe::compile_plan()` turns a declarative recipe into the existing `WorkflowPlan`
- `WorkflowTaskRecipe::evaluate_conditions(...)` lets the DSL query the current `WorldModel`
- `workflow_intent_kind_for_action(...)` bridges semantic actions into existing workflow intent kinds

This preserves compatibility with the current planning surface while still moving us toward DSL-native workflows.

## Current Limits

Phase 9 still stops short of a full persistent runtime.

- No interpreter loop with node execution state is implemented yet
- No serialization loader or CLI runner for DSL documents is implemented yet
- No graph-level variable system is implemented yet

That is expected for this phase. The important result is that the project now has a real declarative workflow language shape with detectors, action graphs, conditions, retries, cooldowns, recovery, backend preference, and task concepts that match the behaviors we want to automate.
