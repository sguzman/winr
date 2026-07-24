use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::{
                Debug::ReadProcessMemory,
                ToolHelp::{
                    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW,
                    TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
                },
            },
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Memory::{
                MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
            },
            Threading::{
                CreateRemoteThread, GetExitCodeThread, OpenProcess, PROCESS_CREATE_THREAD,
                PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
                WaitForSingleObject,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
                KEYEVENTF_KEYUP, MOUSE_EVENT_FLAGS, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
                VIRTUAL_KEY,
            },
            WindowsAndMessaging::GetForegroundWindow,
        },
    },
    core::{PCSTR, PCWSTR},
};
use winr_perception::{
    CameraHints, EntityKind, MemoryCameraState, MemoryObjectState, MemoryObservationDetails,
    MemoryObservationUseCase, MemoryPlayerState, MemoryPromptState, MemorySchemaVersion,
    ObservationCaptureContext, ObservationEntity, ObservationFrame, ObservationMovementState,
    ObservationSourceData, ObservationStateField, PlayerStateHints, WorldModelTracker,
};
use winr_types::{
    AdvancedCommandAckStatus, AdvancedExecutionReason, AdvancedFrontend, AdvancedHostCommand,
    AdvancedHostResponse, AdvancedProfileBackend, AdvancedTargetRef, InjectedInputAction,
    LiveObservationSummary, LiveSessionInspection, ProfileConfig, ProfileRunResult,
    RobloxAdvancedConfig, RobloxMemoryField, RobloxMemorySchema, RobloxMemoryValueKind,
    RobloxObservationSnapshot, RobloxPatrolRegionConfig, WindowInfo, WinrError, WinrResult,
};
use winr_workflows::{
    AppPackMovementTuning, BoundedRegionPatrolController, ControllerMemory, NavigationContext,
    NavigationController, NavigationControllerConfig, ProgressSample, SemanticInputAction,
    SemanticInputTarget, WorkflowExecutionTrace, WorkflowTraceEventKind, load_app_pack_from_dir,
};

use crate::{
    AdvancedAgentTransport, AdvancedBackendSession, AttachmentSupervisor, NamedPipeAgentTransport,
    prepare_profile_backend_for_frontend,
};

const ROBLOX_PACK_DIR: &str = "../../packs/roblox";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveRobloxRunOptions {
    pub poll_interval: Duration,
    pub max_steps: Option<u64>,
}

pub fn load_roblox_memory_schema(path: &Path) -> WinrResult<RobloxMemorySchema> {
    let text = fs::read_to_string(path).map_err(|error| WinrError::Unsupported {
        message: format!(
            "failed to read Roblox memory schema {}: {error}",
            path.display()
        ),
    })?;
    toml::from_str(&text).map_err(|error| WinrError::Unsupported {
        message: format!(
            "failed to parse Roblox memory schema {}: {error}",
            path.display()
        ),
    })
}

pub fn inspect_live_roblox_session(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
) -> WinrResult<LiveSessionInspection> {
    let mut runtime = RobloxLiveRuntime::attach(profile, frontend)?;
    runtime.inspect()
}

pub fn run_live_roblox_workflow<F, G>(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
    options: LiveRobloxRunOptions,
    mut on_step: F,
    mut should_stop: G,
) -> WinrResult<ProfileRunResult>
where
    F: FnMut(u64, &LiveSessionInspection),
    G: FnMut() -> bool,
{
    let mut runtime = RobloxLiveRuntime::attach(profile, frontend)?;
    let max_steps = options.max_steps.unwrap_or(64);
    let mut executed_steps = 0_u64;

    while executed_steps < max_steps {
        if should_stop() {
            runtime.session.record_reasoning(
                "workflow stopped by caller",
                vec!["stop signal received".to_string()],
            );
            break;
        }

        let inspection = runtime.step()?;
        executed_steps += 1;
        on_step(executed_steps, &inspection);

        if runtime.last_command_rejected() {
            break;
        }

        thread::sleep(options.poll_interval);
    }

    Ok(ProfileRunResult {
        profile_id: profile.profile.id.clone(),
        profile_name: profile.profile.name.clone(),
        clicks_fired: executed_steps,
        backend_used: AdvancedProfileBackend::Inject,
        target_window: runtime.target_window.clone(),
    })
}

trait ProcessMemory {
    fn module_base(&self, module_name: &str) -> Result<usize, String>;
    fn read_bytes(&self, address: usize, len: usize) -> Result<Vec<u8>, String>;
}

struct WindowsProcessMemory {
    handle: HANDLE,
    pid: u32,
}

impl WindowsProcessMemory {
    fn open(pid: u32) -> Result<Self, String> {
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) }
                .map_err(|error| format!("OpenProcess failed for pid {pid}: {error}"))?;

        Ok(Self { handle, pid })
    }
}

impl Drop for WindowsProcessMemory {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl ProcessMemory for WindowsProcessMemory {
    fn module_base(&self, module_name: &str) -> Result<usize, String> {
        let snapshot =
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid) }
                .map_err(|error| format!("CreateToolhelp32Snapshot failed: {error}"))?;

        let mut entry = MODULEENTRY32W {
            dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = None;
        if unsafe { Module32FirstW(snapshot, &mut entry) }.is_ok() {
            loop {
                let name_len = entry
                    .szModule
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(entry.szModule.len());
                let name = String::from_utf16_lossy(&entry.szModule[..name_len]);
                if name.eq_ignore_ascii_case(module_name) {
                    found = Some(entry.modBaseAddr as usize);
                    break;
                }
                if unsafe { Module32NextW(snapshot, &mut entry) }.is_err() {
                    break;
                }
            }
        }

        unsafe {
            let _ = CloseHandle(snapshot);
        }

        found.ok_or_else(|| format!("module '{module_name}' not found in pid {}", self.pid))
    }

    fn read_bytes(&self, address: usize, len: usize) -> Result<Vec<u8>, String> {
        let mut buffer = vec![0_u8; len];
        let mut read = 0;
        unsafe {
            ReadProcessMemory(
                self.handle,
                address as *const _,
                buffer.as_mut_ptr() as *mut _,
                len,
                Some(&mut read),
            )
        }
        .map_err(|error| format!("ReadProcessMemory failed at 0x{address:X}: {error}"))?;
        if read != len {
            return Err(format!(
                "short read at 0x{address:X}: expected {len} bytes, got {read}"
            ));
        }
        Ok(buffer)
    }
}

struct RobloxMemoryObserver<'a, TMemory> {
    target: AdvancedTargetRef,
    schema: &'a RobloxMemorySchema,
    memory: TMemory,
}

