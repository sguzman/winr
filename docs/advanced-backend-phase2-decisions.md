# Advanced Backend Phase 2 Decisions

This document records the concrete Phase 2 decisions for the advanced backend host-agent split and IPC boundary.

## Phase 2 Checklist

- [x] Define a host-agent architecture before adding injected logic to the project
- [x] Keep process discovery, policy enforcement, workflow execution, planning, retries, and fallback in the host
- [x] Keep low-level observations, low-level or semantic input hooks, internal capability reporting, and state streaming in the injected agent
- [x] Define IPC for command requests and responses
- [x] Define IPC for event streams and observation updates
- [x] Support binary payloads when frame transport is needed
- [x] Add health and version handshake behavior
- [x] Version the protocol from the first draft
- [x] Avoid assuming render hooks, memory readers, and input shims must always come from the same agent implementation

## Host Responsibilities

The host remains the control plane.

- Discovery and target selection stay in `winr-inject::discovery`.
- Attachment lifecycle, heartbeat tracking, and reattach policy stay in `winr-inject::attachment`.
- Session ownership and protocol sequencing stay in `AdvancedBackendSession`.
- Profile planning, workflow orchestration, retries, fallback, and safety policy remain host-side concerns.
- `AdvancedHostRuntime<TTransport>` is the concrete host runtime entry point for Phase 2.

This keeps the injected side narrow. The host decides what should happen and when. The agent reports what it can do and performs low-level work on request.

## Agent Responsibilities

The agent is the execution plane inside or beside the target process.

- Low-level input shims belong to the agent side.
- Render observation and memory-backed observation belong to the agent side.
- Internal capability reporting belongs to the agent side.
- Observation updates and future semantic adapters originate from the agent side.
- `AdvancedAgentRuntime` is the Phase 2 trait boundary for agent implementations.
- `StubAdvancedAgent` is the first protocol-valid stand-in used for tests and shape validation.

The agent is intentionally allowed to be compositional rather than monolithic. `AdvancedAgentComposition` advertises roles such as `input_shim`, `render_observer`, `memory_observer`, and `semantic_adapter` so the architecture does not assume one DLL or one hook must provide every capability.

## IPC Shape

Phase 2 now defines three protocol lanes.

1. Command lane
   `AdvancedHostCommandEnvelope` carries ordered host requests.
   `AdvancedHostResponseEnvelope` carries ordered responses and includes `response_to`.

2. Event lane
   `AdvancedAgentEventEnvelope` carries asynchronous status and observation events.

3. Binary payload lane
   `AdvancedBinaryPayloadRef` advertises externally transported bytes for cases like frame or tensor delivery.

The first transport abstraction is `AdvancedAgentTransport`.

- `send_command` pushes command requests.
- `recv_response` receives command responses.
- `recv_event` receives asynchronous agent events.
- `push_binary_payload` and `take_binary_payload` model out-of-band frame transport.

`InMemoryAgentTransport` is the first implementation. It is intentionally simple and exists to validate protocol boundaries before named pipes, shared memory, or other Windows-specific transports are introduced.

## Handshake And Capability Negotiation

The Phase 2 handshake builds on the Phase 0 and Phase 1 work rather than replacing it.

- `AdvancedBackendHello` now includes protocol version, lifecycle state, capabilities, target identity, transport descriptor, and agent composition.
- `AdvancedHostCommand::Handshake` remains the first host request.
- `AdvancedHostResponse::Hello` returns the normalized hello payload.
- `AdvancedHostCommand::GetCapabilities` and `AdvancedHostResponse::Capabilities` keep explicit capability negotiation separate from attachment and start/stop flow.

This means the host can attach, negotiate, inspect transport limits, and decide whether to proceed before real injected logic is introduced.

## Observation Updates

Observation transport is now represented explicitly by `AdvancedObservationUpdate`.

- It always includes `frame_id`, `source`, and `detail`.
- It can optionally include `AdvancedBinaryPayloadRef`.
- Synchronous pulls use `FetchObservations`.
- Asynchronous pushes use `AdvancedAgentEvent::ObservationTick`.

This gives us one shape that can later wrap desktop screenshots, render-hook frames, memory snapshots, detector overlays, or semantic state packets.

## Current Limits

Phase 2 intentionally stops short of real injection.

- No DLL injector is implemented yet.
- No named pipe or shared memory transport is implemented yet.
- No render hook, memory reader, or internal action shim is implemented yet.
- The agent and transport are still stubbed for protocol development.

That is expected. The boundary is now explicit enough to begin real injector work in later phases without collapsing host concerns and process-side concerns back together.
