# winr Advanced Backend Roadmap

This document sketches the long-term roadmap for a higher-capability automation backend aimed at custom-rendered applications, games, and other targets that do not cooperate with standard Win32 foreground input, `message` input, or UI Automation.

The immediate motivating use case is Roblox, but the backend should be designed as a general system rather than a Roblox-only fork.

## Goal

Build an advanced backend that can:

- observe application state beyond ordinary desktop screenshots
- drive input without depending exclusively on foreground `SendInput`
- support higher-level workflows such as movement, approach, patrol, and interaction
- stay general enough to support other custom-rendered apps and games later

This backend should live alongside the current `winr-core` automation model, not replace it.

## Non-goals

- do not collapse the whole project into a Roblox-specific codebase
- do not make Direct3D hooking the only observation path
- do not tie workflow logic directly to raw pixels when stronger signals exist
- do not merge advanced process-side techniques into the same safety assumptions as normal Win32 automation

## Target architecture

The long-term shape should separate the system into distinct layers:

1. `winr-core`
   Current desktop automation backend for windows, screenshots, foreground input, message input, and UI Automation.

2. `winr-inject`
   Process-side advanced backend for applications that need injection, render observation, or internal input shims.

3. `winr-perception`
   Shared state and vision layer that can consume screenshots, render-hook frames, memory-backed signals, and detectors.

4. `winr-workflows`
   High-level task and behavior engine for goals such as:
   - move around a bounded region
   - approach a detected object
   - patrol an area
   - react when a target appears

5. app-specific backend packs
   Per-target specializations such as Roblox-specific detectors, movement tuning, and action recipes.

## Core design principles

- Separate rendering, state-reading, and input into different subsystems.
- Prefer capability detection over app-specific assumptions.
- Represent automation in terms of intents and observations, not only raw key and mouse events.
- Keep app-specific behavior in adapters or packs instead of in the generic engine.
- Treat every low-level hook as fragile and replaceable.
- Build for traceability so behavior can be debugged after the fact.

## Capability model

The advanced backend should expose explicit capabilities instead of pretending all targets behave the same.

Candidate capabilities:

- `foreground_input`
- `message_input`
- `uia_input`
- `injected_input`
- `render_observation`
- `memory_observation`
- `semantic_navigation`
- `entity_tracking`
- `internal_interaction`

Backends should advertise which of these they support so workflows can choose the strongest available path.

## Phase 0: Contracts and boundaries

Before any injection work starts, define the interfaces that the rest of `winr` will rely on.

Deliverables:

- backend trait model for observation, input, and workflow execution
- shared target identity model that ties together HWND, PID, executable name, and backend attachment
- explicit lifecycle states such as discovered, attachable, attached, degraded, and detached
- structured error families for advanced backend failures
- capability negotiation model for backend selection

Questions to settle:

- which APIs stay in `winr-core` versus moving to a new crate
- how CLI and MCP choose between ordinary and advanced backends
- whether advanced backend support is opt-in per profile, per target, or auto-detected

## Phase 1: Process discovery and attachment

Build a robust target attachment layer that can discover and track a custom-rendered application over time.

Deliverables:

- target discovery by PID, executable name, HWND, and window class chain
- process metadata capture:
  - architecture and bitness
  - loaded modules
  - integrity level
  - foreground and visibility state
  - likely rendering window
- attach and detach lifecycle
- restart detection and reattachment policy
- heartbeat and health-check plumbing

Requirements:

- attachment should survive target relaunches when possible
- attachment failures should be observable and logged with concrete reasons
- multiple candidate processes should be handled deterministically

## Phase 2: Host-agent split and IPC

Do not start by scattering injected logic directly through the current crates. Define a host-agent architecture first.

Host responsibilities:

- process discovery
- policy enforcement
- profile and workflow execution
- high-level planning
- retries, backoff, and fallback

Injected agent responsibilities:

- expose low-level observations
- expose low-level or semantic input hooks
- report internal capabilities and health
- stream state updates to the host

IPC requirements:

- command requests and responses
- event stream for observation updates
- health and version handshake
- support for binary payloads when frame transport is needed
- protocol versioning from the first draft

Important constraint:

- the host should not assume rendering hooks, memory readers, and input shims all come from the same agent implementation forever

## Phase 3: Observation stack

The advanced backend should support multiple observation sources and normalize them into one internal frame model.

Observation sources:

- desktop screenshot mode
- render-hook frame mode
- memory-backed state mode
- optional OCR and detector overlays

Normalized observation frame fields:

- timestamp
- target identity
- backend source
- image or frame handle
- camera hints
- player-state hints
- detected entities
- confidence metrics
- freshness and staleness markers

Design goal:

- workflows should consume normalized observations without caring whether they came from screenshots, render hooks, or memory-backed state

## Phase 4: Render observation

Render observation should be treated as one observation backend, not the entire system.

Likely responsibilities:

- hook frame presentation or a similar rendering boundary
- extract frame timing and frame availability
- expose sampled image data or analysis hooks
- support overlay/debug visualization in development builds

What it is good for:

- seeing what the user sees
- template or object detection on in-process frames
- correlating visual state changes with actions

