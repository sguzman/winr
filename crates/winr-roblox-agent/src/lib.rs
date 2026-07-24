use std::{
    ffi::c_void,
    fs,
    fs::File,
    io::{Read, Write},
    os::windows::io::FromRawHandle,
    path::PathBuf,
    ptr::null_mut,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HINSTANCE, HWND, LPARAM, WPARAM},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::{
            LibraryLoader::GetModuleHandleW,
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
                PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
            Threading::{CreateThread, GetCurrentProcessId, THREAD_CREATION_FLAGS},
        },
        UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP},
    },
    core::{BOOL, PCWSTR},
};
use winr_types::{
    AdvancedAgentComposition, AdvancedAgentEvent, AdvancedAgentEventEnvelope,
    AdvancedBackendCapabilities, AdvancedBackendHello, AdvancedBackendLifecycleState,
    AdvancedCommandAckStatus, AdvancedHostCommand, AdvancedHostCommandEnvelope,
    AdvancedHostResponse, AdvancedHostResponseEnvelope, AdvancedIpcTransportDescriptor,
    AdvancedIpcTransportKind, AdvancedProfileBackend, AdvancedSequenceNumber, AdvancedSessionId,
    AdvancedTargetRef, InjectedInputAction, RobloxMemoryField, RobloxMemorySchema,
    RobloxMemoryValueKind, RobloxObservationSnapshot, RobloxObservedObject,
};

