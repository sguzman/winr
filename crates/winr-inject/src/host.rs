use winr_types::{
    AdvancedAgentEvent, AdvancedBackendErrorKind, AdvancedHostResponse, ProfileConfig, WinrResult,
};

use crate::{
    advanced_error, discover_profile_targets, prepare_profile_backend, resolve_attachable_target,
    selector_into_target_ref, AdvancedBackendSession, AdvancedAgentTransport, AttachmentSupervisor,
};

#[derive(Debug)]
pub struct AdvancedHostRuntime<TTransport> {
    pub profile: ProfileConfig,
    pub session: AdvancedBackendSession,
    pub attachment: AttachmentSupervisor,
    pub transport: TTransport,
}

impl<TTransport> AdvancedHostRuntime<TTransport>
where
    TTransport: AdvancedAgentTransport,
{
    pub fn attach(profile: &ProfileConfig, transport: TTransport) -> WinrResult<Self> {
        let session = prepare_profile_backend(profile)?;
        let target = resolve_attachable_target(&discover_profile_targets(profile)?)?;
        let (attachment, _) = AttachmentSupervisor::attach(
            &profile.target,
            winr_types::AdvancedAttachmentPolicy::default(),
        )?;

        debug_assert_eq!(target.target, session.hello.target);

        Ok(Self {
            profile: profile.clone(),
            session,
            attachment,
            transport,
        })
    }

    pub fn send_handshake(&mut self) -> WinrResult<()> {
        let command = self.session.handshake_command();
        self.transport.send_command(command)
    }

    pub fn request_capabilities(&mut self) -> WinrResult<()> {
        let command = self.session_command(winr_types::AdvancedHostCommand::GetCapabilities);
        self.transport.send_command(command)
    }

    pub fn subscribe_events(&mut self) -> WinrResult<()> {
        let command = self.session_command(winr_types::AdvancedHostCommand::SubscribeEvents);
        self.transport.send_command(command)
    }

    pub fn start_profile(&mut self) -> WinrResult<()> {
        self.transport
            .send_command(self.session.start_profile_command(&self.profile.profile.id))
    }

    pub fn stop_profile(&mut self) -> WinrResult<()> {
        self.transport
            .send_command(self.session.stop_profile_command(&self.profile.profile.id))
    }

    pub fn ping(&mut self) -> WinrResult<()> {
        self.transport.send_command(self.session.ping_command())
    }

    pub fn poll_responses(&mut self) -> WinrResult<Vec<AdvancedHostResponse>> {
        let mut responses = Vec::new();
        while let Some(envelope) = self.transport.recv_response()? {
            if envelope.session_id != self.session.session_id {
                return Err(advanced_error(
                    AdvancedBackendErrorKind::ResponseMismatch,
                    self.session.hello.backend,
                    "advanced backend response session mismatch".to_string(),
                    Some(self.session.hello.lifecycle_state),
                ));
            }
            responses.push(envelope.response);
        }
        Ok(responses)
    }

    pub fn poll_events(&mut self) -> WinrResult<Vec<AdvancedAgentEvent>> {
        let mut events = Vec::new();
        while let Some(envelope) = self.transport.recv_event()? {
            self.session.apply_event(&envelope)?;
            events.push(envelope.event);
        }
        Ok(events)
    }

    pub fn fallback_target_ref(&self) -> winr_types::AdvancedTargetRef {
        selector_into_target_ref(&self.profile.target)
    }

    fn session_command(
        &mut self,
        command: winr_types::AdvancedHostCommand,
    ) -> winr_types::AdvancedHostCommandEnvelope {
        let envelope = winr_types::AdvancedHostCommandEnvelope {
            session_id: self.session.session_id,
            sequence: self.session.next_host_sequence,
            command,
        };
        self.session.next_host_sequence.0 += 1;
        envelope
    }
}