impl<'a, TMemory> RobloxMemoryObserver<'a, TMemory>
where
    TMemory: ProcessMemory,
{
    fn read_frame(
        &self,
        context: &ObservationCaptureContext,
        patrol: &RobloxPatrolRegionConfig,
    ) -> Result<ObservationFrame, String> {
        let player_position = self
            .schema
            .player_position
            .as_ref()
            .map(|field| self.read_vec3_millimeters(field))
            .transpose()?;
        let player_velocity = self
            .schema
            .player_velocity
            .as_ref()
            .map(|field| self.read_vec3_millimeters(field))
            .transpose()?;
        let camera_yaw = self
            .schema
            .camera_yaw_milli_degrees
            .as_ref()
            .map(|field| self.read_i32(field))
            .transpose()?;
        let camera_pitch = self
            .schema
            .camera_pitch_milli_degrees
            .as_ref()
            .map(|field| self.read_i32(field))
            .transpose()?;
        let prompt_visible = self
            .schema
            .prompt_visible
            .as_ref()
            .map(|field| self.read_bool(field))
            .transpose()?;
        let prompt_distance = self
            .schema
            .prompt_distance_millimeters
            .as_ref()
            .map(|field| self.read_u32(field))
            .transpose()?;

        let mut nearby_objects = Vec::new();
        let mut entities = Vec::new();

        if let Some(position) = player_position {
            entities.push(ObservationEntity {
                id: "player".to_string(),
                kind: EntityKind::Player,
                label: "Local Player".to_string(),
                confidence: 1.0,
                tags: vec![format!(
                    "world_mm:{},{},{}",
                    position[0], position[1], position[2]
                )],
            });
        }

        let region_distance = player_position
            .map(|position| euclidean_distance_millimeters(position, patrol.anchor_millimeters));
        entities.push(ObservationEntity {
            id: "live-patrol-region".to_string(),
            kind: EntityKind::Region,
            label: "Live Patrol Region".to_string(),
            confidence: 1.0,
            tags: vec![
                format!(
                    "world_mm:{},{},{}",
                    patrol.anchor_millimeters[0],
                    patrol.anchor_millimeters[1],
                    patrol.anchor_millimeters[2]
                ),
                format!("radius_mm:{}", patrol.radius_millimeters),
                format!("distance_mm:{}", region_distance.unwrap_or(0)),
            ],
        });

        for (index, waypoint) in patrol_waypoints(patrol).into_iter().enumerate() {
            let distance = player_position
                .map(|position| euclidean_distance_millimeters(position, waypoint))
                .unwrap_or(0);
            entities.push(ObservationEntity {
                id: format!("live-patrol-waypoint-{index}"),
                kind: EntityKind::Waypoint,
                label: format!("Patrol Waypoint {}", index + 1),
                confidence: 0.9,
                tags: vec![
                    format!("world_mm:{},{},{}", waypoint[0], waypoint[1], waypoint[2]),
                    format!("distance_mm:{distance}"),
                ],
            });
        }

        for object in &self.schema.objects {
            let position = self.read_vec3_millimeters(&object.position)?;
            let distance = player_position
                .map(|player| euclidean_distance_millimeters(player, position))
                .unwrap_or(0);
            nearby_objects.push(MemoryObjectState {
                id: object.id.clone(),
                kind: object.kind.clone(),
                label: object.label.clone(),
                world_position_millimeters: Some(position),
                distance_millimeters: Some(distance),
                interactable: object.interactable,
            });
            entities.push(ObservationEntity {
                id: object.id.clone(),
                kind: if object.interactable {
                    EntityKind::Interactable
                } else {
                    EntityKind::VisualMarker
                },
                label: object.label.clone(),
                confidence: 0.85,
                tags: vec![
                    format!("world_mm:{},{},{}", position[0], position[1], position[2]),
                    format!("distance_mm:{distance}"),
                    format!("kind:{}", object.kind),
                ],
            });
        }

        let mut prompts = Vec::new();
        if prompt_visible.unwrap_or(false) {
            let distance = prompt_distance;
            prompts.push(MemoryPromptState {
                id: "live-prompt".to_string(),
                label: "Interact Prompt".to_string(),
                visible: true,
                distance_millimeters: distance,
            });
            entities.push(ObservationEntity {
                id: "live-prompt".to_string(),
                kind: EntityKind::Prompt,
                label: "Interact Prompt".to_string(),
                confidence: 0.95,
                tags: vec![format!("distance_mm:{}", distance.unwrap_or(0))],
            });
        }

        let detail = format!(
            "live Roblox memory snapshot build={} schema={}",
            self.schema.game_build, self.schema.schema_version
        );
        let mut frame = ObservationFrame::from_update(
            context.clone(),
            winr_types::AdvancedObservationUpdate {
                frame_id: context.frame_id,
                source: "roblox-memory".to_string(),
                detail: detail.clone(),
                timestamp_ms: Some(context.timestamp_ms),
                freshness_ms: Some(context.freshness_ms),
                payload: None,
            },
            ObservationSourceData::MemoryState {
                snapshot_id: format!(
                    "roblox-live-{}",
                    self.target
                        .pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                state_fields: vec![
                    ObservationStateField {
                        key: "schema.game_build".to_string(),
                        value: self.schema.game_build.clone(),
                    },
                    ObservationStateField {
                        key: "schema.version".to_string(),
                        value: self.schema.schema_version.clone(),
                    },
                ],
            },
        )
        .with_memory_details(MemoryObservationDetails {
            schema_version: MemorySchemaVersion::V1,
            snapshot_id: "roblox-live".to_string(),
            intended_uses: vec![
                MemoryObservationUseCase::PlayerState,
                MemoryObservationUseCase::CameraState,
                MemoryObservationUseCase::PromptState,
                MemoryObservationUseCase::InteractableDiscovery,
            ],
            player_state: Some(MemoryPlayerState {
                world_position_millimeters: player_position,
                velocity_millimeters_per_second: player_velocity,
                movement_state: Some(if player_velocity.is_some() {
                    ObservationMovementState::Walking
                } else {
                    ObservationMovementState::Unknown
                }),
                active_tool: None,
                active_modes: vec!["live".to_string(), "roblox".to_string()],
            }),
            camera_state: Some(MemoryCameraState {
                yaw_milli_degrees: camera_yaw,
                pitch_milli_degrees: camera_pitch,
                field_of_view_milli_degrees: None,
                mode: Some("third_person".to_string()),
            }),
            prompts,
            nearby_objects,
            raw_layout_hidden: true,
        });

        frame.player_state_hints = Some(PlayerStateHints {
            world_position: player_position.map(|value| {
                [
                    value[0] as f32 / 1000.0,
                    value[1] as f32 / 1000.0,
                    value[2] as f32 / 1000.0,
                ]
            }),
            velocity: player_velocity.map(|value| {
                [
                    value[0] as f32 / 1000.0,
                    value[1] as f32 / 1000.0,
                    value[2] as f32 / 1000.0,
                ]
            }),
            health_percent: Some(1.0),
            movement_state: if player_velocity.is_some() {
                ObservationMovementState::Walking
            } else {
                ObservationMovementState::Unknown
            },
            active_modes: vec!["live".to_string()],
        });
        frame.camera_hints = Some(CameraHints {
            yaw_degrees: camera_yaw.map(|value| value as f32 / 1000.0),
            pitch_degrees: camera_pitch.map(|value| value as f32 / 1000.0),
            field_of_view_degrees: None,
            camera_mode: Some("third_person".to_string()),
        });
        frame.entities = entities;
        frame.notes.push(detail);
        if let Some(yaw) = camera_yaw {
            frame.notes.push(format!("camera_yaw_md:{yaw}"));
        }
        if let Some(player) = player_position {
            let target_yaw = yaw_to_target_milli_degrees(player, patrol.anchor_millimeters);
            frame.notes.push(format!("target_yaw_md:{target_yaw}"));
        }
        Ok(frame.with_confidence_summary(0.95))
    }

    fn read_vec3_millimeters(&self, field: &RobloxMemoryField) -> Result<[i32; 3], String> {
        let address = self.resolve_field_address(field)?;
        match field.value_kind {
            RobloxMemoryValueKind::Vec3F32 => {
                let bytes = self.memory.read_bytes(address, 12)?;
                let mut values = [0_i32; 3];
                for (index, chunk) in bytes.chunks_exact(4).enumerate() {
                    let value =
                        f32::from_le_bytes(chunk.try_into().expect("vec3 chunk must be 4 bytes"));
                    values[index] = (value * 1000.0).round() as i32;
                }
                Ok(values)
            }
            RobloxMemoryValueKind::Vec3I32 => {
                let bytes = self.memory.read_bytes(address, 12)?;
                let mut values = [0_i32; 3];
                for (index, chunk) in bytes.chunks_exact(4).enumerate() {
                    values[index] =
                        i32::from_le_bytes(chunk.try_into().expect("vec3 chunk must be 4 bytes"));
                }
                Ok(values)
            }
            other => Err(format!("field {:?} is not a vec3 field", other)),
        }
    }

    fn read_i32(&self, field: &RobloxMemoryField) -> Result<i32, String> {
        let address = self.resolve_field_address(field)?;
        let bytes = self.memory.read_bytes(address, 4)?;
        Ok(i32::from_le_bytes(bytes.try_into().expect("i32 bytes")))
    }

    fn read_u32(&self, field: &RobloxMemoryField) -> Result<u32, String> {
        let address = self.resolve_field_address(field)?;
        let bytes = self.memory.read_bytes(address, 4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("u32 bytes")))
    }

    fn read_bool(&self, field: &RobloxMemoryField) -> Result<bool, String> {
        let address = self.resolve_field_address(field)?;
        let bytes = self.memory.read_bytes(address, 1)?;
        Ok(bytes[0] != 0)
    }

    fn resolve_field_address(&self, field: &RobloxMemoryField) -> Result<usize, String> {
        let mut cursor = self.memory.module_base(&field.module)? + field.base_offset;
        for offset in &field.dereference_offsets {
            let bytes = self
                .memory
                .read_bytes(cursor, std::mem::size_of::<usize>())?;
            let next = usize::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| format!("invalid pointer size at 0x{cursor:X}"))?,
            );
            cursor = next + *offset;
        }
        Ok(cursor)
    }
}

