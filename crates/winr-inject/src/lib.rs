mod attachment;
mod agent;
mod discovery;
mod host;
mod render;
mod transport;

use tracing::{debug, instrument};
use winr_types::{
    AdvancedAgentComposition, AdvancedAgentEvent, AdvancedAgentEventEnvelope, AdvancedAgentRole,
    AdvancedBackendCapabilities, AdvancedBackendDescriptor, AdvancedBackendError,
    AdvancedBackendErrorKind, AdvancedBackendHello, AdvancedBackendLifecycleState,
    AdvancedBackendOptInMode, AdvancedBackendSelection, AdvancedBackendSelectionReason,
    AdvancedBackendStability, AdvancedCapabilityCatalog, AdvancedCapabilityMatch,
    AdvancedCapabilityRequirements, AdvancedCapabilitySelection, AdvancedFrontend,
    AdvancedHostCommand, AdvancedHostCommandEnvelope, AdvancedIpcTransportDescriptor,
    AdvancedIpcTransportKind, AdvancedProfileBackend,
    AdvancedProfileExecutionPlan, AdvancedSequenceNumber, AdvancedSessionId, MouseInputMode,
    ProfileConfig, WindowSelector, WinrError, WinrResult,
};

pub use attachment::AttachmentSupervisor;
pub use agent::{AdvancedAgentRuntime, StubAdvancedAgent};
pub use discovery::{discover_attachable_targets, resolve_attachable_target};
pub use host::AdvancedHostRuntime;
pub use render::{RenderObservationBackend, StubRenderObserver};
pub use transport::{AdvancedAgentTransport, InMemoryAgentTransport};

pub trait AdvancedObservationBackend {
    fn descriptor(&self) -> AdvancedBackendDescriptor;
    fn discover_targets(
        &self,
        selector: &WindowSelector,
    ) -> WinrResult<winr_types::AdvancedTargetDiscovery>;
}

pub trait AdvancedInputBackend {
    fn descriptor(&self) -> AdvancedBackendDescriptor;
    fn prepare_session(&self, profile: &ProfileConfig) -> WinrResult<AdvancedBackendSession>;
}

