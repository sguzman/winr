use std::collections::VecDeque;

use winr_types::{
    AdvancedAgentEventEnvelope, AdvancedBinaryPayloadRef, AdvancedHostCommandEnvelope,
    AdvancedHostResponseEnvelope, WinrResult,
};

pub trait AdvancedAgentTransport {
    fn send_command(&mut self, command: AdvancedHostCommandEnvelope) -> WinrResult<()>;
    fn recv_response(&mut self) -> WinrResult<Option<AdvancedHostResponseEnvelope>>;
    fn recv_event(&mut self) -> WinrResult<Option<AdvancedAgentEventEnvelope>>;
    fn push_binary_payload(
        &mut self,
        payload: AdvancedBinaryPayloadRef,
        bytes: Vec<u8>,
    ) -> WinrResult<()>;
    fn take_binary_payload(&mut self, payload_id: &str) -> WinrResult<Option<Vec<u8>>>;
}

#[derive(Debug, Default)]
pub struct InMemoryAgentTransport {
    commands: VecDeque<AdvancedHostCommandEnvelope>,
    responses: VecDeque<AdvancedHostResponseEnvelope>,
    events: VecDeque<AdvancedAgentEventEnvelope>,
    binary_payloads: Vec<(AdvancedBinaryPayloadRef, Vec<u8>)>,
}

impl InMemoryAgentTransport {
    pub fn pop_command(&mut self) -> Option<AdvancedHostCommandEnvelope> {
        self.commands.pop_front()
    }

    pub fn push_response(&mut self, response: AdvancedHostResponseEnvelope) {
        self.responses.push_back(response);
    }

    pub fn push_event(&mut self, event: AdvancedAgentEventEnvelope) {
        self.events.push_back(event);
    }
}

impl AdvancedAgentTransport for InMemoryAgentTransport {
    fn send_command(&mut self, command: AdvancedHostCommandEnvelope) -> WinrResult<()> {
        self.commands.push_back(command);
        Ok(())
    }

    fn recv_response(&mut self) -> WinrResult<Option<AdvancedHostResponseEnvelope>> {
        Ok(self.responses.pop_front())
    }

    fn recv_event(&mut self) -> WinrResult<Option<AdvancedAgentEventEnvelope>> {
        Ok(self.events.pop_front())
    }

    fn push_binary_payload(
        &mut self,
        payload: AdvancedBinaryPayloadRef,
        bytes: Vec<u8>,
    ) -> WinrResult<()> {
        self.binary_payloads.push((payload, bytes));
        Ok(())
    }

    fn take_binary_payload(&mut self, payload_id: &str) -> WinrResult<Option<Vec<u8>>> {
        if let Some(index) = self
            .binary_payloads
            .iter()
            .position(|(payload, _)| payload.payload_id == payload_id)
        {
            let (_, bytes) = self.binary_payloads.remove(index);
            return Ok(Some(bytes));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_types::{AdvancedIpcTransportKind, AdvancedPayloadEncoding};

    #[test]
    fn in_memory_transport_round_trips_binary_payloads() {
        let mut transport = InMemoryAgentTransport::default();
        let payload = AdvancedBinaryPayloadRef {
            payload_id: "frame-1".to_string(),
            encoding: AdvancedPayloadEncoding::RawBytes,
            byte_len: 3,
            transport: AdvancedIpcTransportKind::SharedMemory,
            description: "sample frame".to_string(),
        };

        transport
            .push_binary_payload(payload.clone(), vec![1, 2, 3])
            .expect("binary payload should store");

        let bytes = transport
            .take_binary_payload(&payload.payload_id)
            .expect("binary payload lookup should succeed")
            .expect("payload should exist");

        assert_eq!(bytes, vec![1, 2, 3]);
    }
}