struct RobloxLiveRuntime {
    session: AdvancedBackendSession,
    attachment: AttachmentSupervisor,
    target_window: WindowInfo,
    advanced: RobloxAdvancedConfig,
    transport: NamedPipeAgentTransport,
    tracker: WorldModelTracker,
    trace: WorkflowExecutionTrace,
    controller_memory: ControllerMemory,
    step_index: u64,
}

impl RobloxLiveRuntime {
    fn attach(profile: &ProfileConfig, frontend: AdvancedFrontend) -> WinrResult<Self> {
        let advanced = profile
            .advanced
            .as_ref()
            .and_then(|advanced| advanced.roblox.clone())
            .ok_or_else(|| WinrError::Unsupported {
                message: "live Roblox workflow requires [advanced.roblox] config".to_string(),
            })?;
        let workflow = profile
            .workflow
            .as_ref()
            .ok_or_else(|| WinrError::Unsupported {
                message: "live Roblox workflow requires [workflow] config".to_string(),
            })?;
        if !workflow.task.eq_ignore_ascii_case("patrol_region") {
            return Err(WinrError::Unsupported {
                message: format!(
                    "only patrol_region is implemented for live Roblox workflows today, got '{}'",
                    workflow.task
                ),
            });
        }

        let mut session = prepare_profile_backend_for_frontend(profile, frontend)?;
        let (attachment, _) = AttachmentSupervisor::attach(
            &profile.target,
            winr_types::AdvancedAttachmentPolicy::default(),
        )?;
        let target_window = attachable_target_into_window_info(&attachment.attachment.target);
        let schema_path = PathBuf::from(&advanced.memory_schema_path);
        let schema = load_roblox_memory_schema(&schema_path)?;
        validate_live_schema(&schema)?;
        let transport = bootstrap_injected_agent(
            &mut session,
            &attachment.attachment.target.target,
            &schema_path,
        )?;

        Ok(Self {
            session,
            attachment,
            target_window,
            advanced,
            transport,
            tracker: WorldModelTracker::default(),
            trace: WorkflowExecutionTrace::default(),
            controller_memory: ControllerMemory::default(),
            step_index: 0,
        })
    }

    fn inspect(&mut self) -> WinrResult<LiveSessionInspection> {
        let frame = self.capture_frame()?;
        Ok(self.build_inspection(Some(frame)))
    }

    fn step(&mut self) -> WinrResult<LiveSessionInspection> {
        let frame = self.capture_frame()?;
        self.trace.push(
            WorkflowTraceEventKind::ObservationAccepted,
            format!(
                "accepted live observation frame {}",
                frame.metadata.frame_id
            ),
        );
        let world_model =
            self.tracker
                .model
                .as_ref()
                .cloned()
                .ok_or_else(|| WinrError::Unsupported {
                    message: "world model was not initialized from live observation".to_string(),
                })?;

        let waypoints = rotated_waypoint_ids(self.step_index);
        let patrol = BoundedRegionPatrolController {
            region_entity_id: "live-patrol-region".to_string(),
            waypoint_entity_ids: waypoints,
        };
        let config = self.navigation_config()?;
        self.controller_memory
            .progress_samples
            .push(ProgressSample {
                frame_id: frame.metadata.frame_id,
                player_position_millimeters: frame
                    .memory_details
                    .as_ref()
                    .and_then(|details| details.player_state.as_ref())
                    .and_then(|state| state.world_position_millimeters),
                target_distance_millimeters: world_model.best_entity(EntityKind::Region).and_then(
                    |entity| {
                        entity
                            .entity
                            .tags
                            .iter()
                            .find_map(|tag| tag.strip_prefix("distance_mm:"))
                            .and_then(|value| value.parse::<u32>().ok())
                    },
                ),
            });
        if self.controller_memory.progress_samples.len() > 8 {
            self.controller_memory.progress_samples.remove(0);
        }

        let context = NavigationContext {
            world_model,
            frame_id: frame.metadata.frame_id,
            controller_memory: self.controller_memory.clone(),
        };
        let decision = patrol.decide(&context, &config);
        self.trace.push(
            WorkflowTraceEventKind::TaskSelected,
            format!("navigation decision: {}", decision.detail),
        );
        let mut basis = vec![decision.detail.clone()];

        for action in decision.actions {
            let expanded = self.expand_action(&action, &context, &config);
            for command in expanded {
                let outcome = self.execute_action(&command)?;
                basis.push(outcome.clone());
                self.trace
                    .push(WorkflowTraceEventKind::IntentIssued, outcome);
            }
        }

        if decision.kind == winr_workflows::NavigationDecisionKind::Recovering {
            self.trace.push(
                WorkflowTraceEventKind::RecoveryTriggered,
                "navigation controller entered recovery".to_string(),
            );
        }

        self.session.record_reasoning(
            format!("live Roblox patrol step {} evaluated", self.step_index + 1),
            basis,
        );
        self.step_index += 1;

        Ok(self.build_inspection(Some(frame)))
    }