pub trait AdvancedWorkflowBackend: AdvancedObservationBackend + AdvancedInputBackend {
    fn build_execution_plan(&self, profile: &ProfileConfig) -> AdvancedProfileExecutionPlan;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StubAdvancedBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedBackendSession {
    pub session_id: AdvancedSessionId,
    pub hello: AdvancedBackendHello,
    pub next_host_sequence: AdvancedSequenceNumber,
    pub last_agent_sequence: Option<AdvancedSequenceNumber>,
}

#[instrument(skip(profile))]
pub fn prepare_profile_backend(profile: &ProfileConfig) -> WinrResult<AdvancedBackendSession> {
    debug!(
        profile_id = %profile.profile.id,
        backend = profile.execution.backend.as_str(),
        "preparing advanced backend session"
    );

    let selection = resolve_backend_selection(profile, AdvancedFrontend::Cli);
    if selection.resolved != AdvancedProfileBackend::Inject {
        return Err(advanced_error(
            AdvancedBackendErrorKind::AttachNotImplemented,
            selection.resolved,
            format!(
                "advanced backend preparation is only valid for resolved backend '{}'",
                AdvancedProfileBackend::Inject.as_str()
            ),
            None,
        ));
    }

    let discovery = discover_profile_targets(profile).map_err(|error| {
        advanced_error(
            AdvancedBackendErrorKind::DiscoveryFailed,
            AdvancedProfileBackend::Inject,
            error.to_string(),
            None,
        )
    })?;
    let candidate = resolve_attachable_target(&discovery).map_err(|error| match error {
        WinrError::WindowNotFound => advanced_error(
            AdvancedBackendErrorKind::NoAttachableTarget,
            AdvancedProfileBackend::Inject,
            "no attachable targets matched the advanced backend selector".to_string(),
            None,
        ),
        other => advanced_error(
            AdvancedBackendErrorKind::AmbiguousAttachableTarget,
            AdvancedProfileBackend::Inject,
            other.to_string(),
            None,
        ),
    })?;
    let mut hello = stub_hello(profile);
    hello.lifecycle_state = candidate.lifecycle_state;
    hello.target = candidate.target;

    Ok(AdvancedBackendSession::new(AdvancedSessionId(1), hello))
}

pub fn resolve_backend_selection(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
) -> AdvancedBackendSelection {
    match profile.execution.backend {
        AdvancedProfileBackend::Auto => AdvancedBackendSelection {
            frontend,
            requested: AdvancedProfileBackend::Auto,
            resolved: inferred_backend(profile),
            reason: inferred_reason(profile),
            opt_in_mode: AdvancedBackendOptInMode::AutoDetectFromProfile,
            advanced_backend_requested: false,
        },
        backend => AdvancedBackendSelection {
            frontend,
            requested: backend,
            resolved: backend,
            reason: AdvancedBackendSelectionReason::ExplicitProfileBackend,
            opt_in_mode: AdvancedBackendOptInMode::ExplicitProfileOnly,
            advanced_backend_requested: backend == AdvancedProfileBackend::Inject,
        },
    }
}

pub fn catalog_for_frontend(frontend: AdvancedFrontend) -> AdvancedCapabilityCatalog {
    AdvancedCapabilityCatalog {
        frontends: vec![frontend],
        backends: vec![
            foreground_backend_descriptor(),
            message_backend_descriptor(),
            stub_backend_descriptor(),
        ],
    }
}

pub fn capability_requirements_for_profile(
    profile: &ProfileConfig,
) -> AdvancedCapabilityRequirements {
    match profile.action {
        winr_types::ProfileAction::MouseClick { input_mode, .. } => match input_mode {
            Some(MouseInputMode::Message) => AdvancedCapabilityRequirements {
                message_input: true,
                ..Default::default()
            },
            None | Some(MouseInputMode::Foreground) => AdvancedCapabilityRequirements {
                foreground_input: true,
                ..Default::default()
            },
        },
    }
}

pub fn select_backend_by_capabilities(
    catalog: &AdvancedCapabilityCatalog,
    requirements: &AdvancedCapabilityRequirements,
) -> AdvancedCapabilitySelection {
    let mut matches = catalog
        .backends
        .iter()
        .map(|descriptor| AdvancedCapabilityMatch {
            backend: descriptor.backend,
            satisfies_requirements: descriptor.capabilities.supports(requirements),
            score: capability_score(&descriptor.capabilities, requirements),
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        right
            .satisfies_requirements
            .cmp(&left.satisfies_requirements)
            .then_with(|| right.score.cmp(&left.score))
    });

    let selected_backend = matches
        .iter()
        .find(|entry| entry.satisfies_requirements)
        .map(|entry| entry.backend);

    AdvancedCapabilitySelection {
        requirements: requirements.clone(),
        matches,
        selected_backend,
    }
}

fn inferred_backend(profile: &ProfileConfig) -> AdvancedProfileBackend {
    let requirements = capability_requirements_for_profile(profile);
    let catalog = catalog_for_frontend(AdvancedFrontend::Cli);
    select_backend_by_capabilities(&catalog, &requirements)
        .selected_backend
        .unwrap_or(AdvancedProfileBackend::Foreground)
}

fn inferred_reason(profile: &ProfileConfig) -> AdvancedBackendSelectionReason {
    match profile.action {
        winr_types::ProfileAction::MouseClick {
            input_mode: Some(MouseInputMode::Message),
            ..
        } => AdvancedBackendSelectionReason::AutoFromMouseMessageAction,
        _ => AdvancedBackendSelectionReason::AutoDefaultForeground,
    }
}

pub fn stub_hello(profile: &ProfileConfig) -> AdvancedBackendHello {
    AdvancedBackendHello {
        protocol_version: 1,
        backend: AdvancedProfileBackend::Inject,
        lifecycle_state: AdvancedBackendLifecycleState::Discovered,
        capabilities: AdvancedBackendCapabilities::default(),
        target: winr_types::AdvancedTargetRef {
            hwnd: profile.target.hwnd.clone(),
            pid: profile.target.pid,
            exe: profile.target.exe.clone(),
            window_class: profile.target.class_name.clone(),
            title_hint: profile.target.title_contains.clone(),
        },
        transport: default_transport_descriptor(),
        composition: default_agent_composition(),
    }
}

pub fn build_execution_plan(profile: &ProfileConfig) -> AdvancedProfileExecutionPlan {
    let selection = resolve_backend_selection(profile, AdvancedFrontend::Cli);
    AdvancedProfileExecutionPlan {
        profile_id: profile.profile.id.clone(),
        backend: selection.resolved,
        target: winr_types::AdvancedTargetRef {
            hwnd: profile.target.hwnd.clone(),
            pid: profile.target.pid,
            exe: profile.target.exe.clone(),
            window_class: profile.target.class_name.clone(),
            title_hint: profile.target.title_contains.clone(),
        },
    }
}

pub fn discover_profile_targets(
    profile: &ProfileConfig,
) -> WinrResult<winr_types::AdvancedTargetDiscovery> {
    discover_attachable_targets(&profile.target)
}

pub fn selector_into_target_ref(selector: &WindowSelector) -> winr_types::AdvancedTargetRef {
    winr_types::AdvancedTargetRef {
        hwnd: selector.hwnd.clone(),
        pid: selector.pid,
        exe: selector.exe.clone(),
        window_class: selector.class_name.clone(),
        title_hint: selector.title_contains.clone(),
    }
}

impl AdvancedBackendSession {
    pub fn new(session_id: AdvancedSessionId, hello: AdvancedBackendHello) -> Self {
        Self {
            session_id,
            hello,
            next_host_sequence: AdvancedSequenceNumber(1),
            last_agent_sequence: None,
        }
    }

