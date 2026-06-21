# Advanced Backend Phase 12 Decisions

This document records the concrete Phase 12 decisions for user-facing integration.

## Phase 12 Checklist

- [x] Keep one CLI surface
- [x] Keep one MCP surface
- [x] Keep one workflow concept across backends
- [x] Hide backend-specific execution behind capability selection
- [x] Let profiles eventually declare backend preferences such as `foreground`, `message`, `inject`, or `auto`

## Core Decision

Phase 12 makes frontends consume one shared workflow-integration path instead of each frontend inventing its own backend logic.

- `winr-core::describe_profile_workflow(...)` is the shared inspection entry point
- `winr-core::run_profile_for_frontend(...)` is the shared execution entry point
- CLI and MCP now call those shared functions instead of duplicating backend-selection decisions

This keeps the user-facing product surface coherent even while the backend system underneath becomes more capable.

## One CLI Surface

The CLI stays under the existing `winr` command tree.

- `profile run` remains the execution command
- `profile inspect` now exposes the resolved workflow and backend plan
- both commands operate on the same profile model and the same backend-selection rules

That means we did not introduce a separate "advanced backend CLI" or a parallel workflow command family.

## One MCP Surface

The MCP server now exposes the same workflow concept through MCP tools.

- `profile_inspect` returns the same shared integration description that the CLI uses
- `profile_run` runs the same shared profile workflow path for the MCP frontend

This keeps MCP as one surface instead of creating a disconnected advanced-only tool namespace.

## One Workflow Concept

The workflow concept exposed to users remains the profile workflow.

- `ProfileWorkflowIntegration` describes a profile as `workflow_surface = "profile_v1"`
- the same profile file can be inspected or run from either CLI or MCP
- backend differences are described as selection metadata, not as different user-level workflow models

That preserves a stable mental model while still allowing backend sophistication to grow underneath it.

## Hidden Backend-Specific Execution

Backend-specific execution is now hidden behind shared capability selection.

- `resolve_backend_selection(...)` remains the selection entry point
- `describe_profile_workflow(...)` exposes the selection and capability outcome without exposing separate frontend logic
- `run_profile_for_frontend(...)` executes using the resolved backend for the active frontend

From the user perspective, they ask to run or inspect a profile workflow. The system decides how to satisfy it.

## Runtime Integration Cleanup

Phase 12 also closes one host/runtime integration gap.

- `AdvancedBackendSession::command(...)` is now the common command-creation path
- `AdvancedHostRuntime` now routes responses through `session.apply_response(...)`

That keeps the user-facing integration honest by ensuring the host runtime participates in the same observability and acknowledgment system introduced in Phase 11.

## Current Limit

Phase 12 still stops short of a full workflow DSL runner exposed directly to CLI and MCP.

- the shared user-facing workflow concept is still profile-centric
- app-pack and DSL-native execution are not yet a standalone surface
- CLI and MCP do not yet expose every advanced-backend observability DTO as dedicated commands or tools

That is acceptable for this phase. The important result is that `winr` now presents one CLI surface, one MCP surface, one workflow concept, and one shared backend-selection path across those frontends.
