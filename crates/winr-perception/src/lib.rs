use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use winr_types::{
    AdvancedBackendCapabilities, AdvancedBinaryPayloadRef, AdvancedObservationUpdate,
    AdvancedProfileBackend, AdvancedTargetRef, WinrResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSourceKind {
    DesktopScreenshot,
    RenderHookFrame,
    MemoryState,
    DetectorOverlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationPixelFormat {
    Bgra8,
    Rgba8,
    Gray8,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Player,
    Camera,
    Region,
    Prompt,
    Interactable,
    Collectible,
    Obstacle,
    Waypoint,
    VisualMarker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    TemplateMatch,
    ColorCluster,
    Ocr,
    ObjectDetection,
    MemoryEntity,
    RenderEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverlayKind {
    BoundingBoxes,
    SegmentationMask,
    Heatmap,
    OcrText,
    DebugMarkup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationMovementState {
    Idle,
    Walking,
    Running,
    Jumping,
    Falling,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStateVersion {
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DetectorDescriptor {
    pub id: String,
    pub name: String,
    pub kind: DetectorKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationEntity {
    pub id: String,
    pub kind: EntityKind,
    pub label: String,
    pub confidence: f32,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationConfidenceSummary {
    pub overall: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_average: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detector_average: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CameraHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw_degrees: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_degrees: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_of_view_degrees: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerStateHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_position: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_percent: Option<f32>,
    pub movement_state: ObservationMovementState,
    #[serde(default)]
    pub active_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationImageHandle {
    pub payload: AdvancedBinaryPayloadRef,
    pub width: u32,
    pub height: u32,
    pub pixel_format: ObservationPixelFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationFrameHandle {
    pub payload: AdvancedBinaryPayloadRef,
    pub width: u32,
    pub height: u32,
    pub pixel_format: ObservationPixelFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_stride_bytes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationStateField {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DetectorOverlay {
    pub detector_id: String,
    pub kind: OverlayKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<AdvancedBinaryPayloadRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationSourceData {
    DesktopScreenshot {
        image: ObservationImageHandle,
    },
    RenderHookFrame {
        frame: ObservationFrameHandle,
    },
    MemoryState {
        snapshot_id: String,
        #[serde(default)]
        state_fields: Vec<ObservationStateField>,
    },
    DetectorOverlay {
        overlay: DetectorOverlay,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationMetadata {
    pub version: ObservationStateVersion,
    pub backend: AdvancedProfileBackend,
    pub source: ObservationSourceKind,
    pub frame_id: u64,
    pub timestamp_ms: u64,
    pub freshness_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationCaptureContext {
    pub target: AdvancedTargetRef,
    pub backend: AdvancedProfileBackend,
    pub frame_id: u64,
    pub timestamp_ms: u64,
    pub freshness_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationFrame {
    pub target: AdvancedTargetRef,
    pub metadata: ObservationMetadata,
    pub source_data: ObservationSourceData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_hints: Option<CameraHints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_state_hints: Option<PlayerStateHints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ObservationConfidenceSummary>,
    #[serde(default)]
    pub detectors: Vec<DetectorDescriptor>,
    #[serde(default)]
    pub detector_overlays: Vec<DetectorOverlay>,
    #[serde(default)]
    pub entities: Vec<ObservationEntity>,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub trait ObservationFrameSource {
    fn source_kind(&self) -> ObservationSourceKind;
    fn advertised_capabilities(&self) -> AdvancedBackendCapabilities;
    fn describe_detectors(&self) -> Vec<DetectorDescriptor>;
    fn capture_frame(&mut self, context: &ObservationCaptureContext)
        -> WinrResult<Option<ObservationFrame>>;
}

#[derive(Default)]
pub struct ObservationStack {
    sources: Vec<Box<dyn ObservationFrameSource>>,
}

impl ObservationStack {
    pub fn register_source(&mut self, source: Box<dyn ObservationFrameSource>) {
        self.sources.push(source);
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn collect(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<Vec<ObservationFrame>> {
        let mut frames = Vec::new();
        for source in &mut self.sources {
            if let Some(frame) = source.capture_frame(context)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }
}

impl ObservationFrame {
    pub fn from_update(
        context: ObservationCaptureContext,
        update: AdvancedObservationUpdate,
        source_data: ObservationSourceData,
    ) -> Self {
        Self {
            target: context.target,
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: context.backend,
                source: source_data.kind(),
                frame_id: update.frame_id,
                timestamp_ms: context.timestamp_ms,
                freshness_ms: context.freshness_ms,
            },
            source_data,
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: Vec::new(),
            notes: vec![update.detail],
        }
    }

    pub fn entity_confidence_average(&self) -> Option<f32> {
        if self.entities.is_empty() {
            return None;
        }

        Some(
            self.entities
                .iter()
                .map(|entity| entity.confidence)
                .sum::<f32>()
                / self.entities.len() as f32,
        )
    }

    pub fn with_confidence_summary(mut self, overall: f32) -> Self {
        self.confidence = Some(ObservationConfidenceSummary {
            overall,
            entity_average: self.entity_confidence_average(),
            detector_average: None,
        });
        self
    }
}

impl ObservationSourceData {
    pub fn kind(&self) -> ObservationSourceKind {
        match self {
            Self::DesktopScreenshot { .. } => ObservationSourceKind::DesktopScreenshot,
            Self::RenderHookFrame { .. } => ObservationSourceKind::RenderHookFrame,
            Self::MemoryState { .. } => ObservationSourceKind::MemoryState,
            Self::DetectorOverlay { .. } => ObservationSourceKind::DetectorOverlay,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaticObservationSource {
    kind: ObservationSourceKind,
    detectors: Vec<DetectorDescriptor>,
    frame: ObservationFrame,
    capabilities: AdvancedBackendCapabilities,
}

impl StaticObservationSource {
    pub fn new(
        frame: ObservationFrame,
        detectors: Vec<DetectorDescriptor>,
        capabilities: AdvancedBackendCapabilities,
    ) -> Self {
        Self {
            kind: frame.metadata.source,
            detectors,
            frame,
            capabilities,
        }
    }
}

impl ObservationFrameSource for StaticObservationSource {
    fn source_kind(&self) -> ObservationSourceKind {
        self.kind
    }

    fn advertised_capabilities(&self) -> AdvancedBackendCapabilities {
        self.capabilities.clone()
    }

    fn describe_detectors(&self) -> Vec<DetectorDescriptor> {
        self.detectors.clone()
    }

    fn capture_frame(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<Option<ObservationFrame>> {
        let mut frame = self.frame.clone();
        frame.target = context.target.clone();
        frame.metadata.backend = context.backend;
        frame.metadata.frame_id = context.frame_id;
        frame.metadata.timestamp_ms = context.timestamp_ms;
        frame.metadata.freshness_ms = context.freshness_ms;
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_types::{
        AdvancedBinaryPayloadRef, AdvancedIpcTransportKind, AdvancedPayloadEncoding,
    };

    fn sample_target() -> AdvancedTargetRef {
        AdvancedTargetRef {
            hwnd: Some("0x0000000000001234".to_string()),
            pid: Some(42),
            exe: Some("RobloxPlayerBeta.exe".to_string()),
            window_class: Some("WINDOWSCLIENT".to_string()),
            title_hint: Some("Roblox".to_string()),
        }
    }

    fn sample_payload(id: &str) -> AdvancedBinaryPayloadRef {
        AdvancedBinaryPayloadRef {
            payload_id: id.to_string(),
            encoding: AdvancedPayloadEncoding::RawBytes,
            byte_len: 16,
            transport: AdvancedIpcTransportKind::SharedMemory,
            description: "sample pixels".to_string(),
        }
    }

    fn sample_context() -> ObservationCaptureContext {
        ObservationCaptureContext {
            target: sample_target(),
            backend: AdvancedProfileBackend::Inject,
            frame_id: 7,
            timestamp_ms: 1000,
            freshness_ms: 16,
        }
    }

    fn sample_entity() -> ObservationEntity {
        ObservationEntity {
            id: "rock-1".to_string(),
            kind: EntityKind::Interactable,
            label: "Rock".to_string(),
            confidence: 0.92,
            tags: vec!["resource".to_string()],
        }
    }

    #[test]
    fn observation_frame_serializes_cleanly() {
        let frame = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::RenderHookFrame,
                frame_id: 7,
                timestamp_ms: 1000,
                freshness_ms: 16,
            },
            source_data: ObservationSourceData::RenderHookFrame {
                frame: ObservationFrameHandle {
                    payload: sample_payload("frame-7"),
                    width: 1920,
                    height: 1080,
                    pixel_format: ObservationPixelFormat::Bgra8,
                    row_stride_bytes: Some(7680),
                },
            },
            camera_hints: Some(CameraHints {
                yaw_degrees: Some(90.0),
                pitch_degrees: Some(-12.0),
                field_of_view_degrees: Some(70.0),
                camera_mode: Some("third_person".to_string()),
            }),
            player_state_hints: Some(PlayerStateHints {
                world_position: Some([10.0, 0.0, -4.0]),
                velocity: Some([0.5, 0.0, 0.0]),
                health_percent: Some(1.0),
                movement_state: ObservationMovementState::Walking,
                active_modes: vec!["harvesting".to_string()],
            }),
            confidence: Some(ObservationConfidenceSummary {
                overall: 0.95,
                entity_average: Some(0.92),
                detector_average: Some(0.98),
            }),
            detectors: vec![DetectorDescriptor {
                id: "rock-template".to_string(),
                name: "Rock Template".to_string(),
                kind: DetectorKind::TemplateMatch,
            }],
            detector_overlays: vec![DetectorOverlay {
                detector_id: "rock-template".to_string(),
                kind: OverlayKind::BoundingBoxes,
                label: "rock boxes".to_string(),
                payload: Some(sample_payload("overlay-7")),
            }],
            entities: vec![sample_entity()],
            notes: vec!["sample".to_string()],
        };

        let json = serde_json::to_string(&frame).expect("frame should serialize");
        assert!(json.contains("\"render_hook_frame\""));
        assert!(json.contains("\"rock-1\""));
        assert!(json.contains("\"camera_hints\""));
    }

    #[test]
    fn from_update_normalizes_desktop_screenshot_source() {
        let frame = ObservationFrame::from_update(
            sample_context(),
            AdvancedObservationUpdate {
                frame_id: 44,
                source: "desktop".to_string(),
                detail: "captured desktop screenshot".to_string(),
                payload: Some(sample_payload("desktop-44")),
            },
            ObservationSourceData::DesktopScreenshot {
                image: ObservationImageHandle {
                    payload: sample_payload("desktop-44"),
                    width: 800,
                    height: 600,
                    pixel_format: ObservationPixelFormat::Bgra8,
                },
            },
        );

        assert_eq!(frame.metadata.source, ObservationSourceKind::DesktopScreenshot);
        assert_eq!(frame.metadata.frame_id, 44);
        assert_eq!(frame.notes[0], "captured desktop screenshot");
    }

    #[test]
    fn observation_stack_collects_frames_from_multiple_sources() {
        let mut stack = ObservationStack::default();
        let desktop_frame = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Foreground,
                source: ObservationSourceKind::DesktopScreenshot,
                frame_id: 1,
                timestamp_ms: 10,
                freshness_ms: 20,
            },
            source_data: ObservationSourceData::DesktopScreenshot {
                image: ObservationImageHandle {
                    payload: sample_payload("desktop"),
                    width: 640,
                    height: 480,
                    pixel_format: ObservationPixelFormat::Bgra8,
                },
            },
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: vec![sample_entity()],
            notes: Vec::new(),
        };
        let memory_frame = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::MemoryState,
                frame_id: 2,
                timestamp_ms: 10,
                freshness_ms: 5,
            },
            source_data: ObservationSourceData::MemoryState {
                snapshot_id: "snap-2".to_string(),
                state_fields: vec![ObservationStateField {
                    key: "player.position".to_string(),
                    value: "[10,0,-4]".to_string(),
                }],
            },
            camera_hints: None,
            player_state_hints: Some(PlayerStateHints {
                world_position: Some([10.0, 0.0, -4.0]),
                velocity: None,
                health_percent: None,
                movement_state: ObservationMovementState::Walking,
                active_modes: Vec::new(),
            }),
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: Vec::new(),
            notes: Vec::new(),
        };

        stack.register_source(Box::new(StaticObservationSource::new(
            desktop_frame,
            Vec::new(),
            AdvancedBackendCapabilities {
                foreground_input: true,
                ..Default::default()
            },
        )));
        stack.register_source(Box::new(StaticObservationSource::new(
            memory_frame,
            Vec::new(),
            AdvancedBackendCapabilities {
                memory_observation: true,
                ..Default::default()
            },
        )));

        let frames = stack
            .collect(&sample_context())
            .expect("stack collection should succeed");

        assert_eq!(stack.source_count(), 2);
        assert_eq!(frames.len(), 2);
        assert!(frames
            .iter()
            .any(|frame| frame.metadata.source == ObservationSourceKind::DesktopScreenshot));
        assert!(frames
            .iter()
            .any(|frame| frame.metadata.source == ObservationSourceKind::MemoryState));
    }
}