    pub fn handshake_command(&mut self) -> AdvancedHostCommandEnvelope {
        self.next_command(AdvancedHostCommand::Handshake {
            requested_backend: self.hello.backend,
            target: self.hello.target.clone(),
        })
    }

    pub fn start_profile_command(&mut self, profile_id: &str) -> AdvancedHostCommandEnvelope {
        self.next_command(AdvancedHostCommand::StartProfile {
            profile_id: profile_id.to_string(),
        })
    }

    pub fn stop_profile_command(&mut self, profile_id: &str) -> AdvancedHostCommandEnvelope {
        self.next_command(AdvancedHostCommand::StopProfile {
            profile_id: profile_id.to_string(),
        })
    }

    pub fn ping_command(&mut self) -> AdvancedHostCommandEnvelope {
        self.next_command(AdvancedHostCommand::Ping)
    }

    pub fn apply_event(&mut self, envelope: &AdvancedAgentEventEnvelope) -> WinrResult<()> {
        if envelope.session_id != self.session_id {
            return Err(advanced_error(
                AdvancedBackendErrorKind::SessionMismatch,
                self.hello.backend,
                format!(
                    "advanced backend session mismatch: expected {}, got {}",
                    self.session_id.0, envelope.session_id.0
                ),
                Some(self.hello.lifecycle_state),
            ));
        }

        if let Some(last) = self.last_agent_sequence
            && envelope.sequence.0 <= last.0
        {
            return Err(advanced_error(
                AdvancedBackendErrorKind::SequenceOutOfOrder,
                self.hello.backend,
                format!(
                    "advanced backend event sequence must increase: last={} next={}",
                    last.0, envelope.sequence.0
                ),
                Some(self.hello.lifecycle_state),
            ));
        }

        match &envelope.event {
            AdvancedAgentEvent::Hello { hello } => {
                if hello.backend != self.hello.backend {
                    return Err(advanced_error(
                        AdvancedBackendErrorKind::HandshakeMismatch,
                        self.hello.backend,
                        format!(
                            "advanced backend hello mismatch: expected '{}', got '{}'",
                            self.hello.backend.as_str(),
                            hello.backend.as_str()
                        ),
                        Some(self.hello.lifecycle_state),
                    ));
                }
                self.transition_to(hello.lifecycle_state)?;
                self.hello.capabilities = hello.capabilities.clone();
                self.hello.target = hello.target.clone();
                self.hello.protocol_version = hello.protocol_version;
            }
            AdvancedAgentEvent::Status { state, .. } => {
                self.transition_to(*state)?;
            }
            AdvancedAgentEvent::ObservationTick { .. } => {}
        }

        self.last_agent_sequence = Some(envelope.sequence);
        Ok(())
    }