static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentBootstrapConfig {
    session_id: u64,
    command_pipe_name: String,
    event_pipe_name: String,
    schema_path: String,
    target: AdvancedTargetRef,
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH && !STARTED.swap(true, Ordering::SeqCst) {
        let _ = unsafe {
            windows::Win32::System::LibraryLoader::DisableThreadLibraryCalls(module.into())
        };
        let handle = unsafe {
            CreateThread(
                None,
                0,
                Some(agent_thread),
                Some(null_mut()),
                THREAD_CREATION_FLAGS(0),
                None,
            )
        };
        if let Ok(handle) = handle {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
    true.into()
}

unsafe extern "system" fn agent_thread(_: *mut c_void) -> u32 {
    let _ = run_agent();
    0
}

fn run_agent() -> Result<(), String> {
    let bootstrap = load_bootstrap()?;
    let schema_text = fs::read_to_string(&bootstrap.schema_path)
        .map_err(|error| format!("failed to read schema {}: {error}", bootstrap.schema_path))?;
    let schema: RobloxMemorySchema = toml::from_str(&schema_text)
        .map_err(|error| format!("failed to parse schema {}: {error}", bootstrap.schema_path))?;

    let mut command_pipe = open_server_pipe(&bootstrap.command_pipe_name)?;
    let mut event_pipe = open_server_pipe(&bootstrap.event_pipe_name)?;

    let mut frame_id = 0u64;
    let mut next_event_sequence = 1u64;

    loop {
        let command: AdvancedHostCommandEnvelope = read_json_frame(&mut command_pipe)?;
        let response = match &command.command {
            AdvancedHostCommand::Handshake { .. } => {
                write_json_frame(
                    &mut event_pipe,
                    &AdvancedAgentEventEnvelope {
                        session_id: AdvancedSessionId(bootstrap.session_id),
                        sequence: AdvancedSequenceNumber(next_event_sequence),
                        event: AdvancedAgentEvent::Hello {
                            hello: build_hello(&bootstrap.target),
                        },
                    },
                )?;
                next_event_sequence += 1;
                AdvancedHostResponseEnvelope {
                    session_id: command.session_id,
                    sequence: command.sequence,
                    response_to: command.sequence,
                    response: AdvancedHostResponse::Hello {
                        hello: build_hello(&bootstrap.target),
                    },
                }
            }
            AdvancedHostCommand::GetCapabilities => AdvancedHostResponseEnvelope {
                session_id: command.session_id,
                sequence: command.sequence,
                response_to: command.sequence,
                response: AdvancedHostResponse::Capabilities {
                    capabilities: build_capabilities(),
                    composition: build_composition(),
                },
            },
            AdvancedHostCommand::SubscribeEvents => ack(&command, "event subscription active"),
            AdvancedHostCommand::StartProfile { profile_id } => ack(
                &command,
                &format!("profile '{profile_id}' started in injected agent"),
            ),
            AdvancedHostCommand::StopProfile { profile_id } => ack(
                &command,
                &format!("profile '{profile_id}' stopped in injected agent"),
            ),
            AdvancedHostCommand::FetchObservations { max_items } => {
                let _ = max_items;
                frame_id += 1;
                let snapshot = capture_snapshot(&schema, frame_id)?;
                let update = winr_types::AdvancedObservationUpdate {
                    frame_id,
                    source: snapshot.source.clone(),
                    detail: snapshot.detail.clone(),
                    timestamp_ms: Some(snapshot.timestamp_ms),
                    freshness_ms: Some(snapshot.freshness_ms),
                    payload: None,
                };
                AdvancedHostResponseEnvelope {
                    session_id: command.session_id,
                    sequence: command.sequence,
                    response_to: command.sequence,
                    response: AdvancedHostResponse::RobloxObservations {
                        updates: vec![update],
                        snapshots: vec![snapshot],
                    },
                }
            }
            AdvancedHostCommand::ExecuteInput { action } => AdvancedHostResponseEnvelope {
                session_id: command.session_id,
                sequence: command.sequence,
                response_to: command.sequence,
                response: execute_input(&bootstrap.target, *action),
            },
            AdvancedHostCommand::Ping => AdvancedHostResponseEnvelope {
                session_id: command.session_id,
                sequence: command.sequence,
                response_to: command.sequence,
                response: AdvancedHostResponse::Pong {
                    detail: "injected agent alive".to_string(),
                },
            },
        };

        write_json_frame(&mut command_pipe, &response)?;
    }
}

fn ack(command: &AdvancedHostCommandEnvelope, detail: &str) -> AdvancedHostResponseEnvelope {
    AdvancedHostResponseEnvelope {
        session_id: command.session_id,
        sequence: command.sequence,
        response_to: command.sequence,
        response: AdvancedHostResponse::Ack {
            detail: detail.to_string(),
        },
    }
}

fn build_hello(target: &AdvancedTargetRef) -> AdvancedBackendHello {
    AdvancedBackendHello {
        protocol_version: 1,
        backend: AdvancedProfileBackend::Inject,
        lifecycle_state: AdvancedBackendLifecycleState::Attached,
        capabilities: build_capabilities(),
        target: target.clone(),
        transport: AdvancedIpcTransportDescriptor {
            kind: AdvancedIpcTransportKind::NamedPipe,
            supports_commands: true,
            supports_events: true,
            supports_binary_payloads: false,
            ordered_delivery: true,
            notes: vec!["injected Roblox agent connected over named pipes".to_string()],
        },
        composition: build_composition(),
    }
}

fn build_capabilities() -> AdvancedBackendCapabilities {
    AdvancedBackendCapabilities {
        injected_input: true,
        memory_observation: true,
        semantic_navigation: true,
        internal_interaction: true,
        ..Default::default()
    }
}

fn build_composition() -> AdvancedAgentComposition {
    AdvancedAgentComposition {
        roles: vec![
            winr_types::AdvancedAgentRole::MemoryObserver,
            winr_types::AdvancedAgentRole::InputShim,
            winr_types::AdvancedAgentRole::SemanticAdapter,
        ],
    }
}

fn load_bootstrap() -> Result<AgentBootstrapConfig, String> {
    let pid = unsafe { GetCurrentProcessId() };
    let path = bootstrap_path(pid);
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read bootstrap {}: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse bootstrap {}: {error}", path.display()))
}

fn bootstrap_path(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("winr-roblox-agent-{pid}.json"))
}

fn open_server_pipe(pipe_name: &str) -> Result<File, String> {
    let wide = wide_null(&format!(r"\\.\pipe\{pipe_name}"));
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            16 * 1024,
            16 * 1024,
            0,
            None,
        )
    };
    if handle.is_invalid() {
        return Err(format!(
            "CreateNamedPipeW returned an invalid handle for {pipe_name}"
        ));
    }

    unsafe {
        let _ = ConnectNamedPipe(handle, None);
        Ok(File::from_raw_handle(handle.0 as *mut c_void))
    }
}