    fn capture_frame(&mut self) -> WinrResult<ObservationFrame> {
        self.transport.send_command(
            self.session
                .command(AdvancedHostCommand::FetchObservations { max_items: 1 }),
        )?;
        let response = self
            .transport
            .recv_response()?
            .ok_or_else(|| WinrError::Unsupported {
                message: "injected agent did not return an observation response".to_string(),
            })?;
        self.session.apply_response(&response)?;
        while let Some(event) = self.transport.recv_event()? {
            self.session.apply_event(&event)?;
        }
        let snapshot = match response.response {
            AdvancedHostResponse::RobloxObservations { snapshots, .. } => snapshots
                .into_iter()
                .next()
                .ok_or_else(|| WinrError::Unsupported {
                    message: "injected agent returned an empty Roblox observation batch"
                        .to_string(),
                })?,
            other => {
                return Err(WinrError::Unsupported {
                    message: format!(
                        "injected agent returned an unexpected observation response: {other:?}"
                    ),
                });
            }
        };
        let frame = frame_from_snapshot(
            self.attachment.attachment.target.target.clone(),
            &self.advanced.patrol_region,
            snapshot,
        );
        self.tracker.update(&frame);
        Ok(frame)
    }

    fn navigation_config(&self) -> WinrResult<NavigationControllerConfig> {
        let pack_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROBLOX_PACK_DIR);
        let pack = load_app_pack_from_dir(&pack_dir).map_err(|error| WinrError::Unsupported {
            message: format!("failed to load Roblox pack for live workflow: {error}"),
        })?;
        Ok(navigation_config_from_pack(&pack.movement_tuning))
    }

    fn expand_action(
        &self,
        action: &SemanticInputAction,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> Vec<SemanticInputAction> {
        match action {
            SemanticInputAction::WalkTo { target } | SemanticInputAction::Approach { target } => {
                let Some(target_world) = target_world_position(target, context) else {
                    return vec![SemanticInputAction::StopMotion];
                };
                let player = context
                    .controller_memory
                    .progress_samples
                    .last()
                    .and_then(|sample| sample.player_position_millimeters)
                    .unwrap_or(self.advanced.patrol_region.anchor_millimeters);
                let desired_yaw = yaw_to_target_milli_degrees(player, target_world);
                let current_yaw = context
                    .world_model
                    .notes
                    .iter()
                    .find_map(|note| note.strip_prefix("camera_yaw_md:"))
                    .and_then(|value| value.parse::<i32>().ok())
                    .unwrap_or(desired_yaw);
                let turn_delta = desired_yaw - current_yaw;
                let mut actions = Vec::new();
                if turn_delta.unsigned_abs() > config.heading_tolerance_milli_degrees as u32 {
                    actions.push(SemanticInputAction::Turn {
                        delta_yaw_milli_degrees: turn_delta.clamp(
                            -config.turn_step_milli_degrees,
                            config.turn_step_milli_degrees,
                        ),
                    });
                }
                actions.push(SemanticInputAction::MoveForward {
                    duration_ms: config.move_step_ms,
                });
                actions
            }
            other => vec![other.clone()],
        }
    }

    fn execute_action(&mut self, action: &SemanticInputAction) -> WinrResult<String> {
        if let Some(mapped) = map_injected_input_action(action) {
            self.transport.send_command(
                self.session
                    .command(AdvancedHostCommand::ExecuteInput { action: mapped }),
            )?;
            let response =
                self.transport
                    .recv_response()?
                    .ok_or_else(|| WinrError::Unsupported {
                        message: "injected agent did not return an input outcome".to_string(),
                    })?;
            self.session.apply_response(&response)?;
            while let Some(event) = self.transport.recv_event()? {
                self.session.apply_event(&event)?;
            }
            return match response.response {
                AdvancedHostResponse::InputOutcome { status, detail } => {
                    if matches!(
                        status,
                        AdvancedCommandAckStatus::Rejected | AdvancedCommandAckStatus::TimedOut
                    ) {
                        Err(WinrError::Unsupported { message: detail })
                    } else {
                        Ok(detail)
                    }
                }
                other => Err(WinrError::Unsupported {
                    message: format!(
                        "injected agent returned an unexpected input response: {other:?}"
                    ),
                }),
            };
        }

        if self.advanced.allow_foreground_fallback && target_is_foreground(&self.target_window) {
            return match execute_foreground_fallback(action) {
                Ok(detail) => {
                    self.session.record_command_outcome(
                        format!("foreground_fallback::{:?}", action),
                        AdvancedCommandAckStatus::Completed,
                        detail.clone(),
                    );
                    Ok(detail)
                }
                Err(error) => {
                    self.session.record_command_outcome(
                        format!("foreground_fallback::{:?}", action),
                        AdvancedCommandAckStatus::Rejected,
                        error.clone(),
                    );
                    Err(WinrError::Unsupported { message: error })
                }
            };
        }

        let detail =
            "live Roblox workflow does not have an injected mapping for this action".to_string();
        self.session.record_command_outcome(
            format!("roblox_input::{:?}", action),
            AdvancedCommandAckStatus::Rejected,
            detail.clone(),
        );
        Err(WinrError::Unsupported { message: detail })
    }

    fn build_inspection(&self, frame: Option<ObservationFrame>) -> LiveSessionInspection {
        let summary = frame.as_ref().map(|frame| {
            let player_position = frame
                .memory_details
                .as_ref()
                .and_then(|details| details.player_state.as_ref())
                .and_then(|state| state.world_position_millimeters);
            let prompt_visible = frame
                .memory_details
                .as_ref()
                .map(|details| details.prompts.iter().any(|prompt| prompt.visible));
            LiveObservationSummary {
                frame_id: frame.metadata.frame_id,
                source: frame_source_name(frame),
                freshness_ms: frame.metadata.freshness_ms,
                player_position_millimeters: player_position,
                patrol_region_anchor_millimeters: Some(
                    self.advanced.patrol_region.anchor_millimeters,
                ),
                patrol_region_radius_millimeters: Some(
                    self.advanced.patrol_region.radius_millimeters,
                ),
                prompt_visible,
                entities: frame
                    .entities
                    .iter()
                    .map(|entity| entity.id.clone())
                    .collect(),
            }
        });

        LiveSessionInspection {
            workflow_id: "region-patrol-live".to_string(),
            session_id: self.session.session_id,
            lifecycle_state: self.session.hello.lifecycle_state,
            backend: self.session.hello.backend,
            attachment_status: self.attachment.attachment.health.status,
            observation: summary,
            recent_events: self
                .session
                .structured_events
                .iter()
                .rev()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            reasoning: self
                .session
                .recent_reason
                .clone()
                .or_else(|| trace_reasoning(&self.trace)),
            last_rejected_command: self
                .session
                .command_records
                .iter()
                .rev()
                .find(|record| record.status == AdvancedCommandAckStatus::Rejected)
                .cloned(),
        }
    }

    fn last_command_rejected(&self) -> bool {
        self.session
            .command_records
            .last()
            .is_some_and(|record| record.status == AdvancedCommandAckStatus::Rejected)
    }
}