    fn next_command(&mut self, command: AdvancedHostCommand) -> AdvancedHostCommandEnvelope {
        let envelope = AdvancedHostCommandEnvelope {
            session_id: self.session_id,
            sequence: self.next_host_sequence,
            command,
        };
        self.next_host_sequence.0 += 1;
        envelope
    }

    fn transition_to(&mut self, next: AdvancedBackendLifecycleState) -> WinrResult<()> {
        let current = self.hello.lifecycle_state;
        if !current.can_transition_to(next) {
            return Err(advanced_error(
                AdvancedBackendErrorKind::InvalidStateTransition,
                self.hello.backend,
                format!(
                    "invalid advanced backend state transition: {} -> {}",
                    lifecycle_name(current),
                    lifecycle_name(next)
                ),
                Some(current),
            ));
        }
        self.hello.lifecycle_state = next;
        Ok(())
    }
}

impl AdvancedObservationBackend for StubAdvancedBackend {
    fn descriptor(&self) -> AdvancedBackendDescriptor {
        stub_backend_descriptor()
    }

    fn discover_targets(
        &self,
        selector: &WindowSelector,
    ) -> WinrResult<winr_types::AdvancedTargetDiscovery> {
        discover_attachable_targets(selector)
    }
}

impl AdvancedInputBackend for StubAdvancedBackend {
    fn descriptor(&self) -> AdvancedBackendDescriptor {
        stub_backend_descriptor()
    }

    fn prepare_session(&self, profile: &ProfileConfig) -> WinrResult<AdvancedBackendSession> {
        prepare_profile_backend(profile)
    }
}

impl AdvancedWorkflowBackend for StubAdvancedBackend {
    fn build_execution_plan(&self, profile: &ProfileConfig) -> AdvancedProfileExecutionPlan {
        build_execution_plan(profile)
    }
}

pub fn stub_session(profile: &ProfileConfig) -> AdvancedBackendSession {
    AdvancedBackendSession::new(AdvancedSessionId(1), stub_hello(profile))
}

fn lifecycle_name(state: AdvancedBackendLifecycleState) -> &'static str {
    match state {
        AdvancedBackendLifecycleState::Discovered => "discovered",
        AdvancedBackendLifecycleState::Attachable => "attachable",
        AdvancedBackendLifecycleState::Attached => "attached",
        AdvancedBackendLifecycleState::Degraded => "degraded",
        AdvancedBackendLifecycleState::Detached => "detached",
    }
}

fn advanced_error(
    kind: AdvancedBackendErrorKind,
    backend: AdvancedProfileBackend,
    detail: String,
    lifecycle_state: Option<AdvancedBackendLifecycleState>,
) -> WinrError {
    let payload = AdvancedBackendError {
        kind,
        backend,
        detail,
        lifecycle_state,
    };

    WinrError::Unsupported {
        message: format!(
            "advanced backend error: {}",
            serde_json::to_string(&payload)
                .unwrap_or_else(|_| "{\"kind\":\"unknown\"}".to_string())
        ),
    }
}