fn capture_snapshot(
    schema: &RobloxMemorySchema,
    frame_id: u64,
) -> Result<RobloxObservationSnapshot, String> {
    let timestamp_ms = now_ms();
    let player_position = read_required_vec3(schema.player_position.as_ref(), "player_position")?;
    let player_velocity = read_optional_vec3(schema.player_velocity.as_ref());
    let camera_yaw_milli_degrees = read_optional_i32(schema.camera_yaw_milli_degrees.as_ref());
    let camera_pitch_milli_degrees = read_optional_i32(schema.camera_pitch_milli_degrees.as_ref());
    let prompt_visible = read_optional_bool(schema.prompt_visible.as_ref());
    let prompt_distance_millimeters =
        read_optional_u32(schema.prompt_distance_millimeters.as_ref());
    let mut objects = Vec::new();
    for object in &schema.objects {
        if let Ok(position_millimeters) = read_vec3(&object.position) {
            objects.push(RobloxObservedObject {
                id: object.id.clone(),
                label: object.label.clone(),
                kind: object.kind.clone(),
                interactable: object.interactable,
                position_millimeters,
            });
        }
    }

    Ok(RobloxObservationSnapshot {
        frame_id,
        source: "injected-memory".to_string(),
        detail: format!(
            "manual schema '{}' captured from injected agent",
            schema.schema_version
        ),
        timestamp_ms,
        freshness_ms: 16,
        player_position_millimeters: Some(player_position),
        player_velocity_millimeters: player_velocity,
        camera_yaw_milli_degrees,
        camera_pitch_milli_degrees,
        prompt_visible,
        prompt_distance_millimeters,
        objects,
    })
}

fn read_required_vec3(field: Option<&RobloxMemoryField>, label: &str) -> Result<[i32; 3], String> {
    let field = field.ok_or_else(|| format!("required schema field '{label}' is missing"))?;
    read_vec3(field)
}

fn read_optional_vec3(field: Option<&RobloxMemoryField>) -> Option<[i32; 3]> {
    field.and_then(|field| read_vec3(field).ok())
}

fn read_optional_i32(field: Option<&RobloxMemoryField>) -> Option<i32> {
    field.and_then(|field| read_i32(field).ok())
}

fn read_optional_u32(field: Option<&RobloxMemoryField>) -> Option<u32> {
    field.and_then(|field| read_u32(field).ok())
}

fn read_optional_bool(field: Option<&RobloxMemoryField>) -> Option<bool> {
    field.and_then(|field| read_bool(field).ok())
}

fn read_vec3(field: &RobloxMemoryField) -> Result<[i32; 3], String> {
    match field.value_kind {
        RobloxMemoryValueKind::Vec3F32 => {
            let bytes = read_field_bytes(field, 12)?;
            let x = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let z = f32::from_le_bytes(bytes[8..12].try_into().unwrap());
            Ok([
                (x * 1000.0).round() as i32,
                (y * 1000.0).round() as i32,
                (z * 1000.0).round() as i32,
            ])
        }
        RobloxMemoryValueKind::Vec3I32 => {
            let bytes = read_field_bytes(field, 12)?;
            Ok([
                i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ])
        }
        other => Err(format!("field expected vec3 but schema uses {other:?}")),
    }
}

