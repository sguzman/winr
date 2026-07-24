use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::{Read, Write},
    os::windows::io::AsRawHandle,
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{Foundation::HANDLE, System::Pipes::PeekNamedPipe};

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

#[derive(Debug)]
pub struct NamedPipeAgentTransport {
    command_pipe: std::fs::File,
    event_pipe: std::fs::File,
}

impl NamedPipeAgentTransport {
    pub fn connect(
        command_pipe_name: &str,
        event_pipe_name: &str,
        timeout: Duration,
    ) -> WinrResult<Self> {
        let command_pipe = connect_pipe(command_pipe_name, timeout)?;
        let event_pipe = connect_pipe(event_pipe_name, timeout)?;
        Ok(Self {
            command_pipe,
            event_pipe,
        })
    }
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

impl AdvancedAgentTransport for NamedPipeAgentTransport {
    fn send_command(&mut self, command: AdvancedHostCommandEnvelope) -> WinrResult<()> {
        let bytes =
            serde_json::to_vec(&command).map_err(|error| winr_types::WinrError::Unsupported {
                message: format!(
                    "failed to serialize host command for named pipe transport: {error}"
                ),
            })?;
        write_frame(&mut self.command_pipe, &bytes)
    }

    fn recv_response(&mut self) -> WinrResult<Option<AdvancedHostResponseEnvelope>> {
        let bytes = read_frame(&mut self.command_pipe)?;
        let response =
            serde_json::from_slice(&bytes).map_err(|error| winr_types::WinrError::Unsupported {
                message: format!(
                    "failed to deserialize host response from named pipe transport: {error}"
                ),
            })?;
        Ok(Some(response))
    }

    fn recv_event(&mut self) -> WinrResult<Option<AdvancedAgentEventEnvelope>> {
        if !pipe_has_bytes(&self.event_pipe)? {
            return Ok(None);
        }

        let bytes = read_frame(&mut self.event_pipe)?;
        let event =
            serde_json::from_slice(&bytes).map_err(|error| winr_types::WinrError::Unsupported {
                message: format!(
                    "failed to deserialize agent event from named pipe transport: {error}"
                ),
            })?;
        Ok(Some(event))
    }

    fn push_binary_payload(
        &mut self,
        payload: AdvancedBinaryPayloadRef,
        bytes: Vec<u8>,
    ) -> WinrResult<()> {
        let _ = (payload, bytes);
        Err(winr_types::WinrError::Unsupported {
            message: "named pipe transport does not implement binary payload storage yet"
                .to_string(),
        })
    }

    fn take_binary_payload(&mut self, payload_id: &str) -> WinrResult<Option<Vec<u8>>> {
        let _ = payload_id;
        Ok(None)
    }
}

fn connect_pipe(pipe_name: &str, timeout: Duration) -> WinrResult<std::fs::File> {
    let path = format!(r"\\.\pipe\{pipe_name}");
    let started = Instant::now();
    loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => return Ok(file),
            Err(error) if started.elapsed() < timeout => {
                let _ = error;
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(winr_types::WinrError::Unsupported {
                    message: format!("failed to connect to named pipe {path}: {error}"),
                });
            }
        }
    }
}

fn pipe_has_bytes(file: &std::fs::File) -> WinrResult<bool> {
    let handle = HANDLE(file.as_raw_handle() as isize as *mut std::ffi::c_void);
    let mut available = 0u32;
    unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut available), None) }.map_err(
        |error| winr_types::WinrError::Unsupported {
            message: format!("PeekNamedPipe failed while polling agent events: {error}"),
        },
    )?;
    Ok(available > 0)
}

fn write_frame(file: &mut std::fs::File, bytes: &[u8]) -> WinrResult<()> {
    let len = bytes.len() as u32;
    file.write_all(&len.to_le_bytes())
        .and_then(|_| file.write_all(bytes))
        .and_then(|_| file.flush())
        .map_err(|error| winr_types::WinrError::Unsupported {
            message: format!("failed to write named pipe frame: {error}"),
        })
}

fn read_frame(file: &mut std::fs::File) -> WinrResult<Vec<u8>> {
    let mut len_bytes = [0u8; 4];
    file.read_exact(&mut len_bytes)
        .map_err(|error| winr_types::WinrError::Unsupported {
            message: format!("failed to read named pipe frame length: {error}"),
        })?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes)
        .map_err(|error| winr_types::WinrError::Unsupported {
            message: format!("failed to read named pipe frame payload: {error}"),
        })?;
    Ok(bytes)
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