pub fn stub_backend_descriptor() -> AdvancedBackendDescriptor {
    AdvancedBackendDescriptor {
        backend: AdvancedProfileBackend::Inject,
        stability: AdvancedBackendStability::Fragile,
        capabilities: AdvancedBackendCapabilities::default(),
        replaceable: true,
        app_pack_specific: false,
        notes: vec![
            "advanced backends are selected by capabilities instead of app-specific assumptions"
                .to_string(),
            "low-level attach and injection implementations are treated as replaceable".to_string(),
        ],
    }
}

pub fn default_transport_descriptor() -> AdvancedIpcTransportDescriptor {
    AdvancedIpcTransportDescriptor {
        kind: AdvancedIpcTransportKind::InProcess,
        supports_commands: true,
        supports_events: true,
        supports_binary_payloads: true,
        ordered_delivery: true,
        notes: vec![
            "phase 2 starts with an in-process transport for protocol validation".to_string(),
            "later transports can swap in named pipes or shared memory without changing the host runtime".to_string(),
        ],
    }
}

pub fn default_agent_composition() -> AdvancedAgentComposition {
    AdvancedAgentComposition {
        roles: vec![
            AdvancedAgentRole::InputShim,
            AdvancedAgentRole::RenderObserver,
            AdvancedAgentRole::MemoryObserver,
        ],
    }
}

pub fn foreground_backend_descriptor() -> AdvancedBackendDescriptor {
    AdvancedBackendDescriptor {
        backend: AdvancedProfileBackend::Foreground,
        stability: AdvancedBackendStability::Stable,
        capabilities: AdvancedBackendCapabilities {
            foreground_input: true,
            ..Default::default()
        },
        replaceable: false,
        app_pack_specific: false,
        notes: vec!["standard desktop foreground input backend".to_string()],
    }
}

pub fn message_backend_descriptor() -> AdvancedBackendDescriptor {
    AdvancedBackendDescriptor {
        backend: AdvancedProfileBackend::Message,
        stability: AdvancedBackendStability::Experimental,
        capabilities: AdvancedBackendCapabilities {
            message_input: true,
            ..Default::default()
        },
        replaceable: true,
        app_pack_specific: false,
        notes: vec!["classic Win32-oriented background message backend".to_string()],
    }
}

