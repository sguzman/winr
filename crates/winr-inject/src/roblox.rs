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
            Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
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
};
use winr_perception::{
    CameraHints, EntityKind, MemoryCameraState, MemoryObjectState, MemoryObservationDetails,
    MemoryObservationUseCase, MemoryPlayerState, MemoryPromptState, MemorySchemaVersion,
    ObservationCaptureContext, ObservationEntity, ObservationFrame, ObservationMovementState,
    ObservationSourceData, ObservationStateField, PlayerStateHints, WorldModelTracker,
};
use winr_types::{
    AdvancedCommandAckStatus, AdvancedExecutionReason, AdvancedFrontend,
    AdvancedProfileBackend, AdvancedTargetRef, LiveObservationSummary, LiveSessionInspection,
    ProfileConfig, ProfileRunResult, RobloxAdvancedConfig, RobloxPatrolRegionConfig, WinrError,
    WinrResult, WindowInfo,
};
use winr_workflows::{
    AppPackMovementTuning, BoundedRegionPatrolController, ControllerMemory, NavigationContext,
    NavigationController, NavigationControllerConfig, ProgressSample, SemanticInputAction,
    SemanticInputTarget, WorkflowExecutionTrace, WorkflowTraceEventKind, load_app_pack_from_dir,
};

use crate::{AdvancedBackendSession, AttachmentSupervisor, prepare_profile_backend_for_frontend};