fn attachable_target_into_window_info(target: &winr_types::AdvancedAttachableTarget) -> WindowInfo {
    WindowInfo {
        hwnd: target
            .target
            .hwnd
            .clone()
            .unwrap_or_else(|| "0x0000000000000000".to_string()),
        pid: target.target.pid.unwrap_or_default(),
        title: target.title.clone(),
        class_name: target.class_name.clone(),
        exe: target.exe.clone(),
        visible: target.visible,
        minimized: target.minimized,
        foreground: target.foreground,
        rect: winr_types::Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
    }
}

fn patrol_waypoints(region: &RobloxPatrolRegionConfig) -> Vec<[i32; 3]> {
    let [x, y, z] = region.anchor_millimeters;
    let radius = region.radius_millimeters as i32;
    vec![
        [x + radius, y, z],
        [x, y, z + radius],
        [x - radius, y, z],
        [x, y, z - radius],
    ]
}

fn rotated_waypoint_ids(step_index: u64) -> Vec<String> {
    let mut ids = vec![
        "live-patrol-waypoint-0".to_string(),
        "live-patrol-waypoint-1".to_string(),
        "live-patrol-waypoint-2".to_string(),
        "live-patrol-waypoint-3".to_string(),
    ];
    let len = ids.len();
    ids.rotate_left((step_index as usize) % len);
    ids
}

fn navigation_config_from_pack(tuning: &AppPackMovementTuning) -> NavigationControllerConfig {
    NavigationControllerConfig {
        arrival_threshold_millimeters: tuning.arrival_threshold_millimeters,
        heading_tolerance_milli_degrees: 5_000,
        move_step_ms: tuning.move_step_ms,
        turn_step_milli_degrees: tuning.turn_step_milli_degrees,
        stuck_distance_epsilon_millimeters: 80,
        stuck_frame_window: tuning.stuck_frame_window,
    }
}

fn trace_reasoning(trace: &WorkflowExecutionTrace) -> Option<AdvancedExecutionReason> {
    trace.reasoning()
}

fn frame_source_name(frame: &ObservationFrame) -> String {
    match frame.metadata.source {
        winr_perception::ObservationSourceKind::DesktopScreenshot => "desktop".to_string(),
        winr_perception::ObservationSourceKind::RenderHookFrame => "render-hook".to_string(),
        winr_perception::ObservationSourceKind::MemoryState => "memory".to_string(),
        winr_perception::ObservationSourceKind::DetectorOverlay => "overlay".to_string(),
    }
}

fn target_world_position(
    target: &SemanticInputTarget,
    context: &NavigationContext,
) -> Option<[i32; 3]> {
    match target {
        SemanticInputTarget::CurrentTarget => context
            .best_entity_by_kind(EntityKind::Interactable)
            .and_then(entity_world_position),
        SemanticInputTarget::EntityId { entity_id } => context
            .world_model
            .entities
            .iter()
            .find(|entity| entity.entity.id == *entity_id)
            .and_then(entity_world_position),
        SemanticInputTarget::RegionId { .. } => context
            .world_model
            .best_entity(EntityKind::Region)
            .and_then(entity_world_position),
    }
}

fn entity_world_position(entity: &winr_perception::TrackedObservationEntity) -> Option<[i32; 3]> {
    entity
        .entity
        .tags
        .iter()
        .find_map(|tag| tag.strip_prefix("world_mm:"))
        .and_then(parse_world_mm_tag)
}

fn parse_world_mm_tag(value: &str) -> Option<[i32; 3]> {
    let mut parts = value.split(',');
    Some([
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ])
}

fn euclidean_distance_millimeters(left: [i32; 3], right: [i32; 3]) -> u32 {
    let dx = (left[0] - right[0]) as f64;
    let dy = (left[1] - right[1]) as f64;
    let dz = (left[2] - right[2]) as f64;
    ((dx * dx + dy * dy + dz * dz).sqrt().round()) as u32
}

