mod discovery;

use tracing::{debug, instrument};
use winr_types::{
    AdvancedAgentEvent, AdvancedAgentEventEnvelope, AdvancedBackendCapabilities,
    AdvancedBackendHello, AdvancedBackendLifecycleState, AdvancedBackendSelection,
    AdvancedHostCommand, AdvancedHostCommandEnvelope, AdvancedProfileBackend,
    AdvancedProfileExecutionPlan, AdvancedSequenceNumber, AdvancedSessionId, ProfileConfig,
    WindowSelector, WinrError, WinrResult,
};

pub use discovery::{discover_attachable_targets, resolve_attachable_target};

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

    let selection = resolve_backend_selection(profile);
    if selection.resolved != AdvancedProfileBackend::Inject {
        return Err(WinrError::Unsupported {
            message: format!(
                "advanced backend preparation is only valid for resolved backend '{}'",
                AdvancedProfileBackend::Inject.as_str()
            ),
        });
    }

    let discovery = discover_profile_targets(profile)?;
    let candidate = resolve_attachable_target(&discovery)?;
    let mut hello = stub_hello(profile);
    hello.lifecycle_state = candidate.lifecycle_state;
    hello.target = candidate.target;

    Ok(AdvancedBackendSession::new(AdvancedSessionId(1), hello))
}

pub fn resolve_backend_selection(profile: &ProfileConfig) -> AdvancedBackendSelection {
    match profile.execution.backend {
        AdvancedProfileBackend::Auto => AdvancedBackendSelection {
            requested: AdvancedProfileBackend::Auto,
            resolved: inferred_backend(profile),
        },
        backend => AdvancedBackendSelection {
            requested: backend,
            resolved: backend,
        },
    }
}

fn inferred_backend(profile: &ProfileConfig) -> AdvancedProfileBackend {
    match profile.action {
        winr_types::ProfileAction::MouseClick {
            input_mode: Some(winr_types::MouseInputMode::Message),
            ..
        } => AdvancedProfileBackend::Message,
        _ => AdvancedProfileBackend::Foreground,
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
    }
}

pub fn build_execution_plan(profile: &ProfileConfig) -> AdvancedProfileExecutionPlan {
    let selection = resolve_backend_selection(profile);
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
            return Err(WinrError::Unsupported {
                message: format!(
                    "advanced backend session mismatch: expected {}, got {}",
                    self.session_id.0, envelope.session_id.0
                ),
            });
        }

        if let Some(last) = self.last_agent_sequence
            && envelope.sequence.0 <= last.0
        {
            return Err(WinrError::Unsupported {
                message: format!(
                    "advanced backend event sequence must increase: last={} next={}",
                    last.0, envelope.sequence.0
                ),
            });
        }

        match &envelope.event {
            AdvancedAgentEvent::Hello { hello } => {
                if hello.backend != self.hello.backend {
                    return Err(WinrError::Unsupported {
                        message: format!(
                            "advanced backend hello mismatch: expected '{}', got '{}'",
                            self.hello.backend.as_str(),
                            hello.backend.as_str()
                        ),
                    });
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
            return Err(WinrError::Unsupported {
                message: format!(
                    "invalid advanced backend state transition: {} -> {}",
                    lifecycle_name(current),
                    lifecycle_name(next)
                ),
            });
        }
        self.hello.lifecycle_state = next;
        Ok(())
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
        let selection = resolve_backend_selection(&profile);
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
                    frame_id: 1,
                    detail: "frame".to_string(),
                },
            })
            .expect("first event should succeed");

        let error = session
            .apply_event(&AdvancedAgentEventEnvelope {
                session_id: session.session_id,
                sequence: AdvancedSequenceNumber(2),
                event: AdvancedAgentEvent::ObservationTick {
                    frame_id: 2,
                    detail: "late frame".to_string(),
                },
            })
            .unwrap_err();

        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
