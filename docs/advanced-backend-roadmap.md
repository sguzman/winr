# winr Advanced Backend Roadmap

This document tracks the long-term roadmap for a higher-capability automation backend aimed at custom-rendered applications, games, and other targets that do not cooperate with standard Win32 foreground input, `message` input, or UI Automation.

The immediate motivating use case is Roblox, but the backend should be designed as a general system rather than a Roblox-only fork.

## Goals

- [ ] Observe application state beyond ordinary desktop screenshots
- [ ] Drive input without depending exclusively on foreground `SendInput`
- [ ] Support higher-level workflows such as movement, approach, patrol, and interaction
- [ ] Keep the backend general enough to support other custom-rendered apps and games later
- [ ] Keep the advanced backend alongside the current `winr-core` automation model rather than replacing it

## Non-goals

- [ ] Do not collapse the whole project into a Roblox-specific codebase
- [ ] Do not make Direct3D hooking the only observation path
- [ ] Do not tie workflow logic directly to raw pixels when stronger signals exist
- [ ] Do not merge advanced process-side techniques into the same safety assumptions as normal Win32 automation

## Target Architecture

- [x] Preserve `winr-core` as the standard desktop automation backend for windows, screenshots, foreground input, message input, and UI Automation
- [x] Add a `winr-inject` crate for process-side advanced backend work such as injection, render observation, or internal input shims
- [x] Add a `winr-perception` crate for shared state and vision that can consume screenshots, render-hook frames, memory-backed signals, and detectors
- [x] Add a `winr-workflows` crate for higher-level task and behavior execution
- [x] Support app-specific backend packs for targets such as Roblox
- [x] Keep app-specific logic out of the generic workflow engine

See `docs/advanced-backend-architecture.md` for the current crate layout and `packs/` structure.

## Core Principles

- [x] Separate rendering, state-reading, and input into different subsystems
- [x] Prefer capability detection over app-specific assumptions
- [x] Represent automation in terms of intents and observations, not only raw key and mouse events
- [x] Keep app-specific behavior in adapters or packs instead of in the generic engine
- [x] Treat every low-level hook as fragile and replaceable
- [x] Build for traceability so behavior can be debugged after the fact

See `docs/advanced-backend-principles.md` for the concrete mapping from principles to code.

## Capability Model

- [x] Define an explicit capability model instead of pretending all targets behave the same
- [x] Support capability flags such as `foreground_input`, `message_input`, `uia_input`, `injected_input`, `render_observation`, `memory_observation`, `semantic_navigation`, `entity_tracking`, and `internal_interaction`
- [x] Make backends advertise supported capabilities so workflows can choose the strongest available path

See `docs/advanced-backend-capability-model.md` for the current capability vocabulary, descriptor format, and selection rules.

## Phase 0: Contracts And Boundaries

- [x] Define backend traits for observation, input, and workflow execution
- [x] Define a shared target identity model that ties together HWND, PID, executable name, and backend attachment
- [x] Define explicit lifecycle states such as discovered, attachable, attached, degraded, and detached
- [x] Define structured error families for advanced backend failures
- [x] Define capability negotiation for backend selection
- [x] Decide which APIs stay in `winr-core` versus moving to a new crate
- [x] Decide how CLI and MCP choose between ordinary and advanced backends
- [x] Decide whether advanced backend support is opt-in per profile, per target, or auto-detected

See `docs/advanced-backend-phase0-decisions.md` for the concrete decisions recorded for this phase.

## Phase 1: Process Discovery And Attachment

- [x] Build target discovery by PID, executable name, HWND, and window class chain
- [ ] Capture process metadata including architecture, bitness, loaded modules, integrity level, foreground state, visibility state, and likely rendering window
- [ ] Implement attach and detach lifecycle handling
- [ ] Implement restart detection and reattachment policy
- [ ] Add heartbeat and health-check plumbing
- [ ] Ensure attachment can survive target relaunches when possible
- [ ] Ensure attachment failures are observable and logged with concrete reasons
- [ ] Handle multiple candidate processes deterministically

## Phase 2: Host-Agent Split And IPC

- [ ] Define a host-agent architecture before adding injected logic to the project
- [ ] Keep process discovery, policy enforcement, workflow execution, planning, retries, and fallback in the host
- [ ] Keep low-level observations, low-level or semantic input hooks, internal capability reporting, and state streaming in the injected agent
- [ ] Define IPC for command requests and responses
- [ ] Define IPC for event streams and observation updates
- [x] Add health and version handshake behavior
- [ ] Support binary payloads when frame transport is needed
- [x] Version the protocol from the first draft
- [ ] Avoid assuming render hooks, memory readers, and input shims must always come from the same agent implementation

## Phase 3: Observation Stack

- [ ] Support desktop screenshot mode as one observation source
- [ ] Support render-hook frame mode as one observation source
- [ ] Support memory-backed state mode as one observation source
- [ ] Support optional OCR and detector overlays
- [ ] Normalize all observation sources into one internal observation frame model
- [ ] Include timestamp, target identity, backend source, image or frame handle, camera hints, player-state hints, detected entities, confidence metrics, and freshness markers in that model
- [ ] Keep workflows independent from the specific observation source that produced the frame

## Phase 4: Render Observation

- [ ] Treat render observation as one backend, not as the entire advanced architecture
- [ ] Hook frame presentation or a similar rendering boundary
- [ ] Extract frame timing and frame availability
- [ ] Expose sampled image data or analysis hooks
- [ ] Support overlay or debug visualization in development builds
- [ ] Use render observation for visible-scene understanding, template or object detection, and action correlation
- [ ] Avoid treating render observation as a supported game-state API
- [ ] Avoid treating render observation as a true background input channel

