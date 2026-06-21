use winr_types::{
    AdvancedAgentComposition, AdvancedAgentEvent, AdvancedAgentEventEnvelope,
    AdvancedBackendCapabilities, AdvancedBackendHello, AdvancedHostCommand,
    AdvancedHostCommandEnvelope, AdvancedHostResponse, AdvancedHostResponseEnvelope,
    AdvancedObservationUpdate, AdvancedSequenceNumber, WinrResult,
};

pub trait AdvancedAgentRuntime {
    fn hello(&self) -> AdvancedBackendHello;
    fn capabilities(&self) -> AdvancedBackendCapabilities;
    fn composition(&self) -> AdvancedAgentComposition;
    fn handle_command(
        &mut self,
        command: &AdvancedHostCommandEnvelope,
    ) -> WinrResult<AdvancedHostResponseEnvelope>;
    fn poll_events(
        &mut self,
        session_id: winr_types::AdvancedSessionId,
    ) -> WinrResult<Vec<AdvancedAgentEventEnvelope>>;
}

#[derive(Debug, Clone)]
pub struct StubAdvancedAgent {
    hello: AdvancedBackendHello,
    next_agent_sequence: AdvancedSequenceNumber,
    observations: Vec<AdvancedObservationUpdate>,
}

impl StubAdvancedAgent {
    pub fn new(hello: AdvancedBackendHello) -> Self {
        Self {
            hello,
            next_agent_sequence: AdvancedSequenceNumber(1),
            observations: Vec::new(),
        }
    }

    pub fn queue_observation(&mut self, update: AdvancedObservationUpdate) {
        self.observations.push(update);
    }

    fn next_event(
        &mut self,
        session_id: winr_types::AdvancedSessionId,
        event: AdvancedAgentEvent,
    ) -> AdvancedAgentEventEnvelope {
        let envelope = AdvancedAgentEventEnvelope {
            session_id,
            sequence: self.next_agent_sequence,
            event,
        };
        self.next_agent_sequence.0 += 1;
        envelope
    }

    fn response_for(
        &self,
        command: &AdvancedHostCommandEnvelope,
        response: AdvancedHostResponse,
    ) -> AdvancedHostResponseEnvelope {
        AdvancedHostResponseEnvelope {
            session_id: command.session_id,
            sequence: command.sequence,
            response_to: command.sequence,
            response,
        }
    }
}

impl AdvancedAgentRuntime for StubAdvancedAgent {
    fn hello(&self) -> AdvancedBackendHello {
        self.hello.clone()
    }

    fn capabilities(&self) -> AdvancedBackendCapabilities {
        self.hello.capabilities.clone()
    }

    fn composition(&self) -> AdvancedAgentComposition {
        self.hello.composition.clone()
    }

    fn handle_command(
        &mut self,
        command: &AdvancedHostCommandEnvelope,
    ) -> WinrResult<AdvancedHostResponseEnvelope> {
        let response = match &command.command {
            AdvancedHostCommand::Handshake { .. } => AdvancedHostResponse::Hello {
                hello: self.hello(),
            },
            AdvancedHostCommand::GetCapabilities => AdvancedHostResponse::Capabilities {
                capabilities: self.capabilities(),
                composition: self.composition(),
            },
            AdvancedHostCommand::StartProfile { profile_id } => AdvancedHostResponse::Ack {
                detail: format!("profile '{profile_id}' started in stub agent"),
            },
            AdvancedHostCommand::StopProfile { profile_id } => AdvancedHostResponse::Ack {
                detail: format!("profile '{profile_id}' stopped in stub agent"),
            },
            AdvancedHostCommand::SubscribeEvents => AdvancedHostResponse::Ack {
                detail: "event stream subscribed".to_string(),
            },
            AdvancedHostCommand::FetchObservations { max_items } => {
                let count = (*max_items as usize).min(self.observations.len());
                let updates = self.observations.drain(0..count).collect();
                AdvancedHostResponse::Observations { updates }
            }
            AdvancedHostCommand::ExecuteInput { action } => AdvancedHostResponse::InputOutcome {
                status: winr_types::AdvancedCommandAckStatus::Completed,
                detail: format!("stub agent completed {action:?}"),
            },
            AdvancedHostCommand::Ping => AdvancedHostResponse::Pong {
                detail: "stub agent alive".to_string(),
            },
        };

        Ok(self.response_for(command, response))
    }

    fn poll_events(
        &mut self,
        session_id: winr_types::AdvancedSessionId,
    ) -> WinrResult<Vec<AdvancedAgentEventEnvelope>> {
        let mut events = Vec::new();
        let updates = self.observations.drain(..).collect::<Vec<_>>();
        for update in updates {
            events
                .push(self.next_event(session_id, AdvancedAgentEvent::ObservationTick { update }));
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{default_agent_composition, default_transport_descriptor};
    use winr_types::{
        AdvancedBackendCapabilities, AdvancedBackendHello, AdvancedBackendLifecycleState,
        AdvancedProfileBackend, AdvancedTargetRef,
    };

    fn sample_hello() -> AdvancedBackendHello {
        AdvancedBackendHello {
            protocol_version: 1,
            backend: AdvancedProfileBackend::Inject,
            lifecycle_state: AdvancedBackendLifecycleState::Attachable,
            capabilities: AdvancedBackendCapabilities {
                injected_input: true,
                render_observation: true,
                ..Default::default()
            },
            target: AdvancedTargetRef {
                hwnd: Some("0x0000000000001111".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            transport: default_transport_descriptor(),
            composition: default_agent_composition(),
        }
    }

    #[test]
    fn stub_agent_returns_hello_on_handshake() {
        let mut agent = StubAdvancedAgent::new(sample_hello());
        let response = agent
            .handle_command(&AdvancedHostCommandEnvelope {
                session_id: winr_types::AdvancedSessionId(7),
                sequence: AdvancedSequenceNumber(1),
                command: AdvancedHostCommand::Handshake {
                    requested_backend: AdvancedProfileBackend::Inject,
                    target: agent.hello().target,
                },
            })
            .expect("handshake should succeed");

        match response.response {
            AdvancedHostResponse::Hello { hello } => {
                assert_eq!(hello.backend, AdvancedProfileBackend::Inject);
                assert!(hello.transport.supports_events);
            }
            other => panic!("expected hello response, got {other:?}"),
        }
    }

    #[test]
    fn stub_agent_drains_observations_via_fetch() {
        let mut agent = StubAdvancedAgent::new(sample_hello());
        agent.queue_observation(AdvancedObservationUpdate {
            frame_id: 10,
            source: "render-hook".to_string(),
            detail: "sample".to_string(),
            timestamp_ms: Some(100),
            freshness_ms: Some(16),
            payload: None,
        });

        let response = agent
            .handle_command(&AdvancedHostCommandEnvelope {
                session_id: winr_types::AdvancedSessionId(7),
                sequence: AdvancedSequenceNumber(2),
                command: AdvancedHostCommand::FetchObservations { max_items: 4 },
            })
            .expect("fetch should succeed");

        match response.response {
            AdvancedHostResponse::Observations { updates } => {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].source, "render-hook");
            }
            other => panic!("expected observations response, got {other:?}"),
        }
    }
}