What it is not by itself:

- a supported game-state API
- a true background input channel
- a general-purpose semantic model of the world

## Phase 5: Memory and internal state observation

If a target needs richer automation than visual heuristics alone, build a separate state-reading layer.

Potential signals:

- player position
- camera orientation
- current movement state
- active tool or mode
- nearby interactables
- UI prompt presence
- object or node lists

Design requirements:

- keep this source optional
- version observations so changes can be detected across target updates
- never make workflow code depend directly on raw offsets or raw memory layouts
- translate low-level state into stable internal DTOs first

## Phase 6: Input stack

Input for advanced targets should be abstracted into layers, from weakest to strongest:

- Win32 foreground input
- message-based background input
- injected raw input shim
- semantic internal action calls

The workflow engine should prefer semantic actions over raw input spam when available.

Candidate semantic actions:

- `move_forward(duration)`
- `move_backward(duration)`
- `strafe_left(duration)`
- `strafe_right(duration)`
- `turn(delta_yaw)`
- `look_pitch(delta_pitch)`
- `jump()`
- `interact()`
- `hold(action, until)`
- `stop_motion()`
- `approach(target)`
- `walk_to(region_or_entity)`

This is the key shift from the current clicker model:

- profiles and workflows should eventually ask for intent
- backends should decide how that intent maps to raw keys, mouse deltas, or internal actions

## Phase 7: Perception and entity model

Create a shared world model that can represent what the automation system believes about the target.

Core entities:

- player
- camera
- region
- prompt
- interactable object
- collectible object
- obstacle
- waypoint
- detected visual marker

Supported detector families:

- template matching
- color cluster matching
- OCR
- object detection
- memory-backed entity extraction
- render-backed overlay or object extraction

Required behavior:

- stable tracking across frames
- confidence smoothing
- lost target handling
- reacquisition logic
- object prioritization

## Phase 8: Navigation and control

To support tasks like “move around this small area outlined by dirt” or “whenever you see this rock, walk up to it,” `winr` needs control logic, not just input emission.

Controller responsibilities:

- heading control
- movement correction
- arrival detection
- stuck detection
- obstacle or failure recovery
- action cancellation

Required reusable controllers:

- rotate toward target
- approach target until distance or confidence threshold
- follow local waypoint sequence
- patrol inside a bounded region
- recover from no-progress states

Example workflow families:

- patrol region while scanning for objects
- approach object when confidence exceeds threshold
- interact when a prompt or interaction state is available
- resume patrol after interaction completes

## Phase 9: Workflow DSL

The current profile model is a good starting point, but advanced targets need a richer workflow language.

The next-generation workflow DSL should support:

- declarative detectors
- action graphs
- conditions
- retries
- branching
- cooldowns
- recovery steps
- backend preferences

Candidate task concepts:

- `search_for`
- `approach`
- `patrol_region`
- `interact_until`
- `wait_for_prompt`
- `resume_previous_task`
- `recover_if_stuck`

Example target behaviors:

- patrol within the detected dirt patch
- when a rock is detected with high confidence, approach it
- interact until the completion condition is true
- return to patrol if the rock disappears or the interaction completes

## Phase 10: Roblox specialization

Roblox is the main motivating use case, but it should be treated as a specialization on top of the generic advanced backend.

Roblox-specific modules may eventually include:

- detector packs for common resource nodes, prompts, and regions
- movement tuning for Roblox-style camera and locomotion
- task recipes for harvesting, patrolling, or object approach
- profile presets for Roblox workflows

Important rule:

- the generic workflow engine should not depend on Roblox names, assumptions, or object categories

## Phase 11: Reliability and observability

The advanced backend must be much more observable than the current simple input flows.

Needed tooling:

- replayable traces of observations and actions
- structured event logs
- backend health dashboards or summaries
- execution reasoning for “why did it do that”
- stale-state detection
- frame freshness tracking
- command acknowledgment tracking

This is necessary because injected or app-specific backends will fail in more subtle ways than ordinary Win32 automation.

## Phase 12: User-facing integration

The user-facing surface should stay coherent even as backends become more sophisticated.

Desired outcome:

- one CLI
- one MCP surface
- one workflow concept
- backend-specific execution hidden behind capability selection

Profiles should eventually be able to declare backend preferences such as:

- `backend = "foreground"`
- `backend = "message"`
- `backend = "inject"`
- `backend = "auto"`

## Suggested implementation order

To avoid building a one-off Roblox hack, the early implementation order should be:

1. define advanced backend traits and lifecycle contracts
2. define host-agent split and protocol
3. define normalized observation frame model
4. define semantic input action model
5. design workflow DSL v2
6. build simple navigation controllers
7. add app-specific packs starting with Roblox

## Success criteria

Short-term success:

- advanced backend can attach to a target and report capabilities
- observation and input paths are separated cleanly
- workflows can choose backend preferences

Medium-term success:

- `winr` can run “see target -> approach -> interact” workflows
- region patrol behavior works
- workflows recover from lost targets or stuck movement

Long-term success:

- the same high-level workflow model can target classic windows, custom-rendered apps, and games
- Roblox-specific logic stays in specialization layers instead of dominating the core architecture