## Phase 5: Memory And Internal State Observation

- [ ] Build a separate state-reading layer for targets that need richer automation than visual heuristics alone
- [ ] Support potential signals such as player position, camera orientation, movement state, active tool or mode, nearby interactables, prompt state, and object lists
- [ ] Keep memory-backed state optional
- [ ] Version observations so changes can be detected across target updates
- [ ] Prevent workflow code from depending directly on raw offsets or raw memory layouts
- [ ] Translate low-level state into stable internal DTOs first

## Phase 6: Input Stack

- [ ] Define layered input sinks including Win32 foreground input, message-based background input, injected raw input shim, and semantic internal action calls
- [ ] Make the workflow engine prefer semantic actions over raw input spam when available
- [ ] Define candidate semantic actions such as `move_forward(duration)`, `move_backward(duration)`, `strafe_left(duration)`, `strafe_right(duration)`, `turn(delta_yaw)`, `look_pitch(delta_pitch)`, `jump()`, `interact()`, `hold(action, until)`, `stop_motion()`, `approach(target)`, and `walk_to(region_or_entity)`
- [ ] Shift profiles and workflows toward intent instead of raw keystroke loops
- [ ] Let each backend decide how intents map to raw keys, mouse deltas, or internal actions

## Phase 7: Perception And Entity Model

- [ ] Create a shared world model that represents what the automation system believes about the target
- [ ] Support core entities such as player, camera, region, prompt, interactable object, collectible object, obstacle, waypoint, and detected visual marker
- [ ] Support detector families including template matching, color cluster matching, OCR, object detection, memory-backed entity extraction, and render-backed overlay or object extraction
- [ ] Add stable tracking across frames
- [ ] Add confidence smoothing
- [ ] Add lost target handling
- [ ] Add reacquisition logic
- [ ] Add object prioritization

## Phase 8: Navigation And Control

- [ ] Build control logic for tasks like "move around this small area outlined by dirt" or "whenever you see this rock, walk up to it"
- [ ] Implement heading control
- [ ] Implement movement correction
- [ ] Implement arrival detection
- [ ] Implement stuck detection
- [ ] Implement obstacle or failure recovery
- [ ] Implement action cancellation
- [ ] Build reusable controllers for rotate-toward-target, approach-until-threshold, local waypoint following, bounded-region patrol, and no-progress recovery
- [ ] Support workflow families such as patrol-while-scanning, approach-when-confidence-exceeds-threshold, interact-when-prompt-appears, and resume-patrol-after-interaction

## Phase 9: Workflow DSL

- [ ] Design a richer workflow language beyond the current profile model
- [ ] Support declarative detectors
- [ ] Support action graphs
- [ ] Support conditions
- [ ] Support retries
- [ ] Support branching
- [ ] Support cooldowns
- [ ] Support recovery steps
- [ ] Support backend preferences
- [ ] Support task concepts such as `search_for`, `approach`, `patrol_region`, `interact_until`, `wait_for_prompt`, `resume_previous_task`, and `recover_if_stuck`
- [ ] Support target behaviors such as patrol within a detected dirt patch, approach a high-confidence rock, interact until completion, and return to patrol if the target disappears

## Phase 10: Roblox Specialization

- [ ] Treat Roblox as a specialization on top of the generic advanced backend
- [ ] Add Roblox-specific detector packs for common resource nodes, prompts, and regions
- [ ] Add Roblox-specific movement tuning
- [ ] Add Roblox-specific task recipes for harvesting, patrolling, or object approach
- [ ] Add Roblox-specific profile presets for workflows
- [ ] Prevent the generic workflow engine from depending on Roblox names, assumptions, or object categories

## Phase 11: Reliability And Observability

- [ ] Add replayable traces of observations and actions
- [ ] Add structured event logs
- [ ] Add backend health summaries or dashboards
- [ ] Add execution reasoning so operators can answer "why did it do that"
- [ ] Add stale-state detection
- [ ] Add frame freshness tracking
- [ ] Add command acknowledgment tracking
- [ ] Treat observability as a first-class requirement because injected and app-specific backends fail in subtler ways than ordinary Win32 automation

## Phase 12: User-Facing Integration

- [ ] Keep one CLI surface
- [ ] Keep one MCP surface
- [ ] Keep one workflow concept across backends
- [ ] Hide backend-specific execution behind capability selection
- [x] Let profiles eventually declare backend preferences such as `foreground`, `message`, `inject`, or `auto`

## Suggested Implementation Order

- [x] Define advanced backend traits and lifecycle contracts
- [x] Define host-agent split and protocol
- [ ] Define normalized observation frame model
- [ ] Define semantic input action model
- [ ] Design workflow DSL v2
- [ ] Build simple navigation controllers
- [ ] Add app-specific packs starting with Roblox

## Success Criteria

### Short-term

- [ ] Advanced backend can attach to a target and report capabilities
- [ ] Observation and input paths are separated cleanly
- [ ] Workflows can choose backend preferences

### Medium-term

- [ ] `winr` can run "see target -> approach -> interact" workflows
- [ ] Region patrol behavior works
- [ ] Workflows recover from lost targets or stuck movement

### Long-term

- [ ] The same high-level workflow model can target classic windows, custom-rendered apps, and games
- [ ] Roblox-specific logic stays in specialization layers instead of dominating the core architecture