const ROBLOX_PACK_DIR: &str = "../../packs/roblox";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveRobloxRunOptions {
    pub poll_interval: Duration,
    pub max_steps: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobloxMemorySchema {
    pub game_build: String,
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_position: Option<RobloxMemoryField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_velocity: Option<RobloxMemoryField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_yaw_milli_degrees: Option<RobloxMemoryField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_pitch_milli_degrees: Option<RobloxMemoryField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_visible: Option<RobloxMemoryField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_distance_millimeters: Option<RobloxMemoryField>,
    #[serde(default)]
    pub objects: Vec<RobloxObjectField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobloxMemoryField {
    pub module: String,
    pub base_offset: usize,
    #[serde(default)]
    pub dereference_offsets: Vec<usize>,
    pub value_kind: RobloxMemoryValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobloxObjectField {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub interactable: bool,
    pub position: RobloxMemoryField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RobloxMemoryValueKind {
    Vec3F32,
    Vec3I32,
    I32,
    U32,
    F32,
    U8Bool,
}

pub fn load_roblox_memory_schema(path: &Path) -> WinrResult<RobloxMemorySchema> {
    let text = fs::read_to_string(path).map_err(|error| WinrError::Unsupported {
        message: format!("failed to read Roblox memory schema {}: {error}", path.display()),
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
            runtime
                .session
                .record_reasoning("workflow stopped by caller", vec!["stop signal received".to_string()]);
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
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            )
        }
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
        let snapshot = unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid)
        }
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

        let region_distance = player_position.map(|position| {
            euclidean_distance_millimeters(position, patrol.anchor_millimeters)
        });
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
            let bytes = self.memory.read_bytes(cursor, std::mem::size_of::<usize>())?;
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
    schema: RobloxMemorySchema,
    advanced: RobloxAdvancedConfig,
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
        let workflow = profile.workflow.as_ref().ok_or_else(|| WinrError::Unsupported {
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

        let session = prepare_profile_backend_for_frontend(profile, frontend)?;
        let (attachment, _) = AttachmentSupervisor::attach(
            &profile.target,
            winr_types::AdvancedAttachmentPolicy::default(),
        )?;
        let target_window = attachable_target_into_window_info(&attachment.attachment.target);
        let schema_path = PathBuf::from(&advanced.memory_schema_path);
        let schema = load_roblox_memory_schema(&schema_path)?;

        Ok(Self {
            session,
            attachment,
            target_window,
            schema,
            advanced,
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
            format!("accepted live observation frame {}", frame.metadata.frame_id),
        );
        let world_model = self
            .tracker
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
        self.controller_memory.progress_samples.push(ProgressSample {
            frame_id: frame.metadata.frame_id,
            player_position_millimeters: frame
                .memory_details
                .as_ref()
                .and_then(|details| details.player_state.as_ref())
                .and_then(|state| state.world_position_millimeters),
            target_distance_millimeters: world_model
                .best_entity(EntityKind::Region)
                .and_then(|entity| {
                    entity
                        .entity
                        .tags
                        .iter()
                        .find_map(|tag| tag.strip_prefix("distance_mm:"))
                        .and_then(|value| value.parse::<u32>().ok())
                }),
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
        let pid = self
            .attachment
            .attachment
            .target
            .target
            .pid
            .ok_or_else(|| WinrError::Unsupported {
                message: "live Roblox session does not have a PID".to_string(),
            })?;
        let memory = WindowsProcessMemory::open(pid).map_err(|error| WinrError::Unsupported {
            message: error,
        })?;
        let observer = RobloxMemoryObserver {
            target: self.attachment.attachment.target.target.clone(),
            schema: &self.schema,
            memory,
        };
        let context = ObservationCaptureContext {
            target: self.attachment.attachment.target.target.clone(),
            backend: AdvancedProfileBackend::Inject,
            frame_id: self.step_index + 1,
            timestamp_ms: current_timestamp_ms(),
            freshness_ms: 16,
        };
        let frame = observer
            .read_frame(&context, &self.advanced.patrol_region)
            .map_err(|error| WinrError::Unsupported { message: error })?;
        self.tracker.update(&frame);
        self.session.apply_event(&winr_types::AdvancedAgentEventEnvelope {
            session_id: self.session.session_id,
            sequence: winr_types::AdvancedSequenceNumber(self.step_index + 1),
            event: winr_types::AdvancedAgentEvent::ObservationTick {
                update: winr_types::AdvancedObservationUpdate {
                    frame_id: frame.metadata.frame_id,
                    source: frame_source_name(&frame),
                    detail: "live Roblox memory frame".to_string(),
                    timestamp_ms: Some(frame.metadata.timestamp_ms),
                    freshness_ms: Some(frame.metadata.freshness_ms),
                    payload: None,
                },
            },
        })?;
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
        if let Some(pipe_name) = &self.advanced.input_pipe_name {
            return match send_named_pipe_action(pipe_name, action) {
                Ok(detail) => {
                    self.session.record_command_outcome(
                        format!("roblox_input::{:?}", action),
                        AdvancedCommandAckStatus::Acked,
                        detail.clone(),
                    );
                    Ok(detail)
                }
                Err(error) => {
                    self.session.record_command_outcome(
                        format!("roblox_input::{:?}", action),
                        AdvancedCommandAckStatus::Rejected,
                        error.clone(),
                    );
                    Err(WinrError::Unsupported { message: error })
                }
            };
        }

        if self.advanced.allow_foreground_fallback && target_is_foreground(&self.target_window) {
            return match execute_foreground_fallback(action) {
                Ok(detail) => {
                    self.session.record_command_outcome(
                        format!("foreground_fallback::{:?}", action),
                        AdvancedCommandAckStatus::Acked,
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
            "no injected input pipe configured and foreground fallback is unavailable".to_string();
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
                entities: frame.entities.iter().map(|entity| entity.id.clone()).collect(),
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
            reasoning: self.session.recent_reason.clone().or_else(|| trace_reasoning(&self.trace)),
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

fn target_world_position(target: &SemanticInputTarget, context: &NavigationContext) -> Option<[i32; 3]> {
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
            Ok(format!("foreground fallback moved forward for {duration_ms} ms"))
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
            Ok(format!("foreground fallback strafed right for {duration_ms} ms"))
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
            [1.0f32.to_le_bytes(), 0.0f32.to_le_bytes(), 2.5f32.to_le_bytes()].concat(),
        );
        memory
            .bytes
            .insert(0x10002000, 90000_i32.to_le_bytes().to_vec());
        memory.bytes.insert(0x10003000, vec![1_u8]);
        memory.bytes.insert(
            0x10004000,
            [1100_i32.to_le_bytes(), 0_i32.to_le_bytes(), 2500_i32.to_le_bytes()].concat(),
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

        assert_eq!(frame.metadata.source, winr_perception::ObservationSourceKind::MemoryState);
        assert!(frame.entities.iter().any(|entity| entity.id == "live-patrol-region"));
        assert!(frame.entities.iter().any(|entity| entity.id == "rock-1"));
        assert!(frame.entities.iter().any(|entity| entity.id == "live-prompt"));
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
