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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderHookBoundary {
    DxgiPresent,
    D3d11Present,
    D3d12Present,
    VulkanPresent,
    OpenGlSwapBuffers,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderSceneUseCase {
    VisibleSceneUnderstanding,
    TemplateDetection,
    ObjectDetection,
    ActionCorrelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySchemaVersion {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryObservationUseCase {
    PlayerState,
    CameraState,
    InteractableDiscovery,
    PromptState,
    ObjectInventory,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackedEntityStatus {
    Active,
    Lost,
    Reacquired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackedObservationEntity {
    pub entity: ObservationEntity,
    pub smoothed_confidence: f32,
    pub priority_score: u32,
    pub first_seen_frame_id: u64,
    pub last_seen_frame_id: u64,
    pub missed_frames: u32,
    pub status: TrackedEntityStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorldModel {
    pub target: AdvancedTargetRef,
    pub last_updated_frame_id: u64,
    #[serde(default)]
    pub detector_kinds: Vec<DetectorKind>,
    #[serde(default)]
    pub entities: Vec<TrackedObservationEntity>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct WorldModelDelta {
    #[serde(default)]
    pub new_entities: Vec<String>,
    #[serde(default)]
    pub lost_entities: Vec<String>,
    #[serde(default)]
    pub reacquired_entities: Vec<String>,
    #[serde(default)]
    pub reprioritized_entities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorldModelTrackerConfig {
    pub confidence_alpha: f32,
    pub lost_after_missed_frames: u32,
    pub drop_after_missed_frames: u32,
}

impl Default for WorldModelTrackerConfig {
    fn default() -> Self {
        Self {
            confidence_alpha: 0.65,
            lost_after_missed_frames: 2,
            drop_after_missed_frames: 4,
        }
    }
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
pub struct MemoryCameraState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw_milli_degrees: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_milli_degrees: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_of_view_milli_degrees: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryPlayerState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_position_millimeters: Option<[i32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity_millimeters_per_second: Option<[i32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movement_state: Option<ObservationMovementState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tool: Option<String>,
    #[serde(default)]
    pub active_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryPromptState {
    pub id: String,
    pub label: String,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_millimeters: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryObjectState {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_position_millimeters: Option<[i32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_millimeters: Option<u32>,
    pub interactable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryObservationDetails {
    pub schema_version: MemorySchemaVersion,
    pub snapshot_id: String,
    #[serde(default)]
    pub intended_uses: Vec<MemoryObservationUseCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_state: Option<MemoryPlayerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_state: Option<MemoryCameraState>,
    #[serde(default)]
    pub prompts: Vec<MemoryPromptState>,
    #[serde(default)]
    pub nearby_objects: Vec<MemoryObjectState>,
    pub raw_layout_hidden: bool,
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
pub struct RenderFrameTiming {
    pub present_timestamp_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_interval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderFrameAvailability {
    pub frame_ready: bool,
    pub present_count: u64,
    pub dropped_since_last_capture: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderSampleRegion {
    pub id: String,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<AdvancedBinaryPayloadRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DebugOverlayCommand {
    pub label: String,
    pub kind: OverlayKind,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderDebugOverlaySurface {
    pub development_only: bool,
    #[serde(default)]
    pub commands: Vec<DebugOverlayCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderObservationDetails {
    pub boundary: RenderHookBoundary,
    pub timing: RenderFrameTiming,
    pub availability: RenderFrameAvailability,
    #[serde(default)]
    pub sample_regions: Vec<RenderSampleRegion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_overlay: Option<RenderDebugOverlaySurface>,
    #[serde(default)]
    pub intended_uses: Vec<RenderSceneUseCase>,
    pub does_not_claim_game_state_api: bool,
    pub does_not_claim_background_input_channel: bool,
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
    pub render_details: Option<RenderObservationDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_details: Option<MemoryObservationDetails>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationFreshnessStatus {
    Fresh,
    Aging,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationFreshnessPolicy {
    pub aging_threshold_ms: u64,
    pub stale_threshold_ms: u64,
}

impl Default for ObservationFreshnessPolicy {
    fn default() -> Self {
        Self {
            aging_threshold_ms: 33,
            stale_threshold_ms: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationFreshnessAssessment {
    pub status: ObservationFreshnessStatus,
    pub freshness_ms: u64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ObservationReplayTape {
    #[serde(default)]
    pub frames: Vec<ObservationFrame>,
}

pub trait ObservationFrameSource {
    fn source_kind(&self) -> ObservationSourceKind;
    fn advertised_capabilities(&self) -> AdvancedBackendCapabilities;
    fn describe_detectors(&self) -> Vec<DetectorDescriptor>;
    fn capture_frame(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<Option<ObservationFrame>>;
}

pub trait RenderFrameAnalyzer {
    fn name(&self) -> &str;
    fn analyze(&self, frame: &ObservationFrame) -> Vec<DetectorOverlay>;
}

pub trait MemoryStateProjector {
    fn name(&self) -> &str;
    fn project_entities(&self, frame: &ObservationFrame) -> Vec<ObservationEntity>;
}

#[derive(Debug, Default)]
pub struct WorldModelTracker {
    pub config: WorldModelTrackerConfig,
    pub model: Option<WorldModel>,
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
                timestamp_ms: update.timestamp_ms.unwrap_or(context.timestamp_ms),
                freshness_ms: update.freshness_ms.unwrap_or(context.freshness_ms),
            },
            source_data,
            render_details: None,
            memory_details: None,
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

    pub fn with_render_details(mut self, render_details: RenderObservationDetails) -> Self {
        self.render_details = Some(render_details);
        self
    }

    pub fn with_memory_details(mut self, memory_details: MemoryObservationDetails) -> Self {
        self.memory_details = Some(memory_details);
        self
    }

    pub fn assess_freshness(
        &self,
        policy: &ObservationFreshnessPolicy,
    ) -> ObservationFreshnessAssessment {
        let freshness_ms = self.metadata.freshness_ms;
        let status = if freshness_ms >= policy.stale_threshold_ms {
            ObservationFreshnessStatus::Stale
        } else if freshness_ms >= policy.aging_threshold_ms {
            ObservationFreshnessStatus::Aging
        } else {
            ObservationFreshnessStatus::Fresh
        };
        let detail = match status {
            ObservationFreshnessStatus::Fresh => {
                format!("frame freshness {freshness_ms}ms is within fresh threshold")
            }
            ObservationFreshnessStatus::Aging => {
                format!("frame freshness {freshness_ms}ms is aging toward stale")
            }
            ObservationFreshnessStatus::Stale => {
                format!("frame freshness {freshness_ms}ms exceeded stale threshold")
            }
        };

        ObservationFreshnessAssessment {
            status,
            freshness_ms,
            detail,
        }
    }
}

impl ObservationReplayTape {
    pub fn push(&mut self, frame: ObservationFrame) {
        self.frames.push(frame);
    }

    pub fn latest(&self) -> Option<&ObservationFrame> {
        self.frames.last()
    }

    pub fn frame(&self, frame_id: u64) -> Option<&ObservationFrame> {
        self.frames
            .iter()
            .find(|frame| frame.metadata.frame_id == frame_id)
    }

    pub fn stale_frames(&self, policy: &ObservationFreshnessPolicy) -> Vec<&ObservationFrame> {
        self.frames
            .iter()
            .filter(|frame| {
                matches!(
                    frame.assess_freshness(policy).status,
                    ObservationFreshnessStatus::Stale
                )
            })
            .collect()
    }
}

impl WorldModel {
    pub fn active_entities(&self) -> Vec<&TrackedObservationEntity> {
        self.entities
            .iter()
            .filter(|entity| entity.status != TrackedEntityStatus::Lost)
            .collect()
    }

    pub fn lost_entities(&self) -> Vec<&TrackedObservationEntity> {
        self.entities
            .iter()
            .filter(|entity| entity.status == TrackedEntityStatus::Lost)
            .collect()
    }

    pub fn prioritized_entities(&self) -> Vec<&TrackedObservationEntity> {
        let mut entities = self.active_entities();
        entities.sort_by(|left, right| {
            right
                .priority_score
                .cmp(&left.priority_score)
                .then_with(|| right.last_seen_frame_id.cmp(&left.last_seen_frame_id))
        });
        entities
    }

    pub fn best_entity(&self, kind: EntityKind) -> Option<&TrackedObservationEntity> {
        self.prioritized_entities()
            .into_iter()
            .find(|entity| entity.entity.kind == kind)
    }
}

impl WorldModelTracker {
    pub fn update(&mut self, frame: &ObservationFrame) -> WorldModelDelta {
        let mut delta = WorldModelDelta::default();
        let model = self.model.get_or_insert_with(|| WorldModel {
            target: frame.target.clone(),
            last_updated_frame_id: frame.metadata.frame_id,
            detector_kinds: frame
                .detectors
                .iter()
                .map(|detector| detector.kind)
                .collect(),
            entities: Vec::new(),
            notes: vec!["world model initialized from observation frame".to_string()],
        });

        model.target = frame.target.clone();
        model.last_updated_frame_id = frame.metadata.frame_id;
        model.detector_kinds = unique_detector_kinds(frame);

        let mut seen_ids = Vec::new();
        for entity in &frame.entities {
            seen_ids.push(entity.id.clone());
            match model
                .entities
                .iter_mut()
                .find(|tracked| tracked.entity.id == entity.id)
            {
                Some(tracked) => {
                    let previous_status = tracked.status;
                    let previous_priority = tracked.priority_score;
                    tracked.entity = entity.clone();
                    tracked.smoothed_confidence = smooth_confidence(
                        tracked.smoothed_confidence,
                        entity.confidence,
                        self.config.confidence_alpha,
                    );
                    tracked.priority_score =
                        compute_priority_score(&tracked.entity, tracked.smoothed_confidence);
                    tracked.last_seen_frame_id = frame.metadata.frame_id;
                    tracked.missed_frames = 0;
                    tracked.status = if previous_status == TrackedEntityStatus::Lost {
                        delta.reacquired_entities.push(entity.id.clone());
                        TrackedEntityStatus::Reacquired
                    } else {
                        TrackedEntityStatus::Active
                    };
                    if previous_priority != tracked.priority_score {
                        delta.reprioritized_entities.push(entity.id.clone());
                    }
                }
                None => {
                    model.entities.push(TrackedObservationEntity {
                        entity: entity.clone(),
                        smoothed_confidence: entity.confidence,
                        priority_score: compute_priority_score(entity, entity.confidence),
                        first_seen_frame_id: frame.metadata.frame_id,
                        last_seen_frame_id: frame.metadata.frame_id,
                        missed_frames: 0,
                        status: TrackedEntityStatus::Active,
                    });
                    delta.new_entities.push(entity.id.clone());
                }
            }
        }

        for tracked in &mut model.entities {
            if seen_ids.iter().any(|id| id == &tracked.entity.id) {
                continue;
            }
            tracked.missed_frames += 1;
            if tracked.missed_frames >= self.config.lost_after_missed_frames
                && tracked.status != TrackedEntityStatus::Lost
            {
                tracked.status = TrackedEntityStatus::Lost;
                delta.lost_entities.push(tracked.entity.id.clone());
            }
        }

        model
            .entities
            .retain(|tracked| tracked.missed_frames < self.config.drop_after_missed_frames);

        delta
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

fn unique_detector_kinds(frame: &ObservationFrame) -> Vec<DetectorKind> {
    let mut kinds = Vec::new();
    for detector in &frame.detectors {
        if !kinds.contains(&detector.kind) {
            kinds.push(detector.kind);
        }
    }
    kinds
}

fn smooth_confidence(previous: f32, current: f32, alpha: f32) -> f32 {
    (current * alpha) + (previous * (1.0 - alpha))
}

fn compute_priority_score(entity: &ObservationEntity, smoothed_confidence: f32) -> u32 {
    let base = match entity.kind {
        EntityKind::Prompt => 110,
        EntityKind::Interactable => 100,
        EntityKind::Collectible => 90,
        EntityKind::Player => 80,
        EntityKind::Region => 70,
        EntityKind::Waypoint => 60,
        EntityKind::VisualMarker => 55,
        EntityKind::Obstacle => 50,
        EntityKind::Camera => 40,
    };
    let tag_bonus = entity
        .tags
        .iter()
        .map(|tag| match tag.as_str() {
            "priority" => 20,
            "resource" => 10,
            "patrol" => 8,
            _ => 0,
        })
        .sum::<u32>();

    base + tag_bonus + (smoothed_confidence.clamp(0.0, 1.0) * 100.0) as u32
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
    use winr_types::{AdvancedBinaryPayloadRef, AdvancedIpcTransportKind, AdvancedPayloadEncoding};

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
            render_details: Some(RenderObservationDetails {
                boundary: RenderHookBoundary::DxgiPresent,
                timing: RenderFrameTiming {
                    present_timestamp_ms: 1000,
                    frame_interval_ms: Some(16),
                    capture_latency_ms: Some(2),
                },
                availability: RenderFrameAvailability {
                    frame_ready: true,
                    present_count: 77,
                    dropped_since_last_capture: 0,
                },
                sample_regions: vec![RenderSampleRegion {
                    id: "center-sample".to_string(),
                    left: 800,
                    top: 400,
                    width: 320,
                    height: 240,
                    payload: Some(sample_payload("sample-7")),
                }],
                debug_overlay: Some(RenderDebugOverlaySurface {
                    development_only: true,
                    commands: vec![DebugOverlayCommand {
                        label: "target box".to_string(),
                        kind: OverlayKind::BoundingBoxes,
                        left: 790,
                        top: 390,
                        width: 340,
                        height: 260,
                    }],
                }),
                intended_uses: vec![
                    RenderSceneUseCase::VisibleSceneUnderstanding,
                    RenderSceneUseCase::TemplateDetection,
                    RenderSceneUseCase::ActionCorrelation,
                ],
                does_not_claim_game_state_api: true,
                does_not_claim_background_input_channel: true,
            }),
            memory_details: None,
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
        assert!(json.contains("\"dxgi_present\""));
    }

    #[test]
    fn from_update_normalizes_desktop_screenshot_source() {
        let frame = ObservationFrame::from_update(
            sample_context(),
            AdvancedObservationUpdate {
                frame_id: 44,
                source: "desktop".to_string(),
                detail: "captured desktop screenshot".to_string(),
                timestamp_ms: Some(1000),
                freshness_ms: Some(16),
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

        assert_eq!(
            frame.metadata.source,
            ObservationSourceKind::DesktopScreenshot
        );
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
            render_details: None,
            memory_details: None,
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
            render_details: None,
            memory_details: Some(MemoryObservationDetails {
                schema_version: MemorySchemaVersion::V1,
                snapshot_id: "snap-2".to_string(),
                intended_uses: vec![
                    MemoryObservationUseCase::PlayerState,
                    MemoryObservationUseCase::InteractableDiscovery,
                ],
                player_state: Some(MemoryPlayerState {
                    world_position_millimeters: Some([10000, 0, -4000]),
                    velocity_millimeters_per_second: Some([500, 0, 0]),
                    movement_state: Some(ObservationMovementState::Walking),
                    active_tool: Some("pickaxe".to_string()),
                    active_modes: vec!["harvesting".to_string()],
                }),
                camera_state: Some(MemoryCameraState {
                    yaw_milli_degrees: Some(90000),
                    pitch_milli_degrees: Some(-12000),
                    field_of_view_milli_degrees: Some(70000),
                    mode: Some("third_person".to_string()),
                }),
                prompts: vec![MemoryPromptState {
                    id: "prompt-1".to_string(),
                    label: "Press E".to_string(),
                    visible: true,
                    distance_millimeters: Some(900),
                }],
                nearby_objects: vec![MemoryObjectState {
                    id: "rock-1".to_string(),
                    kind: "resource_node".to_string(),
                    label: "Rock".to_string(),
                    world_position_millimeters: Some([10800, 0, -3900]),
                    distance_millimeters: Some(1200),
                    interactable: true,
                }],
                raw_layout_hidden: true,
            }),
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
        assert!(
            frames
                .iter()
                .any(|frame| frame.metadata.source == ObservationSourceKind::DesktopScreenshot)
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.metadata.source == ObservationSourceKind::MemoryState)
        );
    }

    #[test]
    fn render_details_capture_boundary_timing_and_limits() {
        let frame = ObservationFrame::from_update(
            sample_context(),
            AdvancedObservationUpdate {
                frame_id: 55,
                source: "render-hook".to_string(),
                detail: "captured at present boundary".to_string(),
                timestamp_ms: Some(555),
                freshness_ms: Some(12),
                payload: Some(sample_payload("render-55")),
            },
            ObservationSourceData::RenderHookFrame {
                frame: ObservationFrameHandle {
                    payload: sample_payload("render-55"),
                    width: 1280,
                    height: 720,
                    pixel_format: ObservationPixelFormat::Bgra8,
                    row_stride_bytes: Some(5120),
                },
            },
        )
        .with_render_details(RenderObservationDetails {
            boundary: RenderHookBoundary::D3d11Present,
            timing: RenderFrameTiming {
                present_timestamp_ms: 555,
                frame_interval_ms: Some(16),
                capture_latency_ms: Some(3),
            },
            availability: RenderFrameAvailability {
                frame_ready: true,
                present_count: 10,
                dropped_since_last_capture: 1,
            },
            sample_regions: vec![RenderSampleRegion {
                id: "ore-cluster".to_string(),
                left: 200,
                top: 300,
                width: 128,
                height: 128,
                payload: Some(sample_payload("sample-55")),
            }],
            debug_overlay: Some(RenderDebugOverlaySurface {
                development_only: true,
                commands: vec![DebugOverlayCommand {
                    label: "ore highlight".to_string(),
                    kind: OverlayKind::Heatmap,
                    left: 180,
                    top: 280,
                    width: 180,
                    height: 180,
                }],
            }),
            intended_uses: vec![
                RenderSceneUseCase::VisibleSceneUnderstanding,
                RenderSceneUseCase::ObjectDetection,
            ],
            does_not_claim_game_state_api: true,
            does_not_claim_background_input_channel: true,
        });

        let details = frame
            .render_details
            .expect("render details should be attached");
        assert_eq!(details.boundary, RenderHookBoundary::D3d11Present);
        assert_eq!(details.availability.present_count, 10);
        assert!(details.does_not_claim_game_state_api);
        assert!(details.does_not_claim_background_input_channel);
        assert!(
            details
                .debug_overlay
                .expect("overlay should exist")
                .development_only
        );
    }

    #[test]
    fn memory_details_version_and_normalized_state_are_preserved() {
        let frame = ObservationFrame::from_update(
            sample_context(),
            AdvancedObservationUpdate {
                frame_id: 66,
                source: "memory-reader".to_string(),
                detail: "snapshot captured".to_string(),
                timestamp_ms: Some(333),
                freshness_ms: Some(8),
                payload: None,
            },
            ObservationSourceData::MemoryState {
                snapshot_id: "snap-66".to_string(),
                state_fields: vec![
                    ObservationStateField {
                        key: "player.position".to_string(),
                        value: "[10,0,-4]".to_string(),
                    },
                    ObservationStateField {
                        key: "prompt.visible".to_string(),
                        value: "true".to_string(),
                    },
                ],
            },
        )
        .with_memory_details(MemoryObservationDetails {
            schema_version: MemorySchemaVersion::V1,
            snapshot_id: "snap-66".to_string(),
            intended_uses: vec![
                MemoryObservationUseCase::PlayerState,
                MemoryObservationUseCase::PromptState,
                MemoryObservationUseCase::ObjectInventory,
            ],
            player_state: Some(MemoryPlayerState {
                world_position_millimeters: Some([10000, 0, -4000]),
                velocity_millimeters_per_second: Some([0, 0, 0]),
                movement_state: Some(ObservationMovementState::Idle),
                active_tool: Some("pickaxe".to_string()),
                active_modes: vec!["harvesting".to_string()],
            }),
            camera_state: Some(MemoryCameraState {
                yaw_milli_degrees: Some(90000),
                pitch_milli_degrees: Some(-10000),
                field_of_view_milli_degrees: Some(70000),
                mode: Some("third_person".to_string()),
            }),
            prompts: vec![MemoryPromptState {
                id: "ore-prompt".to_string(),
                label: "Press E".to_string(),
                visible: true,
                distance_millimeters: Some(850),
            }],
            nearby_objects: vec![MemoryObjectState {
                id: "ore-rock".to_string(),
                kind: "resource_node".to_string(),
                label: "Ore Rock".to_string(),
                world_position_millimeters: Some([10800, 0, -3900]),
                distance_millimeters: Some(1200),
                interactable: true,
            }],
            raw_layout_hidden: true,
        });

        let details = frame
            .memory_details
            .expect("memory details should be attached");
        assert_eq!(details.schema_version, MemorySchemaVersion::V1);
        assert!(details.raw_layout_hidden);
        assert_eq!(details.prompts.len(), 1);
        assert_eq!(details.nearby_objects[0].kind, "resource_node");
    }

    #[test]
    fn world_model_tracks_smooths_and_prioritizes_entities() {
        let mut tracker = WorldModelTracker::default();
        let first = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::MemoryState,
                frame_id: 1,
                timestamp_ms: 10,
                freshness_ms: 5,
            },
            source_data: ObservationSourceData::MemoryState {
                snapshot_id: "snap-1".to_string(),
                state_fields: Vec::new(),
            },
            render_details: None,
            memory_details: None,
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: vec![
                DetectorDescriptor {
                    id: "mem-entities".to_string(),
                    name: "Memory Entities".to_string(),
                    kind: DetectorKind::MemoryEntity,
                },
                DetectorDescriptor {
                    id: "ocr-prompt".to_string(),
                    name: "OCR Prompt".to_string(),
                    kind: DetectorKind::Ocr,
                },
            ],
            detector_overlays: Vec::new(),
            entities: vec![
                ObservationEntity {
                    id: "rock-1".to_string(),
                    kind: EntityKind::Interactable,
                    label: "Rock".to_string(),
                    confidence: 0.9,
                    tags: vec!["resource".to_string()],
                },
                ObservationEntity {
                    id: "prompt-1".to_string(),
                    kind: EntityKind::Prompt,
                    label: "Press E".to_string(),
                    confidence: 0.7,
                    tags: vec!["priority".to_string()],
                },
            ],
            notes: Vec::new(),
        };

        let delta = tracker.update(&first);
        assert_eq!(delta.new_entities.len(), 2);
        let model = tracker.model.as_ref().expect("model should exist");
        assert_eq!(model.detector_kinds.len(), 2);
        assert_eq!(
            model
                .best_entity(EntityKind::Prompt)
                .expect("prompt should exist")
                .entity
                .id,
            "prompt-1"
        );

        let second = ObservationFrame {
            metadata: ObservationMetadata {
                frame_id: 2,
                ..first.metadata.clone()
            },
            entities: vec![ObservationEntity {
                id: "rock-1".to_string(),
                kind: EntityKind::Interactable,
                label: "Rock".to_string(),
                confidence: 0.4,
                tags: vec!["resource".to_string()],
            }],
            ..first.clone()
        };
        let delta = tracker.update(&second);
        assert_eq!(delta.lost_entities.len(), 0);
        let rock = tracker
            .model
            .as_ref()
            .expect("model should exist")
            .best_entity(EntityKind::Interactable)
            .expect("rock should exist");
        assert!(rock.smoothed_confidence > 0.4);
        assert_eq!(rock.status, TrackedEntityStatus::Active);
    }

    #[test]
    fn world_model_marks_lost_and_reacquired_entities() {
        let mut tracker = WorldModelTracker::default();
        let base = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::RenderHookFrame,
                frame_id: 1,
                timestamp_ms: 10,
                freshness_ms: 5,
            },
            source_data: ObservationSourceData::RenderHookFrame {
                frame: ObservationFrameHandle {
                    payload: sample_payload("frame"),
                    width: 10,
                    height: 10,
                    pixel_format: ObservationPixelFormat::Bgra8,
                    row_stride_bytes: None,
                },
            },
            render_details: None,
            memory_details: None,
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: vec![ObservationEntity {
                id: "marker-1".to_string(),
                kind: EntityKind::VisualMarker,
                label: "Marker".to_string(),
                confidence: 0.8,
                tags: Vec::new(),
            }],
            notes: Vec::new(),
        };

        tracker.update(&base);
        tracker.update(&ObservationFrame {
            metadata: ObservationMetadata {
                frame_id: 2,
                ..base.metadata.clone()
            },
            entities: Vec::new(),
            ..base.clone()
        });
        let lost = tracker.update(&ObservationFrame {
            metadata: ObservationMetadata {
                frame_id: 3,
                ..base.metadata.clone()
            },
            entities: Vec::new(),
            ..base.clone()
        });
        assert_eq!(lost.lost_entities, vec!["marker-1".to_string()]);

        let reacquired = tracker.update(&ObservationFrame {
            metadata: ObservationMetadata {
                frame_id: 4,
                ..base.metadata.clone()
            },
            ..base.clone()
        });
        assert_eq!(reacquired.reacquired_entities, vec!["marker-1".to_string()]);
        let tracked = tracker
            .model
            .as_ref()
            .expect("model should exist")
            .best_entity(EntityKind::VisualMarker)
            .expect("marker should exist");
        assert_eq!(tracked.status, TrackedEntityStatus::Reacquired);
    }

    #[test]
    fn freshness_assessment_and_replay_tape_flag_stale_frames() {
        let fresh = ObservationFrame {
            target: sample_target(),
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::MemoryState,
                frame_id: 1,
                timestamp_ms: 1000,
                freshness_ms: 20,
            },
            source_data: ObservationSourceData::MemoryState {
                snapshot_id: "snap-1".to_string(),
                state_fields: Vec::new(),
            },
            render_details: None,
            memory_details: None,
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: Vec::new(),
            notes: Vec::new(),
        };
        let stale = ObservationFrame {
            metadata: ObservationMetadata {
                frame_id: 2,
                freshness_ms: 180,
                ..fresh.metadata.clone()
            },
            notes: vec!["late frame".to_string()],
            ..fresh.clone()
        };
        let policy = ObservationFreshnessPolicy::default();
        let mut tape = ObservationReplayTape::default();
        tape.push(fresh.clone());
        tape.push(stale.clone());

        let fresh_assessment = fresh.assess_freshness(&policy);
        let stale_assessment = stale.assess_freshness(&policy);

        assert_eq!(fresh_assessment.status, ObservationFreshnessStatus::Fresh);
        assert_eq!(stale_assessment.status, ObservationFreshnessStatus::Stale);
        assert_eq!(
            tape.latest()
                .expect("latest frame should exist")
                .metadata
                .frame_id,
            2
        );
        assert_eq!(tape.stale_frames(&policy).len(), 1);
        assert_eq!(
            tape.frame(2).expect("frame id 2 should exist").notes[0],
            "late frame"
        );
    }
}
