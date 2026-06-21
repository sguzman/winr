# Advanced Backend Phase 6 Decisions

This document records the concrete Phase 6 decisions for the input stack.

## Phase 6 Checklist

- [x] Define layered input sinks including Win32 foreground input, message-based background input, injected raw input shim, and semantic internal action calls
- [x] Make the workflow engine prefer semantic actions over raw input spam when available
- [x] Define candidate semantic actions such as `move_forward(duration)`, `move_backward(duration)`, `strafe_left(duration)`, `strafe_right(duration)`, `turn(delta_yaw)`, `look_pitch(delta_pitch)`, `jump()`, `interact()`, `hold(action, until)`, `stop_motion()`, `approach(target)`, and `walk_to(region_or_entity)`
- [x] Shift profiles and workflows toward intent instead of raw keystroke loops
- [x] Let each backend decide how intents map to raw keys, mouse deltas, or internal actions

## Core Decision

The input stack is now modeled as intent-first rather than key-first.

- `winr-workflows` owns the semantic input action model.
- `winr-inject` owns backend-specific input sink selection and realization.
- Workflows express what they want done, and backends decide how to execute that request.

This keeps planner logic from depending on Win32 messages, keycodes, or per-target raw input sequences.

## Layered Sinks

Phase 6 now defines four explicit sink families.

- `win32_foreground`
- `win32_message`
- `injected_raw_input`
- `semantic_internal_action`

These are represented by `InputSinkKind` and selected through `preferred_input_sink(...)` in `winr-workflows`.

The layered model keeps classic desktop input available while allowing injected and semantic paths to become first-class options.

## Semantic Actions

Phase 6 adds a normalized semantic action vocabulary in `winr-workflows`.

- `SemanticInputAction::MoveForward`
- `SemanticInputAction::MoveBackward`
- `SemanticInputAction::StrafeLeft`
- `SemanticInputAction::StrafeRight`
- `SemanticInputAction::Turn`
- `SemanticInputAction::LookPitch`
- `SemanticInputAction::Jump`
- `SemanticInputAction::Interact`
- `SemanticInputAction::Hold`
- `SemanticInputAction::StopMotion`
- `SemanticInputAction::Approach`
- `SemanticInputAction::WalkTo`

Targets are expressed through `SemanticInputTarget`, which keeps actions pointed at the current target, an entity id, or a region id.

## Workflow Preference

Workflows can now express input preference instead of only action intent.

- `WorkflowIntentDefinition` can carry a `semantic_action`
- `WorkflowIntentDefinition` can carry a `sink_preference`
- `WorkflowPlan::resolve_input_plan(...)` translates planner intents into a `WorkflowInputPlan`

This is the first concrete shift away from raw repeated mouse or keyboard loops as the primary workflow representation.

## Backend Mapping

The backend now decides how a semantic action should be executed.

- `LayeredInputBackend` is the process-side contract in `winr-inject`
- `StubLayeredInputBackend` is the first protocol-valid stand-in
- The backend advertises supported sinks from capability flags
- The backend resolves action-to-sink mappings and explains why a mapping was chosen

This preserves the important separation: workflows describe intent, but backends own execution details.

## Preference Rules

The default preference logic now favors stronger abstractions when the backend supports them.

- Semantic navigation actions prefer `semantic_internal_action`
- Other actions can fall back to `injected_raw_input`
- Simple actions like `interact`, `hold`, `jump`, and `stop_motion` can still map to `win32_message`
- `win32_foreground` remains a final compatibility fallback

That gives us an upgrade path from raw input to stronger backend-specific actions without breaking the overall workflow model.

## Current Limits

Phase 6 still stops short of real action execution.

- No real injected raw input shim is implemented yet
- No real semantic controller is implemented yet
- No translation to concrete key or mouse sequences is implemented yet beyond mapping decisions

That is expected for this phase. The key result is that the input stack is now explicit, layered, and intent-driven, which gives later navigation and Roblox-specialization work a stable control surface.