fn read_i32(field: &RobloxMemoryField) -> Result<i32, String> {
    if field.value_kind != RobloxMemoryValueKind::I32 {
        return Err(format!(
            "field expected i32 but schema uses {:?}",
            field.value_kind
        ));
    }
    let bytes = read_field_bytes(field, 4)?;
    Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_u32(field: &RobloxMemoryField) -> Result<u32, String> {
    if field.value_kind != RobloxMemoryValueKind::U32 {
        return Err(format!(
            "field expected u32 but schema uses {:?}",
            field.value_kind
        ));
    }
    let bytes = read_field_bytes(field, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_bool(field: &RobloxMemoryField) -> Result<bool, String> {
    if field.value_kind != RobloxMemoryValueKind::U8Bool {
        return Err(format!(
            "field expected bool but schema uses {:?}",
            field.value_kind
        ));
    }
    let bytes = read_field_bytes(field, 1)?;
    Ok(bytes[0] != 0)
}

fn read_field_bytes(field: &RobloxMemoryField, len: usize) -> Result<Vec<u8>, String> {
    let module_base = module_base(&field.module)?;
    let mut address = module_base + field.base_offset;
    for (index, offset) in field.dereference_offsets.iter().enumerate() {
        let pointer = read_pointer(address)?;
        address = pointer + *offset;
        if address == 0 {
            return Err(format!(
                "null pointer while resolving '{}' at dereference step {}",
                field.module, index
            ));
        }
    }
    read_bytes(address, len)
}

fn module_base(module: &str) -> Result<usize, String> {
    let wide = wide_null(module);
    let handle = unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) }
        .map_err(|error| format!("GetModuleHandleW failed for module '{module}': {error}"))?;
    Ok(handle.0 as usize)
}

fn read_pointer(address: usize) -> Result<usize, String> {
    let size = std::mem::size_of::<usize>();
    let bytes = read_bytes(address, size)?;
    let pointer = if size == 8 {
        u64::from_le_bytes(bytes.try_into().unwrap()) as usize
    } else {
        u32::from_le_bytes(bytes.try_into().unwrap()) as usize
    };
    Ok(pointer)
}

fn read_bytes(address: usize, len: usize) -> Result<Vec<u8>, String> {
    if address == 0 {
        return Err("attempted to read null address".to_string());
    }
    let mut buffer = vec![0u8; len];
    unsafe {
        std::ptr::copy_nonoverlapping(address as *const u8, buffer.as_mut_ptr(), len);
    }
    Ok(buffer)
}

fn execute_input(target: &AdvancedTargetRef, action: InjectedInputAction) -> AdvancedHostResponse {
    match do_execute_input(target, action) {
        Ok(detail) => AdvancedHostResponse::InputOutcome {
            status: AdvancedCommandAckStatus::Completed,
            detail,
        },
        Err(detail) => AdvancedHostResponse::InputOutcome {
            status: AdvancedCommandAckStatus::Rejected,
            detail,
        },
    }
}

fn do_execute_input(
    target: &AdvancedTargetRef,
    action: InjectedInputAction,
) -> Result<String, String> {
    let hwnd_text = target
        .hwnd
        .as_deref()
        .ok_or_else(|| "target hwnd missing for injected input".to_string())?;
    let hwnd = parse_hwnd(hwnd_text)?;
    match action {
        InjectedInputAction::MoveForward { duration_ms } => {
            post_key(hwnd, 0x57, false)?;
            thread::sleep(Duration::from_millis(duration_ms));
            post_key(hwnd, 0x57, true)?;
            Ok(format!("posted move forward for {duration_ms} ms"))
        }
        InjectedInputAction::StopMotion => {
            for vk in [0x57, 0x41, 0x53, 0x44] {
                post_key(hwnd, vk, true)?;
            }
            Ok("posted stop motion".to_string())
        }
        InjectedInputAction::Turn {
            delta_yaw_milli_degrees,
        } => {
            let (vk, magnitude) = if delta_yaw_milli_degrees >= 0 {
                (0x27, delta_yaw_milli_degrees)
            } else {
                (0x25, -delta_yaw_milli_degrees)
            };
            let hold_ms = ((magnitude as u64) / 100).clamp(20, 300);
            post_key(hwnd, vk, false)?;
            thread::sleep(Duration::from_millis(hold_ms));
            post_key(hwnd, vk, true)?;
            Ok(format!("posted turn for {hold_ms} ms"))
        }
        InjectedInputAction::Interact => {
            post_key(hwnd, 0x45, false)?;
            post_key(hwnd, 0x45, true)?;
            Ok("posted interact".to_string())
        }
        InjectedInputAction::Jump => {
            post_key(hwnd, 0x20, false)?;
            post_key(hwnd, 0x20, true)?;
            Ok("posted jump".to_string())
        }
        InjectedInputAction::StrafeRight { duration_ms } => {
            post_key(hwnd, 0x44, false)?;
            thread::sleep(Duration::from_millis(duration_ms));
            post_key(hwnd, 0x44, true)?;
            Ok(format!("posted strafe right for {duration_ms} ms"))
        }
    }
}

fn post_key(hwnd: HWND, vk: u16, key_up: bool) -> Result<(), String> {
    let message = if key_up { WM_KEYUP } else { WM_KEYDOWN };
    unsafe { PostMessageW(Some(hwnd), message, WPARAM(vk as usize), LPARAM(0)) }
        .map_err(|error| format!("PostMessageW failed for vk=0x{vk:02X}: {error}"))
}

fn parse_hwnd(text: &str) -> Result<HWND, String> {
    let trimmed = text.trim_start_matches("0x");
    let raw = usize::from_str_radix(trimmed, 16)
        .map_err(|error| format!("failed to parse hwnd '{text}': {error}"))?;
    Ok(HWND(raw as *mut c_void))
}

fn write_json_frame<T: Serialize>(file: &mut File, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("serialize pipe frame failed: {error}"))?;
    let len = bytes.len() as u32;
    file.write_all(&len.to_le_bytes())
        .and_then(|_| file.write_all(&bytes))
        .and_then(|_| file.flush())
        .map_err(|error| format!("write pipe frame failed: {error}"))
}

fn read_json_frame<T: for<'de> Deserialize<'de>>(file: &mut File) -> Result<T, String> {
    let mut len_bytes = [0u8; 4];
    file.read_exact(&mut len_bytes)
        .map_err(|error| format!("read pipe frame length failed: {error}"))?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("read pipe frame payload failed: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("deserialize pipe frame failed: {error}"))
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