fn yaw_to_target_milli_degrees(from: [i32; 3], to: [i32; 3]) -> i32 {
    let dx = (to[0] - from[0]) as f64;
    let dz = (to[2] - from[2]) as f64;
    (dx.atan2(dz).to_degrees() * 1000.0).round() as i32
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RobloxAgentBootstrapConfig {
    session_id: u64,
    command_pipe_name: String,
    event_pipe_name: String,
    schema_path: String,
    target: AdvancedTargetRef,
}

fn validate_live_schema(schema: &RobloxMemorySchema) -> WinrResult<()> {
    let Some(player_position) = schema.player_position.as_ref() else {
        return Err(WinrError::Unsupported {
            message: "manual schema required: player_position is missing".to_string(),
        });
    };

    let looks_unedited = player_position.base_offset == 0
        && player_position.dereference_offsets.is_empty()
        && schema.objects.iter().all(|object| {
            object.position.base_offset == 0 && object.position.dereference_offsets.is_empty()
        });
    if looks_unedited {
        return Err(WinrError::Unsupported {
            message:
                "manual schema required: the Roblox live schema still appears to be the placeholder example".to_string(),
        });
    }

    Ok(())
}

fn bootstrap_injected_agent(
    session: &mut AdvancedBackendSession,
    target: &AdvancedTargetRef,
    schema_path: &Path,
) -> WinrResult<NamedPipeAgentTransport> {
    let pid = target.pid.ok_or_else(|| WinrError::Unsupported {
        message: "injected Roblox workflow requires a target pid".to_string(),
    })?;
    let command_pipe_name = format!("winr-roblox-cmd-{pid}-{}", session.session_id.0);
    let event_pipe_name = format!("winr-roblox-evt-{pid}-{}", session.session_id.0);
    let bootstrap = RobloxAgentBootstrapConfig {
        session_id: session.session_id.0,
        command_pipe_name: command_pipe_name.clone(),
        event_pipe_name: event_pipe_name.clone(),
        schema_path: schema_path
            .canonicalize()
            .unwrap_or_else(|_| schema_path.to_path_buf())
            .display()
            .to_string(),
        target: target.clone(),
    };
    let bootstrap_path = write_agent_bootstrap_file(pid, &bootstrap)?;
    let dll_path = resolve_agent_dll_path()?;
    inject_agent_dll(pid, &dll_path)?;
    let mut transport = NamedPipeAgentTransport::connect(
        &command_pipe_name,
        &event_pipe_name,
        Duration::from_secs(5),
    )?;

    transport.send_command(session.handshake_command())?;
    let response = transport
        .recv_response()?
        .ok_or_else(|| WinrError::Unsupported {
            message: "injected agent did not respond to handshake".to_string(),
        })?;
    session.apply_response(&response)?;
    while let Some(event) = transport.recv_event()? {
        session.apply_event(&event)?;
    }

    transport.send_command(session.command(AdvancedHostCommand::GetCapabilities))?;
    let response = transport
        .recv_response()?
        .ok_or_else(|| WinrError::Unsupported {
            message: "injected agent did not return capabilities".to_string(),
        })?;
    session.apply_response(&response)?;

    transport.send_command(session.command(AdvancedHostCommand::SubscribeEvents))?;
    let response = transport
        .recv_response()?
        .ok_or_else(|| WinrError::Unsupported {
            message: "injected agent did not confirm event subscription".to_string(),
        })?;
    session.apply_response(&response)?;

    let _ = fs::remove_file(bootstrap_path);
    Ok(transport)
}

fn write_agent_bootstrap_file(
    pid: u32,
    bootstrap: &RobloxAgentBootstrapConfig,
) -> WinrResult<PathBuf> {
    let path = std::env::temp_dir().join(format!("winr-roblox-agent-{pid}.json"));
    let raw = serde_json::to_string_pretty(bootstrap).map_err(|error| WinrError::Unsupported {
        message: format!("failed to serialize injected Roblox bootstrap config: {error}"),
    })?;
    fs::write(&path, raw).map_err(|error| WinrError::Unsupported {
        message: format!(
            "failed to write injected Roblox bootstrap {}: {error}",
            path.display()
        ),
    })?;
    Ok(path)
}

fn resolve_agent_dll_path() -> WinrResult<PathBuf> {
    let current_exe = std::env::current_exe().map_err(|error| WinrError::Unsupported {
        message: format!("failed to resolve current executable path for agent dll lookup: {error}"),
    })?;
    let mut candidates = vec![
        current_exe.with_file_name("winr_roblox_agent.dll"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/winr_roblox_agent.dll"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/winr_roblox_agent.dll"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../winr-roblox-agent/target/debug/winr_roblox_agent.dll"),
    ];
    candidates.retain(|path| path.exists());
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| WinrError::Unsupported {
            message:
                "failed to locate winr_roblox_agent.dll; build the injected agent target first"
                    .to_string(),
        })
}

fn inject_agent_dll(pid: u32, dll_path: &Path) -> WinrResult<()> {
    let process = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            false,
            pid,
        )
    }
    .map_err(|error| WinrError::Unsupported {
        message: format!("OpenProcess for injection failed for pid {pid}: {error}"),
    })?;

    let result = (|| -> WinrResult<()> {
        let dll_wide = wide_null(&dll_path.display().to_string());
        let byte_len = dll_wide.len() * std::mem::size_of::<u16>();
        let remote_buffer = unsafe {
            VirtualAllocEx(
                process,
                None,
                byte_len,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if remote_buffer.is_null() {
            return Err(WinrError::Unsupported {
                message: "VirtualAllocEx failed while allocating remote dll path buffer"
                    .to_string(),
            });
        }

        unsafe {
            windows::Win32::System::Diagnostics::Debug::WriteProcessMemory(
                process,
                remote_buffer,
                dll_wide.as_ptr() as *const _,
                byte_len,
                None,
            )
        }
        .map_err(|error| WinrError::Unsupported {
            message: format!("WriteProcessMemory failed while writing remote dll path: {error}"),
        })?;

        let kernel32 = unsafe { GetModuleHandleW(PCWSTR(wide_null("kernel32.dll").as_ptr())) }
            .map_err(|error| WinrError::Unsupported {
                message: format!("GetModuleHandleW(kernel32.dll) failed: {error}"),
            })?;
        let load_library =
            unsafe { GetProcAddress(kernel32, PCSTR(c"LoadLibraryW".as_ptr() as *const u8)) }
                .ok_or_else(|| WinrError::Unsupported {
                    message: "GetProcAddress(LoadLibraryW) returned null".to_string(),
                })?;

        let thread = unsafe {
            CreateRemoteThread(
                process,
                None,
                0,
                Some(std::mem::transmute(load_library)),
                Some(remote_buffer),
                0,
                None,
            )
        }
        .map_err(|error| WinrError::Unsupported {
            message: format!("CreateRemoteThread for LoadLibraryW failed: {error}"),
        })?;

        unsafe {
            let _ = WaitForSingleObject(thread, 5_000);
        }
        let mut exit_code = 0u32;
        unsafe { GetExitCodeThread(thread, &mut exit_code) }.map_err(|error| {
            WinrError::Unsupported {
                message: format!("GetExitCodeThread failed after injection: {error}"),
            }
        })?;
        unsafe {
            let _ = CloseHandle(thread);
            let _ = VirtualFreeEx(process, remote_buffer, 0, MEM_RELEASE);
        }
        if exit_code == 0 {
            return Err(WinrError::Unsupported {
                message: format!(
                    "LoadLibraryW returned null while injecting {} into pid {}",
                    dll_path.display(),
                    pid
                ),
            });
        }
        Ok(())
    })();

    unsafe {
        let _ = CloseHandle(process);
    }
    result
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn map_injected_input_action(action: &SemanticInputAction) -> Option<InjectedInputAction> {
    match action {
        SemanticInputAction::MoveForward { duration_ms } => {
            Some(InjectedInputAction::MoveForward {
                duration_ms: *duration_ms,
            })
        }
        SemanticInputAction::StopMotion => Some(InjectedInputAction::StopMotion),
        SemanticInputAction::Turn {
            delta_yaw_milli_degrees,
        } => Some(InjectedInputAction::Turn {
            delta_yaw_milli_degrees: *delta_yaw_milli_degrees,
        }),
        SemanticInputAction::Interact => Some(InjectedInputAction::Interact),
        SemanticInputAction::Jump => Some(InjectedInputAction::Jump),
        SemanticInputAction::StrafeRight { duration_ms } => {
            Some(InjectedInputAction::StrafeRight {
                duration_ms: *duration_ms,
            })
        }
        _ => None,
    }
}

fn frame_from_snapshot(
    target: AdvancedTargetRef,
    patrol: &RobloxPatrolRegionConfig,
    snapshot: RobloxObservationSnapshot,
) -> ObservationFrame {
    let player_position = snapshot.player_position_millimeters;
    let region_distance = player_position
        .map(|position| euclidean_distance_millimeters(position, patrol.anchor_millimeters))
        .unwrap_or_default();
    let player_entity = snapshot
        .player_position_millimeters
        .map(|position| ObservationEntity {
            id: "player-self".to_string(),
            label: "Player".to_string(),
            kind: EntityKind::Player,
            confidence: 1.0,
            tags: vec![format!(
                "world_mm:{},{},{}",
                position[0], position[1], position[2]
            )],
        });
    let mut entities = Vec::new();
    if let Some(player_entity) = player_entity {
        entities.push(player_entity);
    }
    entities.push(ObservationEntity {
        id: "live-patrol-region".to_string(),
        label: "Patrol Region".to_string(),
        kind: EntityKind::Region,
        confidence: 1.0,
        tags: vec![
            format!(
                "world_mm:{},{},{}",
                patrol.anchor_millimeters[0],
                patrol.anchor_millimeters[1],
                patrol.anchor_millimeters[2]
            ),
            format!("radius_mm:{}", patrol.radius_millimeters),
            format!("distance_mm:{}", region_distance),
        ],
    });
    for (index, waypoint) in patrol_waypoints(patrol).into_iter().enumerate() {
        entities.push(ObservationEntity {
            id: format!("live-patrol-waypoint-{index}"),
            label: format!("Waypoint {}", index + 1),
            kind: EntityKind::Waypoint,
            confidence: 1.0,
            tags: vec![format!(
                "world_mm:{},{},{}",
                waypoint[0], waypoint[1], waypoint[2]
            )],
        });
    }
    for object in &snapshot.objects {
        entities.push(ObservationEntity {
            id: object.id.clone(),
            label: object.label.clone(),
            kind: if object.interactable {
                EntityKind::Interactable
            } else {
                EntityKind::VisualMarker
            },
            confidence: 0.8,
            tags: vec![
                format!(
                    "world_mm:{},{},{}",
                    object.position_millimeters[0],
                    object.position_millimeters[1],
                    object.position_millimeters[2]
                ),
                format!("kind:{}", object.kind),
            ],
        });
    }

    let prompt_visible = snapshot.prompt_visible.unwrap_or(false);
    let prompts = if prompt_visible {
        vec![MemoryPromptState {
            id: "prompt-visible".to_string(),
            label: "Interaction Prompt".to_string(),
            visible: true,
            distance_millimeters: snapshot.prompt_distance_millimeters,
        }]
    } else {
        Vec::new()
    };

    if prompt_visible {
        entities.push(ObservationEntity {
            id: "prompt-visible".to_string(),
            label: "Interaction Prompt".to_string(),
            kind: EntityKind::Prompt,
            confidence: 0.8,
            tags: vec!["visible:true".to_string()],
        });
    }

    let nearby_objects = snapshot
        .objects
        .iter()
        .map(|object| MemoryObjectState {
            id: object.id.clone(),
            label: object.label.clone(),
            kind: object.kind.clone(),
            world_position_millimeters: Some(object.position_millimeters),
            distance_millimeters: player_position
                .map(|player| euclidean_distance_millimeters(player, object.position_millimeters)),
            interactable: object.interactable,
        })
        .collect::<Vec<_>>();

    ObservationFrame {
        target,
        metadata: winr_perception::ObservationMetadata {
            version: winr_perception::ObservationStateVersion::V1,
            frame_id: snapshot.frame_id,
            timestamp_ms: snapshot.timestamp_ms,
            freshness_ms: snapshot.freshness_ms,
            backend: AdvancedProfileBackend::Inject,
            source: winr_perception::ObservationSourceKind::MemoryState,
        },
        source_data: ObservationSourceData::MemoryState {
            snapshot_id: format!("roblox-live-{}", snapshot.frame_id),
            state_fields: vec![ObservationStateField {
                key: "source".to_string(),
                value: snapshot.source,
            }],
        },
        render_details: None,
        entities,
        memory_details: Some(MemoryObservationDetails {
            schema_version: MemorySchemaVersion::V1,
            snapshot_id: format!("roblox-live-{}", snapshot.frame_id),
            intended_uses: vec![
                MemoryObservationUseCase::PlayerState,
                MemoryObservationUseCase::CameraState,
                MemoryObservationUseCase::PromptState,
                MemoryObservationUseCase::InteractableDiscovery,
            ],
            player_state: Some(MemoryPlayerState {
                world_position_millimeters: snapshot.player_position_millimeters,
                velocity_millimeters_per_second: snapshot.player_velocity_millimeters,
                movement_state: Some(ObservationMovementState::Walking),
                active_tool: None,
                active_modes: Vec::new(),
            }),
            camera_state: Some(MemoryCameraState {
                yaw_milli_degrees: snapshot.camera_yaw_milli_degrees,
                pitch_milli_degrees: snapshot.camera_pitch_milli_degrees,
                field_of_view_milli_degrees: None,
                mode: None,
            }),
            prompts,
            nearby_objects,
            raw_layout_hidden: true,
        }),
        camera_hints: Some(CameraHints {
            yaw_degrees: snapshot
                .camera_yaw_milli_degrees
                .map(|yaw| yaw as f32 / 1000.0),
            pitch_degrees: snapshot
                .camera_pitch_milli_degrees
                .map(|pitch| pitch as f32 / 1000.0),
            field_of_view_degrees: None,
            camera_mode: None,
        }),
        player_state_hints: Some(PlayerStateHints {
            world_position: snapshot.player_position_millimeters.map(|position| {
                [
                    position[0] as f32 / 1000.0,
                    position[1] as f32 / 1000.0,
                    position[2] as f32 / 1000.0,
                ]
            }),
            velocity: snapshot.player_velocity_millimeters.map(|velocity| {
                [
                    velocity[0] as f32 / 1000.0,
                    velocity[1] as f32 / 1000.0,
                    velocity[2] as f32 / 1000.0,
                ]
            }),
            health_percent: None,
            movement_state: ObservationMovementState::Walking,
            active_modes: Vec::new(),
        }),
        confidence: None,
        detectors: Vec::new(),
        detector_overlays: Vec::new(),
        notes: vec![
            snapshot.detail,
            format!(
                "camera_yaw_md:{}",
                snapshot.camera_yaw_milli_degrees.unwrap_or_default()
            ),
        ],
    }
}

fn target_is_foreground(window: &WindowInfo) -> bool {
    let Some(hwnd_text) = Some(&window.hwnd) else {
        return false;
    };
    let Ok(expected) = winr_types::parse_hwnd(hwnd_text) else {
        return false;
    };
    let foreground = unsafe { GetForegroundWindow() };
    foreground.0 == expected as *mut std::ffi::c_void
}

fn execute_foreground_fallback(action: &SemanticInputAction) -> Result<String, String> {
    match action {
        SemanticInputAction::MoveForward { duration_ms } => {
            send_key(VIRTUAL_KEY(0x57), false)?;
            thread::sleep(Duration::from_millis(*duration_ms));
            send_key(VIRTUAL_KEY(0x57), true)?;
            Ok(format!(
                "foreground fallback moved forward for {duration_ms} ms"
            ))
        }
        SemanticInputAction::StopMotion => {
            send_key(VIRTUAL_KEY(0x57), true)?;
            send_key(VIRTUAL_KEY(0x41), true)?;
            send_key(VIRTUAL_KEY(0x44), true)?;
            Ok("foreground fallback stopped movement".to_string())
        }
        SemanticInputAction::Turn {
            delta_yaw_milli_degrees,
        } => {
            let dx = (delta_yaw_milli_degrees / 1000).clamp(-127, 127);
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSE_EVENT_FLAGS(MOUSEEVENTF_MOVE.0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let count = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
            if count == 0 {
                return Err("SendInput mouse move failed".to_string());
            }
            Ok(format!(
                "foreground fallback turned by {} milli-degrees",
                delta_yaw_milli_degrees
            ))
        }
        SemanticInputAction::Interact => {
            send_key(VIRTUAL_KEY(0x45), false)?;
            send_key(VIRTUAL_KEY(0x45), true)?;
            Ok("foreground fallback interacted".to_string())
        }
        SemanticInputAction::Jump => {
            send_key(VIRTUAL_KEY(0x20), false)?;
            send_key(VIRTUAL_KEY(0x20), true)?;
            Ok("foreground fallback jumped".to_string())
        }
        SemanticInputAction::StrafeRight { duration_ms } => {
            send_key(VIRTUAL_KEY(0x44), false)?;
            thread::sleep(Duration::from_millis(*duration_ms));
            send_key(VIRTUAL_KEY(0x44), true)?;
            Ok(format!(
                "foreground fallback strafed right for {duration_ms} ms"
            ))
        }
        unsupported => Err(format!(
            "foreground fallback does not implement action {:?}",
            unsupported
        )),
    }
}

fn send_key(key: VIRTUAL_KEY, key_up: bool) -> Result<(), String> {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYBD_EVENT_FLAGS(KEYEVENTF_KEYUP.0)
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let count = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if count == 0 {
        return Err("SendInput keyboard event failed".to_string());
    }
    Ok(())
}

fn send_named_pipe_action(pipe_name: &str, action: &SemanticInputAction) -> Result<String, String> {
    let path = format!(r"\\.\pipe\{pipe_name}");
    let mut pipe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("failed to open input pipe {path}: {error}"))?;
    let payload = serde_json::to_vec(action)
        .map_err(|error| format!("failed to serialize input action for pipe: {error}"))?;
    pipe.write_all(&payload)
        .and_then(|_| pipe.write_all(b"\n"))
        .map_err(|error| format!("failed to write input pipe command: {error}"))?;
    pipe.flush()
        .map_err(|error| format!("failed to flush input pipe command: {error}"))?;

    let mut response = String::new();
    pipe.read_to_string(&mut response)
        .map_err(|error| format!("failed to read input pipe response: {error}"))?;
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err("input pipe returned an empty response".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use winr_types::{
        AdvancedAttachmentHealthStatus, AdvancedBackendLifecycleState, AdvancedProfileBackend,
        RobloxObjectField,
    };

    use crate::{default_agent_composition, default_transport_descriptor};

    #[derive(Default)]
    struct FakeMemory {
        modules: HashMap<String, usize>,
        bytes: HashMap<usize, Vec<u8>>,
    }

    impl ProcessMemory for FakeMemory {
        fn module_base(&self, module_name: &str) -> Result<usize, String> {
            self.modules
                .get(module_name)
                .copied()
                .ok_or_else(|| format!("missing module {module_name}"))
        }

        fn read_bytes(&self, address: usize, len: usize) -> Result<Vec<u8>, String> {
            self.bytes
                .get(&address)
                .cloned()
                .filter(|bytes| bytes.len() == len)
                .ok_or_else(|| format!("missing bytes at 0x{address:X}"))
        }
    }

    fn sample_schema() -> RobloxMemorySchema {
        RobloxMemorySchema {
            game_build: "test".to_string(),
            schema_version: "1".to_string(),
            player_position: Some(RobloxMemoryField {
                module: "RobloxPlayerBeta.exe".to_string(),
                base_offset: 0x1000,
                dereference_offsets: vec![0x20],
                value_kind: RobloxMemoryValueKind::Vec3F32,
            }),
            player_velocity: None,
            camera_yaw_milli_degrees: Some(RobloxMemoryField {
                module: "RobloxPlayerBeta.exe".to_string(),
                base_offset: 0x2000,
                dereference_offsets: Vec::new(),
                value_kind: RobloxMemoryValueKind::I32,
            }),
            camera_pitch_milli_degrees: None,
            prompt_visible: Some(RobloxMemoryField {
                module: "RobloxPlayerBeta.exe".to_string(),
                base_offset: 0x3000,
                dereference_offsets: Vec::new(),
                value_kind: RobloxMemoryValueKind::U8Bool,
            }),
            prompt_distance_millimeters: None,
            objects: vec![RobloxObjectField {
                id: "rock-1".to_string(),
                label: "Rock".to_string(),
                kind: "resource_node".to_string(),
                interactable: true,
                position: RobloxMemoryField {
                    module: "RobloxPlayerBeta.exe".to_string(),
                    base_offset: 0x4000,
                    dereference_offsets: Vec::new(),
                    value_kind: RobloxMemoryValueKind::Vec3I32,
                },
            }],
        }
    }

    fn sample_context() -> ObservationCaptureContext {
        ObservationCaptureContext {
            target: AdvancedTargetRef {
                hwnd: Some("0x0000000000001234".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            backend: AdvancedProfileBackend::Inject,
            frame_id: 1,
            timestamp_ms: 100,
            freshness_ms: 16,
        }
    }

    #[test]
    fn schema_observer_projects_live_entities() {
        let mut memory = FakeMemory::default();
        memory
            .modules
            .insert("RobloxPlayerBeta.exe".to_string(), 0x10000000);
        memory
            .bytes
            .insert(0x10001000, (0x20000000usize).to_le_bytes().to_vec());
        memory.bytes.insert(
            0x20000020,
            [
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                2.5f32.to_le_bytes(),
            ]
            .concat(),
        );
        memory
            .bytes
            .insert(0x10002000, 90000_i32.to_le_bytes().to_vec());
        memory.bytes.insert(0x10003000, vec![1_u8]);
        memory.bytes.insert(
            0x10004000,
            [
                1100_i32.to_le_bytes(),
                0_i32.to_le_bytes(),
                2500_i32.to_le_bytes(),
            ]
            .concat(),
        );

        let observer = RobloxMemoryObserver {
            target: sample_context().target.clone(),
            schema: &sample_schema(),
            memory,
        };
        let frame = observer
            .read_frame(
                &sample_context(),
                &RobloxPatrolRegionConfig {
                    anchor_millimeters: [1000, 0, 2000],
                    radius_millimeters: 500,
                },
            )
            .expect("live frame should build");

        assert_eq!(
            frame.metadata.source,
            winr_perception::ObservationSourceKind::MemoryState
        );
        assert!(
            frame
                .entities
                .iter()
                .any(|entity| entity.id == "live-patrol-region")
        );
        assert!(frame.entities.iter().any(|entity| entity.id == "rock-1"));
        assert!(
            frame
                .entities
                .iter()
                .any(|entity| entity.id == "live-prompt")
        );
    }

    #[test]
    fn live_runtime_inspection_surfaces_recent_rejection() {
        let mut session = AdvancedBackendSession::new(
            winr_types::AdvancedSessionId(1),
            winr_types::AdvancedBackendHello {
                protocol_version: 1,
                backend: AdvancedProfileBackend::Inject,
                lifecycle_state: AdvancedBackendLifecycleState::Attached,
                capabilities: winr_types::AdvancedBackendCapabilities {
                    memory_observation: true,
                    injected_input: true,
                    ..Default::default()
                },
                target: sample_context().target,
                transport: default_transport_descriptor(),
                composition: default_agent_composition(),
            },
        );
        session.record_command_outcome(
            "roblox_input::walk_to",
            AdvancedCommandAckStatus::Rejected,
            "bridge missing",
        );
        let inspection = LiveSessionInspection {
            workflow_id: "region-patrol-live".to_string(),
            session_id: session.session_id,
            lifecycle_state: session.hello.lifecycle_state,
            backend: session.hello.backend,
            attachment_status: AdvancedAttachmentHealthStatus::Healthy,
            observation: None,
            recent_events: session.structured_events.clone(),
            reasoning: None,
            last_rejected_command: session.command_records.last().cloned(),
        };

        assert_eq!(
            inspection
                .last_rejected_command
                .as_ref()
                .expect("rejected command should exist")
                .detail,
            "bridge missing"
        );
    }
}