fn capability_score(
    capabilities: &AdvancedBackendCapabilities,
    requirements: &AdvancedCapabilityRequirements,
) -> u32 {
    let mut score = 0_u32;
    if requirements.foreground_input && capabilities.foreground_input {
        score += 1;
    }
    if requirements.message_input && capabilities.message_input {
        score += 1;
    }
    if requirements.uia_input && capabilities.uia_input {
        score += 1;
    }
    if requirements.injected_input && capabilities.injected_input {
        score += 1;
    }
    if requirements.render_observation && capabilities.render_observation {
        score += 1;
    }
    if requirements.memory_observation && capabilities.memory_observation {
        score += 1;
    }
    if requirements.semantic_navigation && capabilities.semantic_navigation {
        score += 1;
    }
    if requirements.entity_tracking && capabilities.entity_tracking {
        score += 1;
    }
    if requirements.internal_interaction && capabilities.internal_interaction {
        score += 1;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ProfileConfig {
        toml::from_str(
            r#"
[profile]
id = "inject-demo"
name = "Inject Demo"
description = "Test profile"
version = "1"

[target]
title_contains = "Roblox"
exe = "RobloxPlayerBeta.exe"

[action]
kind = "mouse_click"
button = "left"

[schedule]
mode = "interval"
every_ms = 50

[logging]
level = "info"
mode = "single_line_counter"
update_every_trigger = true
template = "count={count}"

[safety]
require_visible_window = true
require_foreground_window = true
stop_on_focus_loss = true
"#,
        )
        .expect("sample profile should parse")
    }

    #[test]
    fn auto_backend_defaults_to_foreground() {
        let profile = sample_profile();
        let selection = resolve_backend_selection(&profile, AdvancedFrontend::Cli);
        assert_eq!(selection.requested, AdvancedProfileBackend::Auto);
        assert_eq!(selection.resolved, AdvancedProfileBackend::Foreground);
    }

    #[test]
    fn explicit_inject_backend_returns_stub_error() {
        let mut profile = sample_profile();
        profile.execution.backend = AdvancedProfileBackend::Inject;
        let result = prepare_profile_backend(&profile);
        match result {
            Ok(session) => assert_eq!(session.hello.backend, AdvancedProfileBackend::Inject),
            Err(error) => assert!(matches!(
                error,
                WinrError::WindowNotFound | WinrError::Unsupported { .. }
            )),
        }
    }

    #[test]
    fn stub_backend_descriptor_is_fragile_and_replaceable() {
        let descriptor = stub_backend_descriptor();
        assert_eq!(descriptor.backend, AdvancedProfileBackend::Inject);
        assert_eq!(descriptor.stability, AdvancedBackendStability::Fragile);
        assert!(descriptor.replaceable);
    }

    #[test]
    fn capability_selection_prefers_message_backend_when_required() {
        let catalog = catalog_for_frontend(AdvancedFrontend::Cli);
        let selection = select_backend_by_capabilities(
            &catalog,
            &AdvancedCapabilityRequirements {
                message_input: true,
                ..Default::default()
            },
        );

        assert_eq!(
            selection.selected_backend,
            Some(AdvancedProfileBackend::Message)
        );
        assert!(
            selection
                .matches
                .iter()
                .any(|entry| entry.satisfies_requirements)
        );
    }

    #[test]
    fn session_emits_incrementing_host_commands() {
        let profile = sample_profile();
        let mut session = stub_session(&profile);

        let handshake = session.handshake_command();
        let ping = session.ping_command();

        assert_eq!(handshake.sequence.0, 1);
        assert_eq!(ping.sequence.0, 2);
        assert_eq!(handshake.session_id.0, ping.session_id.0);
    }

    #[test]
    fn session_accepts_valid_status_transition() {
        let profile = sample_profile();
        let mut session = stub_session(&profile);

        session
            .apply_event(&AdvancedAgentEventEnvelope {
                session_id: session.session_id,
                sequence: AdvancedSequenceNumber(1),
                event: AdvancedAgentEvent::Status {
                    state: AdvancedBackendLifecycleState::Attachable,
                    detail: "ready".to_string(),
                },
            })
            .expect("attachable transition should succeed");

        assert_eq!(
            session.hello.lifecycle_state,
            AdvancedBackendLifecycleState::Attachable
        );
    }

    #[test]
    fn session_rejects_invalid_status_transition() {
        let profile = sample_profile();
        let mut session = stub_session(&profile);

        let error = session
            .apply_event(&AdvancedAgentEventEnvelope {
                session_id: session.session_id,
                sequence: AdvancedSequenceNumber(1),
                event: AdvancedAgentEvent::Status {
                    state: AdvancedBackendLifecycleState::Attached,
                    detail: "skipped attachable".to_string(),
                },
            })
            .unwrap_err();

        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn session_rejects_out_of_order_agent_sequences() {
        let profile = sample_profile();
        let mut session = stub_session(&profile);

        session
            .apply_event(&AdvancedAgentEventEnvelope {
                session_id: session.session_id,
                sequence: AdvancedSequenceNumber(3),
                event: AdvancedAgentEvent::ObservationTick {
                    update: winr_types::AdvancedObservationUpdate {
                        frame_id: 1,
                        source: "stub".to_string(),
                        detail: "frame".to_string(),
                        payload: None,
                    },
                },
            })
            .expect("first event should succeed");

        let error = session
            .apply_event(&AdvancedAgentEventEnvelope {
                session_id: session.session_id,
                sequence: AdvancedSequenceNumber(2),
                event: AdvancedAgentEvent::ObservationTick {
                    update: winr_types::AdvancedObservationUpdate {
                        frame_id: 2,
                        source: "stub".to_string(),
                        detail: "late frame".to_string(),
                        payload: None,
                    },
                },
            })
            .unwrap_err();

        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
