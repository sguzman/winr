use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use winr_perception::{DetectorDescriptor, EntityKind, ObservationFrame, WorldModel};
use winr_types::{AdvancedBackendCapabilities, AdvancedExecutionReason, AdvancedProfileBackend};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTaskKind {
    SearchFor,
    Approach,
    PatrolRegion,
    InteractUntil,
    WaitForPrompt,
    ResumePreviousTask,
    RecoverIfStuck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowIntentKind {
    MoveForward,
    MoveBackward,
    StrafeLeft,
    StrafeRight,
    Turn,
    LookPitch,
    Jump,
    Interact,
    StopMotion,
    ApproachTarget,
    WalkToRegionOrEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputSinkKind {
    Win32Foreground,
    Win32Message,
    InjectedRawInput,
    SemanticInternalAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticInputTarget {
    CurrentTarget,
    EntityId { entity_id: String },
    RegionId { region_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticInputAction {
    MoveForward { duration_ms: u64 },
    MoveBackward { duration_ms: u64 },
    StrafeLeft { duration_ms: u64 },
    StrafeRight { duration_ms: u64 },
    Turn { delta_yaw_milli_degrees: i32 },
    LookPitch { delta_pitch_milli_degrees: i32 },
    Jump,
    Interact,
    Hold { action: String, duration_ms: u64 },
    StopMotion,
    Approach { target: SemanticInputTarget },
    WalkTo { target: SemanticInputTarget },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputSinkPreference {
    #[serde(default)]
    pub ordered_sinks: Vec<InputSinkKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputSinkMapping {
    pub sink: InputSinkKind,
    pub action: SemanticInputAction,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowInputPlan {
    #[serde(default)]
    pub mappings: Vec<InputSinkMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowTaskDefinition {
    pub id: String,
    pub name: String,
    pub kind: WorkflowTaskKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowIntentDefinition {
    pub kind: WorkflowIntentKind,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_action: Option<SemanticInputAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_preference: Option<InputSinkPreference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTraceEventKind {
    ObservationAccepted,
    TaskSelected,
    IntentIssued,
    RecoveryTriggered,
    PlanBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackManifest {
    pub id: String,
    pub name: String,
    pub target_family: String,
    #[serde(default)]
    pub backend_preferences: Vec<AdvancedProfileBackend>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct AppPackRegistry {
    #[serde(default)]
    pub packs: Vec<AppPackManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackManifestFile {
    pub id: String,
    pub name: String,
    pub target_family: String,
    #[serde(default)]
    pub backend_preferences: Vec<AdvancedProfileBackend>,
    pub detectors_file: String,
    pub workflows_file: String,
    pub movement_file: String,
    pub profile_presets_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackDetectorPreset {
    pub id: String,
    pub name: String,
    pub detector: DeclarativeDetector,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackMovementTuning {
    pub turn_step_milli_degrees: i32,
    pub arrival_threshold_millimeters: u32,
    pub move_step_ms: u64,
    pub patrol_region_radius_millimeters: u32,
    pub stuck_frame_window: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackProfilePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub workflow_task: WorkflowTaskKind,
    #[serde(default)]
    pub backend_preferences: Vec<AdvancedProfileBackend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_title_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_exe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackDetectorFile {
    #[serde(default)]
    pub detectors: Vec<AppPackDetectorPreset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackWorkflowFile {
    #[serde(default)]
    pub tasks: Vec<WorkflowTaskRecipe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackProfilePresetFile {
    #[serde(default)]
    pub presets: Vec<AppPackProfilePreset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppPackBundle {
    pub manifest: AppPackManifest,
    #[serde(default)]
    pub detectors: Vec<AppPackDetectorPreset>,
    #[serde(default)]
    pub task_recipes: Vec<WorkflowTaskRecipe>,
    pub movement_tuning: AppPackMovementTuning,
    #[serde(default)]
    pub profile_presets: Vec<AppPackProfilePreset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowPlan {
    pub pack_id: String,
    pub task: WorkflowTaskDefinition,
    #[serde(default)]
    pub required_detectors: Vec<DetectorDescriptor>,
    #[serde(default)]
    pub required_entity_kinds: Vec<EntityKind>,
    #[serde(default)]
    pub intents: Vec<WorkflowIntentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowTraceEvent {
    pub sequence: u64,
    pub kind: WorkflowTraceEventKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct WorkflowExecutionTrace {
    #[serde(default)]
    pub events: Vec<WorkflowTraceEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDslVersion {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Detect,
    Act,
    Branch,
    Wait,
    Recover,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowConditionOperator {
    Exists,
    NotExists,
    ConfidenceAtLeast,
    LostForFramesAtLeast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeclarativeDetector {
    TemplateMatch {
        detector_id: String,
        entity_kind: EntityKind,
    },
    ColorCluster {
        detector_id: String,
        entity_kind: EntityKind,
    },
    Ocr {
        detector_id: String,
        entity_kind: EntityKind,
    },
    ObjectDetection {
        detector_id: String,
        entity_kind: EntityKind,
    },
    MemoryEntity {
        detector_id: String,
        entity_kind: EntityKind,
    },
    RenderEntity {
        detector_id: String,
        entity_kind: EntityKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowCondition {
    pub entity_kind: EntityKind,
    pub operator: WorkflowConditionOperator,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRetryPolicy {
    pub max_attempts: u32,
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowCooldown {
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowBackendPreference {
    #[serde(default)]
    pub preferred_backends: Vec<AdvancedProfileBackend>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowRecoveryStep {
    RetryCurrentNode,
    RunController {
        controller: NavigationControllerKind,
    },
    EmitAction {
        action: SemanticInputAction,
    },
    ResumePreviousTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStep {
    Detector { detector: DeclarativeDetector },
    Action { action: SemanticInputAction },
    Condition { condition: WorkflowCondition },
    Recovery { step: WorkflowRecoveryStep },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowNode {
    pub id: String,
    pub name: String,
    pub kind: WorkflowNodeKind,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub next: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<WorkflowRetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown: Option<WorkflowCooldown>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowTaskRecipe {
    pub id: String,
    pub kind: WorkflowTaskKind,
    #[serde(default)]
    pub detectors: Vec<DeclarativeDetector>,
    #[serde(default)]
    pub recovery: Vec<WorkflowRecoveryStep>,
    #[serde(default)]
    pub action_graph: Vec<WorkflowNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_preference: Option<WorkflowBackendPreference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowDslDocument {
    pub version: WorkflowDslVersion,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tasks: Vec<WorkflowTaskRecipe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NavigationControllerKind {
    RotateTowardTarget,
    ApproachUntilThreshold,
    LocalWaypointFollow,
    BoundedRegionPatrol,
    NoProgressRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDecisionKind {
    Continue,
    Arrived,
    Recovering,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NavigationDecision {
    pub kind: NavigationDecisionKind,
    #[serde(default)]
    pub actions: Vec<SemanticInputAction>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HeadingControlState {
    pub desired_yaw_milli_degrees: i32,
    pub current_yaw_milli_degrees: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MovementCorrectionState {
    pub desired_distance_millimeters: u32,
    pub current_distance_millimeters: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArrivalState {
    pub within_threshold: bool,
    pub threshold_millimeters: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProgressSample {
    pub frame_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_position_millimeters: Option<[i32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_distance_millimeters: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ControllerMemory {
    #[serde(default)]
    pub progress_samples: Vec<ProgressSample>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NavigationContext {
    pub world_model: WorldModel,
    pub frame_id: u64,
    pub controller_memory: ControllerMemory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NavigationControllerConfig {
    pub arrival_threshold_millimeters: u32,
    pub heading_tolerance_milli_degrees: i32,
    pub move_step_ms: u64,
    pub turn_step_milli_degrees: i32,
    pub stuck_distance_epsilon_millimeters: u32,
    pub stuck_frame_window: usize,
}

impl Default for NavigationControllerConfig {
    fn default() -> Self {
        Self {
            arrival_threshold_millimeters: 900,
            heading_tolerance_milli_degrees: 5000,
            move_step_ms: 150,
            turn_step_milli_degrees: 12000,
            stuck_distance_epsilon_millimeters: 80,
            stuck_frame_window: 3,
        }
    }
}

pub trait NavigationController {
    fn kind(&self) -> NavigationControllerKind;
    fn decide(
        &self,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> NavigationDecision;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RotateTowardTargetController;

#[derive(Debug, Clone)]
pub struct ApproachUntilThresholdController {
    pub target_entity_id: String,
}

#[derive(Debug, Clone)]
pub struct BoundedRegionPatrolController {
    pub region_entity_id: String,
    pub waypoint_entity_ids: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoProgressRecoveryController;

pub trait AppWorkflowPack {
    fn manifest(&self) -> AppPackManifest;
    fn supported_tasks(&self) -> Vec<WorkflowTaskDefinition>;
    fn default_plan(&self, task: WorkflowTaskKind) -> Option<WorkflowPlan>;
}

pub trait WorkflowPlanner {
    fn can_plan(&self, frame: &ObservationFrame) -> bool;
    fn plan(&self, frame: &ObservationFrame, task: WorkflowTaskKind) -> Option<WorkflowPlan>;
}

impl DeclarativeDetector {
    pub fn to_descriptor(&self) -> DetectorDescriptor {
        match self {
            Self::TemplateMatch { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::TemplateMatch,
            },
            Self::ColorCluster { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::ColorCluster,
            },
            Self::Ocr { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::Ocr,
            },
            Self::ObjectDetection { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::ObjectDetection,
            },
            Self::MemoryEntity { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::MemoryEntity,
            },
            Self::RenderEntity { detector_id, .. } => DetectorDescriptor {
                id: detector_id.clone(),
                name: detector_id.clone(),
                kind: winr_perception::DetectorKind::RenderEntity,
            },
        }
    }

    pub fn entity_kind(&self) -> EntityKind {
        match self {
            Self::TemplateMatch { entity_kind, .. }
            | Self::ColorCluster { entity_kind, .. }
            | Self::Ocr { entity_kind, .. }
            | Self::ObjectDetection { entity_kind, .. }
            | Self::MemoryEntity { entity_kind, .. }
            | Self::RenderEntity { entity_kind, .. } => *entity_kind,
        }
    }
}

impl WorkflowDslDocument {
    pub fn task(&self, kind: WorkflowTaskKind) -> Option<&WorkflowTaskRecipe> {
        self.tasks.iter().find(|task| task.kind == kind)
    }
}

impl AppPackBundle {
    pub fn task_recipe(&self, kind: WorkflowTaskKind) -> Option<&WorkflowTaskRecipe> {
        self.task_recipes.iter().find(|task| task.kind == kind)
    }

    pub fn detector(&self, id: &str) -> Option<&AppPackDetectorPreset> {
        self.detectors.iter().find(|detector| detector.id == id)
    }

    pub fn profile_preset(&self, id: &str) -> Option<&AppPackProfilePreset> {
        self.profile_presets.iter().find(|preset| preset.id == id)
    }
}

pub fn load_app_pack_from_dir(dir: &Path) -> Result<AppPackBundle, String> {
    let manifest_path = dir.join("pack.toml");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read '{}': {error}", manifest_path.display()))?;
    let manifest_file: PackManifestFile = toml::from_str(&manifest_text)
        .map_err(|error| format!("failed to parse '{}': {error}", manifest_path.display()))?;

    let detectors_file: AppPackDetectorFile = read_pack_toml(dir, &manifest_file.detectors_file)?;
    let workflows_file: AppPackWorkflowFile = read_pack_toml(dir, &manifest_file.workflows_file)?;
    let movement_tuning: AppPackMovementTuning = read_pack_toml(dir, &manifest_file.movement_file)?;
    let profile_presets_file: AppPackProfilePresetFile =
        read_pack_toml(dir, &manifest_file.profile_presets_file)?;

    Ok(AppPackBundle {
        manifest: AppPackManifest {
            id: manifest_file.id,
            name: manifest_file.name,
            target_family: manifest_file.target_family,
            backend_preferences: manifest_file.backend_preferences,
        },
        detectors: detectors_file.detectors,
        task_recipes: workflows_file.tasks,
        movement_tuning,
        profile_presets: profile_presets_file.presets,
    })
}

fn read_pack_toml<T>(dir: &Path, relative_path: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let path = dir.join(relative_path);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("failed to parse '{}': {error}", path.display()))
}

impl WorkflowTaskRecipe {
    pub fn compile_plan(&self) -> WorkflowPlan {
        WorkflowPlan {
            pack_id: self.id.clone(),
            task: WorkflowTaskDefinition {
                id: self.id.clone(),
                name: self.id.clone(),
                kind: self.kind,
            },
            required_detectors: self
                .detectors
                .iter()
                .map(DeclarativeDetector::to_descriptor)
                .collect(),
            required_entity_kinds: self
                .detectors
                .iter()
                .map(DeclarativeDetector::entity_kind)
                .collect(),
            intents: self
                .action_graph
                .iter()
                .flat_map(|node| node.steps.iter())
                .filter_map(|step| match step {
                    WorkflowStep::Action { action } => Some(WorkflowIntentDefinition {
                        kind: workflow_intent_kind_for_action(action),
                        description: format!("compiled from node action '{}'", self.id),
                        semantic_action: Some(action.clone()),
                        sink_preference: None,
                    }),
                    _ => None,
                })
                .collect(),
        }
    }

    pub fn evaluate_conditions(&self, world_model: &WorldModel) -> bool {
        self.action_graph
            .iter()
            .flat_map(|node| node.steps.iter())
            .all(|step| match step {
                WorkflowStep::Condition { condition } => evaluate_condition(condition, world_model),
                _ => true,
            })
    }
}

pub fn next_nodes<'a>(recipe: &'a WorkflowTaskRecipe, node_id: &str) -> Vec<&'a WorkflowNode> {
    let Some(node) = recipe.action_graph.iter().find(|node| node.id == node_id) else {
        return Vec::new();
    };

    node.next
        .iter()
        .filter_map(|next_id| {
            recipe
                .action_graph
                .iter()
                .find(|candidate| candidate.id == *next_id)
        })
        .collect()
}

fn evaluate_condition(condition: &WorkflowCondition, world_model: &WorldModel) -> bool {
    let entity = world_model.best_entity(condition.entity_kind);
    match condition.operator {
        WorkflowConditionOperator::Exists => entity.is_some(),
        WorkflowConditionOperator::NotExists => entity.is_none(),
        WorkflowConditionOperator::ConfidenceAtLeast => entity.is_some_and(|entity| {
            entity.smoothed_confidence * 100.0 >= condition.threshold.unwrap_or(0) as f32
        }),
        WorkflowConditionOperator::LostForFramesAtLeast => {
            entity.is_some_and(|entity| entity.missed_frames >= condition.threshold.unwrap_or(0))
        }
    }
}

fn workflow_intent_kind_for_action(action: &SemanticInputAction) -> WorkflowIntentKind {
    match action {
        SemanticInputAction::MoveForward { .. } => WorkflowIntentKind::MoveForward,
        SemanticInputAction::MoveBackward { .. } => WorkflowIntentKind::MoveBackward,
        SemanticInputAction::StrafeLeft { .. } => WorkflowIntentKind::StrafeLeft,
        SemanticInputAction::StrafeRight { .. } => WorkflowIntentKind::StrafeRight,
        SemanticInputAction::Turn { .. } => WorkflowIntentKind::Turn,
        SemanticInputAction::LookPitch { .. } => WorkflowIntentKind::LookPitch,
        SemanticInputAction::Jump => WorkflowIntentKind::Jump,
        SemanticInputAction::Interact => WorkflowIntentKind::Interact,
        SemanticInputAction::Hold { .. } => WorkflowIntentKind::Interact,
        SemanticInputAction::StopMotion => WorkflowIntentKind::StopMotion,
        SemanticInputAction::Approach { .. } => WorkflowIntentKind::ApproachTarget,
        SemanticInputAction::WalkTo { .. } => WorkflowIntentKind::WalkToRegionOrEntity,
    }
}

pub fn select_prioritized_entity_id(world_model: &WorldModel, kind: EntityKind) -> Option<String> {
    world_model
        .best_entity(kind)
        .map(|entity| entity.entity.id.clone())
}

impl NavigationContext {
    pub fn best_entity_by_kind(
        &self,
        kind: EntityKind,
    ) -> Option<&winr_perception::TrackedObservationEntity> {
        self.world_model.best_entity(kind)
    }

    pub fn entity_distance_millimeters(&self, entity_id: &str) -> Option<u32> {
        self.world_model
            .entities
            .iter()
            .find(|entity| entity.entity.id == entity_id)
            .and_then(entity_distance_millimeters)
    }

    pub fn current_yaw_milli_degrees(&self) -> Option<i32> {
        self.world_model
            .entities
            .iter()
            .find(|entity| entity.entity.kind == EntityKind::Camera)
            .and_then(|_| None)
            .or_else(|| self.world_model.notes.iter().find_map(|_| None))
    }
}

impl NavigationController for RotateTowardTargetController {
    fn kind(&self) -> NavigationControllerKind {
        NavigationControllerKind::RotateTowardTarget
    }

    fn decide(
        &self,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> NavigationDecision {
        let Some(camera_yaw) = navigation_camera_yaw(&context.world_model) else {
            return NavigationDecision {
                kind: NavigationDecisionKind::Blocked,
                actions: Vec::new(),
                detail: "camera yaw unavailable for heading control".to_string(),
            };
        };
        let Some(desired_yaw) = navigation_target_yaw(&context.world_model) else {
            return NavigationDecision {
                kind: NavigationDecisionKind::Blocked,
                actions: Vec::new(),
                detail: "target yaw unavailable for heading control".to_string(),
            };
        };

        let delta = desired_yaw - camera_yaw;
        if delta.abs() <= config.heading_tolerance_milli_degrees {
            return NavigationDecision {
                kind: NavigationDecisionKind::Continue,
                actions: Vec::new(),
                detail: "heading already within tolerance".to_string(),
            };
        }

        NavigationDecision {
            kind: NavigationDecisionKind::Continue,
            actions: vec![SemanticInputAction::Turn {
                delta_yaw_milli_degrees: delta.signum() * config.turn_step_milli_degrees,
            }],
            detail: format!("rotate toward target by correcting yaw delta {delta}"),
        }
    }
}

impl NavigationController for ApproachUntilThresholdController {
    fn kind(&self) -> NavigationControllerKind {
        NavigationControllerKind::ApproachUntilThreshold
    }

    fn decide(
        &self,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> NavigationDecision {
        let Some(distance) = context.entity_distance_millimeters(&self.target_entity_id) else {
            return NavigationDecision {
                kind: NavigationDecisionKind::Blocked,
                actions: Vec::new(),
                detail: format!("target '{}' is unavailable", self.target_entity_id),
            };
        };
        if distance <= config.arrival_threshold_millimeters {
            return NavigationDecision {
                kind: NavigationDecisionKind::Arrived,
                actions: vec![SemanticInputAction::StopMotion],
                detail: format!("arrived within threshold at distance {} mm", distance),
            };
        }

        NavigationDecision {
            kind: NavigationDecisionKind::Continue,
            actions: vec![SemanticInputAction::Approach {
                target: SemanticInputTarget::EntityId {
                    entity_id: self.target_entity_id.clone(),
                },
            }],
            detail: format!(
                "approach target '{}' at {} mm",
                self.target_entity_id, distance
            ),
        }
    }
}

impl NavigationController for BoundedRegionPatrolController {
    fn kind(&self) -> NavigationControllerKind {
        NavigationControllerKind::BoundedRegionPatrol
    }

    fn decide(
        &self,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> NavigationDecision {
        let Some(region_distance) = context.entity_distance_millimeters(&self.region_entity_id)
        else {
            return NavigationDecision {
                kind: NavigationDecisionKind::Blocked,
                actions: Vec::new(),
                detail: format!("region '{}' is unavailable", self.region_entity_id),
            };
        };
        if region_distance > config.arrival_threshold_millimeters * 2 {
            return NavigationDecision {
                kind: NavigationDecisionKind::Continue,
                actions: vec![SemanticInputAction::WalkTo {
                    target: SemanticInputTarget::RegionId {
                        region_id: self.region_entity_id.clone(),
                    },
                }],
                detail: format!(
                    "return to patrol region '{}' from {} mm away",
                    self.region_entity_id, region_distance
                ),
            };
        }

        if let Some(next_waypoint) = self
            .waypoint_entity_ids
            .iter()
            .find(|waypoint| context.entity_distance_millimeters(waypoint).is_some())
        {
            return NavigationDecision {
                kind: NavigationDecisionKind::Continue,
                actions: vec![SemanticInputAction::WalkTo {
                    target: SemanticInputTarget::EntityId {
                        entity_id: next_waypoint.clone(),
                    },
                }],
                detail: format!("patrol toward waypoint '{}'", next_waypoint),
            };
        }

        NavigationDecision {
            kind: NavigationDecisionKind::Continue,
            actions: vec![SemanticInputAction::WalkTo {
                target: SemanticInputTarget::RegionId {
                    region_id: self.region_entity_id.clone(),
                },
            }],
            detail: format!("patrol inside region '{}'", self.region_entity_id),
        }
    }
}

impl NavigationController for NoProgressRecoveryController {
    fn kind(&self) -> NavigationControllerKind {
        NavigationControllerKind::NoProgressRecovery
    }

    fn decide(
        &self,
        context: &NavigationContext,
        config: &NavigationControllerConfig,
    ) -> NavigationDecision {
        if !is_stuck(&context.controller_memory, config) {
            return NavigationDecision {
                kind: NavigationDecisionKind::Continue,
                actions: Vec::new(),
                detail: "movement progress is still changing".to_string(),
            };
        }

        NavigationDecision {
            kind: NavigationDecisionKind::Recovering,
            actions: vec![
                SemanticInputAction::StopMotion,
                SemanticInputAction::StrafeRight {
                    duration_ms: config.move_step_ms,
                },
                SemanticInputAction::Jump,
            ],
            detail: "stuck detected, issuing no-progress recovery sequence".to_string(),
        }
    }
}

pub fn patrol_while_scanning_decision(
    context: &NavigationContext,
    patrol: &BoundedRegionPatrolController,
) -> NavigationDecision {
    if let Some(target) = context.best_entity_by_kind(EntityKind::Interactable) {
        return NavigationDecision {
            kind: NavigationDecisionKind::Continue,
            actions: vec![SemanticInputAction::Approach {
                target: SemanticInputTarget::EntityId {
                    entity_id: target.entity.id.clone(),
                },
            }],
            detail: format!("scan found target '{}', interrupt patrol", target.entity.id),
        };
    }

    patrol.decide(context, &NavigationControllerConfig::default())
}

pub fn interact_when_prompt_appears_decision(context: &NavigationContext) -> NavigationDecision {
    if let Some(prompt) = context.best_entity_by_kind(EntityKind::Prompt) {
        return NavigationDecision {
            kind: NavigationDecisionKind::Continue,
            actions: vec![SemanticInputAction::Interact],
            detail: format!("prompt '{}' visible, interact", prompt.entity.id),
        };
    }

    NavigationDecision {
        kind: NavigationDecisionKind::Blocked,
        actions: Vec::new(),
        detail: "prompt not visible yet".to_string(),
    }
}

pub fn resume_patrol_after_interaction_decision(
    context: &NavigationContext,
    patrol: &BoundedRegionPatrolController,
) -> NavigationDecision {
    if context.best_entity_by_kind(EntityKind::Prompt).is_none() {
        return patrol.decide(context, &NavigationControllerConfig::default());
    }

    NavigationDecision {
        kind: NavigationDecisionKind::Continue,
        actions: vec![SemanticInputAction::Interact],
        detail: "interaction still active, continue interacting".to_string(),
    }
}

pub fn is_stuck(memory: &ControllerMemory, config: &NavigationControllerConfig) -> bool {
    if memory.progress_samples.len() < config.stuck_frame_window {
        return false;
    }

    let window =
        &memory.progress_samples[memory.progress_samples.len() - config.stuck_frame_window..];
    let Some(first) = window
        .first()
        .and_then(|sample| sample.target_distance_millimeters)
    else {
        return false;
    };
    let Some(last) = window
        .last()
        .and_then(|sample| sample.target_distance_millimeters)
    else {
        return false;
    };

    first.abs_diff(last) <= config.stuck_distance_epsilon_millimeters
}

fn entity_distance_millimeters(entity: &winr_perception::TrackedObservationEntity) -> Option<u32> {
    if entity.entity.kind == EntityKind::Prompt {
        return entity
            .entity
            .tags
            .iter()
            .find_map(|tag| tag.strip_prefix("distance_mm:"))
            .and_then(|value| value.parse::<u32>().ok());
    }

    entity
        .entity
        .tags
        .iter()
        .find_map(|tag| tag.strip_prefix("distance_mm:"))
        .and_then(|value| value.parse::<u32>().ok())
}

fn navigation_camera_yaw(world_model: &WorldModel) -> Option<i32> {
    world_model
        .notes
        .iter()
        .find_map(|note| note.strip_prefix("camera_yaw_md:"))
        .and_then(|value| value.parse::<i32>().ok())
}

fn navigation_target_yaw(world_model: &WorldModel) -> Option<i32> {
    world_model
        .notes
        .iter()
        .find_map(|note| note.strip_prefix("target_yaw_md:"))
        .and_then(|value| value.parse::<i32>().ok())
}

impl WorkflowIntentDefinition {
    pub fn into_input_mapping(
        &self,
        capabilities: &AdvancedBackendCapabilities,
    ) -> Option<InputSinkMapping> {
        let action = self.semantic_action.clone()?;
        let sink = preferred_input_sink(capabilities, self.sink_preference.as_ref(), &action)?;
        Some(InputSinkMapping {
            sink,
            action,
            detail: self.description.clone(),
        })
    }
}

impl WorkflowPlan {
    pub fn resolve_input_plan(
        &self,
        capabilities: &AdvancedBackendCapabilities,
    ) -> WorkflowInputPlan {
        WorkflowInputPlan {
            mappings: self
                .intents
                .iter()
                .filter_map(|intent| intent.into_input_mapping(capabilities))
                .collect(),
        }
    }
}

pub fn preferred_input_sink(
    capabilities: &AdvancedBackendCapabilities,
    preference: Option<&InputSinkPreference>,
    action: &SemanticInputAction,
) -> Option<InputSinkKind> {
    let ordered = preference
        .map(|value| value.ordered_sinks.clone())
        .unwrap_or_else(|| default_sink_order(capabilities, action));

    ordered
        .into_iter()
        .find(|sink| sink_supports_action(*sink, capabilities, action))
}

fn default_sink_order(
    capabilities: &AdvancedBackendCapabilities,
    action: &SemanticInputAction,
) -> Vec<InputSinkKind> {
    let mut sinks = Vec::new();

    if is_semantic_preferred_action(action) && capabilities.internal_interaction {
        sinks.push(InputSinkKind::SemanticInternalAction);
    }
    if is_semantic_navigation_action(action) && capabilities.semantic_navigation {
        sinks.push(InputSinkKind::SemanticInternalAction);
    }
    if capabilities.injected_input {
        sinks.push(InputSinkKind::InjectedRawInput);
    }
    if capabilities.message_input {
        sinks.push(InputSinkKind::Win32Message);
    }
    if capabilities.foreground_input {
        sinks.push(InputSinkKind::Win32Foreground);
    }

    sinks
}

fn sink_supports_action(
    sink: InputSinkKind,
    capabilities: &AdvancedBackendCapabilities,
    action: &SemanticInputAction,
) -> bool {
    match sink {
        InputSinkKind::SemanticInternalAction => {
            (is_semantic_navigation_action(action) && capabilities.semantic_navigation)
                || (is_semantic_preferred_action(action) && capabilities.internal_interaction)
        }
        InputSinkKind::InjectedRawInput => capabilities.injected_input,
        InputSinkKind::Win32Message => capabilities.message_input && is_message_safe_action(action),
        InputSinkKind::Win32Foreground => capabilities.foreground_input,
    }
}

fn is_semantic_navigation_action(action: &SemanticInputAction) -> bool {
    matches!(
        action,
        SemanticInputAction::Approach { .. } | SemanticInputAction::WalkTo { .. }
    )
}

fn is_semantic_preferred_action(action: &SemanticInputAction) -> bool {
    matches!(
        action,
        SemanticInputAction::Approach { .. }
            | SemanticInputAction::WalkTo { .. }
            | SemanticInputAction::Interact
            | SemanticInputAction::StopMotion
    )
}

fn is_message_safe_action(action: &SemanticInputAction) -> bool {
    matches!(
        action,
        SemanticInputAction::Jump
            | SemanticInputAction::Interact
            | SemanticInputAction::Hold { .. }
            | SemanticInputAction::StopMotion
    )
}

impl AppPackRegistry {
    pub fn register(&mut self, manifest: AppPackManifest) {
        self.packs.push(manifest);
    }
}

impl WorkflowExecutionTrace {
    pub fn push(&mut self, kind: WorkflowTraceEventKind, detail: impl Into<String>) {
        let next_sequence = self.events.len() as u64 + 1;
        self.events.push(WorkflowTraceEvent {
            sequence: next_sequence,
            kind,
            detail: detail.into(),
        });
    }

    pub fn reasoning(&self) -> Option<AdvancedExecutionReason> {
        let latest = self.events.last()?;
        let basis = self
            .events
            .iter()
            .rev()
            .take(3)
            .map(|event| {
                format!(
                    "{}: {}",
                    workflow_trace_event_kind_name(event.kind),
                    event.detail
                )
            })
            .collect();

        Some(AdvancedExecutionReason {
            summary: latest.detail.clone(),
            basis,
        })
    }
}

fn workflow_trace_event_kind_name(kind: WorkflowTraceEventKind) -> &'static str {
    match kind {
        WorkflowTraceEventKind::ObservationAccepted => "observation_accepted",
        WorkflowTraceEventKind::TaskSelected => "task_selected",
        WorkflowTraceEventKind::IntentIssued => "intent_issued",
        WorkflowTraceEventKind::RecoveryTriggered => "recovery_triggered",
        WorkflowTraceEventKind::PlanBlocked => "plan_blocked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_perception::{
        ObservationMetadata, ObservationSourceData, ObservationSourceKind, ObservationStateVersion,
        TrackedEntityStatus, TrackedObservationEntity, WorldModel,
    };

    struct RobloxPack;

    struct EntityOnlyPlanner;

    fn sample_dsl() -> WorkflowDslDocument {
        WorkflowDslDocument {
            version: WorkflowDslVersion::V1,
            id: "roblox-harvest".to_string(),
            name: "Roblox Harvest".to_string(),
            tasks: vec![
                WorkflowTaskRecipe {
                    id: "search-for-rock".to_string(),
                    kind: WorkflowTaskKind::SearchFor,
                    detectors: vec![DeclarativeDetector::TemplateMatch {
                        detector_id: "rock-template".to_string(),
                        entity_kind: EntityKind::Interactable,
                    }],
                    recovery: vec![WorkflowRecoveryStep::RetryCurrentNode],
                    action_graph: vec![
                        WorkflowNode {
                            id: "detect-rock".to_string(),
                            name: "Detect Rock".to_string(),
                            kind: WorkflowNodeKind::Detect,
                            steps: vec![WorkflowStep::Condition {
                                condition: WorkflowCondition {
                                    entity_kind: EntityKind::Interactable,
                                    operator: WorkflowConditionOperator::Exists,
                                    threshold: None,
                                },
                            }],
                            next: vec!["approach-rock".to_string()],
                            retry: Some(WorkflowRetryPolicy {
                                max_attempts: 3,
                                cooldown_ms: 250,
                            }),
                            cooldown: Some(WorkflowCooldown { cooldown_ms: 50 }),
                        },
                        WorkflowNode {
                            id: "approach-rock".to_string(),
                            name: "Approach Rock".to_string(),
                            kind: WorkflowNodeKind::Act,
                            steps: vec![WorkflowStep::Action {
                                action: SemanticInputAction::Approach {
                                    target: SemanticInputTarget::EntityId {
                                        entity_id: "rock-1".to_string(),
                                    },
                                },
                            }],
                            next: vec!["wait-for-prompt".to_string()],
                            retry: None,
                            cooldown: None,
                        },
                        WorkflowNode {
                            id: "wait-for-prompt".to_string(),
                            name: "Wait For Prompt".to_string(),
                            kind: WorkflowNodeKind::Branch,
                            steps: vec![WorkflowStep::Condition {
                                condition: WorkflowCondition {
                                    entity_kind: EntityKind::Prompt,
                                    operator: WorkflowConditionOperator::Exists,
                                    threshold: None,
                                },
                            }],
                            next: vec!["interact".to_string(), "recover".to_string()],
                            retry: None,
                            cooldown: Some(WorkflowCooldown { cooldown_ms: 200 }),
                        },
                        WorkflowNode {
                            id: "interact".to_string(),
                            name: "Interact".to_string(),
                            kind: WorkflowNodeKind::Act,
                            steps: vec![WorkflowStep::Action {
                                action: SemanticInputAction::Interact,
                            }],
                            next: vec!["resume-patrol".to_string()],
                            retry: None,
                            cooldown: None,
                        },
                        WorkflowNode {
                            id: "recover".to_string(),
                            name: "Recover".to_string(),
                            kind: WorkflowNodeKind::Recover,
                            steps: vec![WorkflowStep::Recovery {
                                step: WorkflowRecoveryStep::RunController {
                                    controller: NavigationControllerKind::NoProgressRecovery,
                                },
                            }],
                            next: vec!["resume-patrol".to_string()],
                            retry: Some(WorkflowRetryPolicy {
                                max_attempts: 2,
                                cooldown_ms: 500,
                            }),
                            cooldown: None,
                        },
                        WorkflowNode {
                            id: "resume-patrol".to_string(),
                            name: "Resume Patrol".to_string(),
                            kind: WorkflowNodeKind::Act,
                            steps: vec![WorkflowStep::Action {
                                action: SemanticInputAction::WalkTo {
                                    target: SemanticInputTarget::RegionId {
                                        region_id: "dirt-patch-1".to_string(),
                                    },
                                },
                            }],
                            next: vec!["complete".to_string()],
                            retry: None,
                            cooldown: None,
                        },
                        WorkflowNode {
                            id: "complete".to_string(),
                            name: "Complete".to_string(),
                            kind: WorkflowNodeKind::Complete,
                            steps: Vec::new(),
                            next: Vec::new(),
                            retry: None,
                            cooldown: None,
                        },
                    ],
                    backend_preference: Some(WorkflowBackendPreference {
                        preferred_backends: vec![
                            AdvancedProfileBackend::Inject,
                            AdvancedProfileBackend::Foreground,
                        ],
                    }),
                },
                WorkflowTaskRecipe {
                    id: "patrol-region".to_string(),
                    kind: WorkflowTaskKind::PatrolRegion,
                    detectors: vec![DeclarativeDetector::MemoryEntity {
                        detector_id: "region-memory".to_string(),
                        entity_kind: EntityKind::Region,
                    }],
                    recovery: vec![WorkflowRecoveryStep::ResumePreviousTask],
                    action_graph: vec![WorkflowNode {
                        id: "patrol".to_string(),
                        name: "Patrol".to_string(),
                        kind: WorkflowNodeKind::Act,
                        steps: vec![WorkflowStep::Action {
                            action: SemanticInputAction::WalkTo {
                                target: SemanticInputTarget::RegionId {
                                    region_id: "dirt-patch-1".to_string(),
                                },
                            },
                        }],
                        next: Vec::new(),
                        retry: None,
                        cooldown: None,
                    }],
                    backend_preference: Some(WorkflowBackendPreference {
                        preferred_backends: vec![AdvancedProfileBackend::Inject],
                    }),
                },
            ],
        }
    }

    impl AppWorkflowPack for RobloxPack {
        fn manifest(&self) -> AppPackManifest {
            AppPackManifest {
                id: "roblox".to_string(),
                name: "Roblox Pack".to_string(),
                target_family: "roblox".to_string(),
                backend_preferences: vec![AdvancedProfileBackend::Inject],
            }
        }

        fn supported_tasks(&self) -> Vec<WorkflowTaskDefinition> {
            vec![WorkflowTaskDefinition {
                id: "approach-rock".to_string(),
                name: "Approach Rock".to_string(),
                kind: WorkflowTaskKind::Approach,
            }]
        }

        fn default_plan(&self, task: WorkflowTaskKind) -> Option<WorkflowPlan> {
            if task != WorkflowTaskKind::Approach {
                return None;
            }

            Some(WorkflowPlan {
                pack_id: "roblox".to_string(),
                task: WorkflowTaskDefinition {
                    id: "approach-rock".to_string(),
                    name: "Approach Rock".to_string(),
                    kind: WorkflowTaskKind::Approach,
                },
                required_detectors: vec![DetectorDescriptor {
                    id: "rock-template".to_string(),
                    name: "Rock Template".to_string(),
                    kind: winr_perception::DetectorKind::TemplateMatch,
                }],
                required_entity_kinds: vec![EntityKind::Interactable],
                intents: vec![WorkflowIntentDefinition {
                    kind: WorkflowIntentKind::ApproachTarget,
                    description: "move toward the detected target".to_string(),
                    semantic_action: Some(SemanticInputAction::Approach {
                        target: SemanticInputTarget::EntityId {
                            entity_id: "rock-1".to_string(),
                        },
                    }),
                    sink_preference: Some(InputSinkPreference {
                        ordered_sinks: vec![
                            InputSinkKind::SemanticInternalAction,
                            InputSinkKind::InjectedRawInput,
                            InputSinkKind::Win32Foreground,
                        ],
                    }),
                }],
            })
        }
    }

    impl WorkflowPlanner for EntityOnlyPlanner {
        fn can_plan(&self, frame: &ObservationFrame) -> bool {
            frame
                .entities
                .iter()
                .any(|entity| entity.kind == EntityKind::Interactable)
        }

        fn plan(&self, frame: &ObservationFrame, task: WorkflowTaskKind) -> Option<WorkflowPlan> {
            if task != WorkflowTaskKind::Approach || !self.can_plan(frame) {
                return None;
            }

            Some(
                RobloxPack
                    .default_plan(task)
                    .expect("approach plan should exist"),
            )
        }
    }

    fn sample_frame(source: ObservationSourceKind) -> ObservationFrame {
        ObservationFrame {
            target: winr_types::AdvancedTargetRef {
                hwnd: Some("0x0000000000001234".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            metadata: ObservationMetadata {
                version: ObservationStateVersion::V1,
                backend: AdvancedProfileBackend::Inject,
                source,
                frame_id: 7,
                timestamp_ms: 1000,
                freshness_ms: 16,
            },
            source_data: match source {
                ObservationSourceKind::DesktopScreenshot => ObservationSourceData::MemoryState {
                    snapshot_id: "desktop-placeholder".to_string(),
                    state_fields: Vec::new(),
                },
                ObservationSourceKind::RenderHookFrame => ObservationSourceData::MemoryState {
                    snapshot_id: "render-placeholder".to_string(),
                    state_fields: Vec::new(),
                },
                ObservationSourceKind::MemoryState => ObservationSourceData::MemoryState {
                    snapshot_id: "memory-placeholder".to_string(),
                    state_fields: Vec::new(),
                },
                ObservationSourceKind::DetectorOverlay => ObservationSourceData::MemoryState {
                    snapshot_id: "overlay-placeholder".to_string(),
                    state_fields: Vec::new(),
                },
            },
            render_details: None,
            memory_details: None,
            camera_hints: None,
            player_state_hints: None,
            confidence: None,
            detectors: Vec::new(),
            detector_overlays: Vec::new(),
            entities: vec![winr_perception::ObservationEntity {
                id: "rock-1".to_string(),
                kind: EntityKind::Interactable,
                label: "Rock".to_string(),
                confidence: 0.9,
                tags: Vec::new(),
            }],
            notes: Vec::new(),
        }
    }

    #[test]
    fn registry_can_hold_generic_pack_manifest() {
        let mut registry = AppPackRegistry::default();
        registry.register(RobloxPack.manifest());
        assert_eq!(registry.packs.len(), 1);
        assert_eq!(registry.packs[0].id, "roblox");
    }

    #[test]
    fn workflow_trace_orders_events() {
        let mut trace = WorkflowExecutionTrace::default();
        trace.push(
            WorkflowTraceEventKind::ObservationAccepted,
            "accepted a render-backed observation",
        );
        trace.push(
            WorkflowTraceEventKind::IntentIssued,
            "issued approach intent",
        );

        assert_eq!(trace.events.len(), 2);
        assert_eq!(trace.events[0].sequence, 1);
        assert_eq!(trace.events[1].sequence, 2);
    }

    #[test]
    fn planner_stays_source_agnostic_for_equivalent_frames() {
        let planner = EntityOnlyPlanner;
        let desktop_frame = sample_frame(ObservationSourceKind::DesktopScreenshot);
        let render_frame = sample_frame(ObservationSourceKind::RenderHookFrame);

        let desktop_plan = planner
            .plan(&desktop_frame, WorkflowTaskKind::Approach)
            .expect("desktop frame should plan");
        let render_plan = planner
            .plan(&render_frame, WorkflowTaskKind::Approach)
            .expect("render frame should plan");

        assert_eq!(desktop_plan.task.id, render_plan.task.id);
        assert_eq!(
            desktop_plan.required_entity_kinds,
            render_plan.required_entity_kinds
        );
    }

    #[test]
    fn workflow_plan_prefers_semantic_actions_when_available() {
        let plan = RobloxPack
            .default_plan(WorkflowTaskKind::Approach)
            .expect("approach plan should exist");
        let input_plan = plan.resolve_input_plan(&AdvancedBackendCapabilities {
            injected_input: true,
            semantic_navigation: true,
            internal_interaction: true,
            foreground_input: true,
            ..Default::default()
        });

        assert_eq!(input_plan.mappings.len(), 1);
        assert_eq!(
            input_plan.mappings[0].sink,
            InputSinkKind::SemanticInternalAction
        );
        assert!(matches!(
            input_plan.mappings[0].action,
            SemanticInputAction::Approach { .. }
        ));
    }

    #[test]
    fn workflow_plan_falls_back_to_injected_then_foreground() {
        let intent = WorkflowIntentDefinition {
            kind: WorkflowIntentKind::MoveForward,
            description: "move forward briefly".to_string(),
            semantic_action: Some(SemanticInputAction::MoveForward { duration_ms: 250 }),
            sink_preference: None,
        };

        let injected = intent
            .into_input_mapping(&AdvancedBackendCapabilities {
                injected_input: true,
                ..Default::default()
            })
            .expect("injected fallback should resolve");
        assert_eq!(injected.sink, InputSinkKind::InjectedRawInput);

        let foreground = intent
            .into_input_mapping(&AdvancedBackendCapabilities {
                foreground_input: true,
                ..Default::default()
            })
            .expect("foreground fallback should resolve");
        assert_eq!(foreground.sink, InputSinkKind::Win32Foreground);
    }

    #[test]
    fn workflow_can_pick_best_entity_from_world_model() {
        let world_model = WorldModel {
            target: winr_types::AdvancedTargetRef {
                hwnd: Some("0x0000000000001234".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            last_updated_frame_id: 9,
            detector_kinds: vec![winr_perception::DetectorKind::MemoryEntity],
            entities: vec![
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "rock-1".to_string(),
                        kind: EntityKind::Interactable,
                        label: "Rock".to_string(),
                        confidence: 0.8,
                        tags: vec!["resource".to_string()],
                    },
                    smoothed_confidence: 0.82,
                    priority_score: 192,
                    first_seen_frame_id: 1,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "rock-2".to_string(),
                        kind: EntityKind::Interactable,
                        label: "Rock".to_string(),
                        confidence: 0.6,
                        tags: Vec::new(),
                    },
                    smoothed_confidence: 0.61,
                    priority_score: 161,
                    first_seen_frame_id: 2,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
            ],
            notes: Vec::new(),
        };

        let best = select_prioritized_entity_id(&world_model, EntityKind::Interactable)
            .expect("best entity should exist");

        assert_eq!(best, "rock-1");
    }

    fn sample_world_model() -> WorldModel {
        WorldModel {
            target: winr_types::AdvancedTargetRef {
                hwnd: Some("0x0000000000001234".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            last_updated_frame_id: 9,
            detector_kinds: vec![winr_perception::DetectorKind::MemoryEntity],
            entities: vec![
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "rock-1".to_string(),
                        kind: EntityKind::Interactable,
                        label: "Rock".to_string(),
                        confidence: 0.8,
                        tags: vec!["resource".to_string(), "distance_mm:1200".to_string()],
                    },
                    smoothed_confidence: 0.82,
                    priority_score: 192,
                    first_seen_frame_id: 1,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "dirt-patch-1".to_string(),
                        kind: EntityKind::Region,
                        label: "Dirt Patch".to_string(),
                        confidence: 0.75,
                        tags: vec!["patrol".to_string(), "distance_mm:600".to_string()],
                    },
                    smoothed_confidence: 0.8,
                    priority_score: 158,
                    first_seen_frame_id: 1,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "waypoint-1".to_string(),
                        kind: EntityKind::Waypoint,
                        label: "Waypoint".to_string(),
                        confidence: 0.7,
                        tags: vec!["distance_mm:500".to_string()],
                    },
                    smoothed_confidence: 0.72,
                    priority_score: 132,
                    first_seen_frame_id: 1,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
                TrackedObservationEntity {
                    entity: winr_perception::ObservationEntity {
                        id: "prompt-1".to_string(),
                        kind: EntityKind::Prompt,
                        label: "Press E".to_string(),
                        confidence: 0.9,
                        tags: vec!["priority".to_string(), "distance_mm:700".to_string()],
                    },
                    smoothed_confidence: 0.91,
                    priority_score: 221,
                    first_seen_frame_id: 1,
                    last_seen_frame_id: 9,
                    missed_frames: 0,
                    status: TrackedEntityStatus::Active,
                },
            ],
            notes: vec![
                "camera_yaw_md:10000".to_string(),
                "target_yaw_md:30000".to_string(),
            ],
        }
    }

    #[test]
    fn approach_controller_arrives_and_stops_when_close() {
        let mut world_model = sample_world_model();
        if let Some(rock) = world_model
            .entities
            .iter_mut()
            .find(|entity| entity.entity.id == "rock-1")
        {
            rock.entity.tags = vec!["distance_mm:600".to_string()];
        }
        let controller = ApproachUntilThresholdController {
            target_entity_id: "rock-1".to_string(),
        };
        let context = NavigationContext {
            world_model,
            frame_id: 10,
            controller_memory: ControllerMemory::default(),
        };

        let decision = controller.decide(&context, &NavigationControllerConfig::default());

        assert_eq!(decision.kind, NavigationDecisionKind::Arrived);
        assert_eq!(decision.actions, vec![SemanticInputAction::StopMotion]);
    }

    #[test]
    fn rotate_controller_requests_heading_correction() {
        let controller = RotateTowardTargetController;
        let context = NavigationContext {
            world_model: sample_world_model(),
            frame_id: 10,
            controller_memory: ControllerMemory::default(),
        };

        let decision = controller.decide(&context, &NavigationControllerConfig::default());

        assert_eq!(decision.kind, NavigationDecisionKind::Continue);
        assert!(matches!(
            decision.actions[0],
            SemanticInputAction::Turn { .. }
        ));
    }

    #[test]
    fn bounded_patrol_controller_walks_region_or_waypoint() {
        let controller = BoundedRegionPatrolController {
            region_entity_id: "dirt-patch-1".to_string(),
            waypoint_entity_ids: vec!["waypoint-1".to_string()],
        };
        let context = NavigationContext {
            world_model: sample_world_model(),
            frame_id: 10,
            controller_memory: ControllerMemory::default(),
        };

        let decision = controller.decide(&context, &NavigationControllerConfig::default());

        assert_eq!(decision.kind, NavigationDecisionKind::Continue);
        assert!(matches!(
            decision.actions[0],
            SemanticInputAction::WalkTo { .. }
        ));
    }

    #[test]
    fn no_progress_recovery_detects_stuck_and_recovers() {
        let controller = NoProgressRecoveryController;
        let context = NavigationContext {
            world_model: sample_world_model(),
            frame_id: 10,
            controller_memory: ControllerMemory {
                progress_samples: vec![
                    ProgressSample {
                        frame_id: 7,
                        player_position_millimeters: Some([0, 0, 0]),
                        target_distance_millimeters: Some(1200),
                    },
                    ProgressSample {
                        frame_id: 8,
                        player_position_millimeters: Some([10, 0, 0]),
                        target_distance_millimeters: Some(1180),
                    },
                    ProgressSample {
                        frame_id: 9,
                        player_position_millimeters: Some([12, 0, 0]),
                        target_distance_millimeters: Some(1175),
                    },
                ],
            },
        };
        let config = NavigationControllerConfig {
            stuck_distance_epsilon_millimeters: 40,
            ..Default::default()
        };

        let decision = controller.decide(&context, &config);

        assert_eq!(decision.kind, NavigationDecisionKind::Recovering);
        assert_eq!(decision.actions.len(), 3);
    }

    #[test]
    fn prompt_and_patrol_workflow_helpers_switch_modes() {
        let patrol = BoundedRegionPatrolController {
            region_entity_id: "dirt-patch-1".to_string(),
            waypoint_entity_ids: vec!["waypoint-1".to_string()],
        };
        let context = NavigationContext {
            world_model: sample_world_model(),
            frame_id: 10,
            controller_memory: ControllerMemory::default(),
        };

        let scan_decision = patrol_while_scanning_decision(&context, &patrol);
        let interact_decision = interact_when_prompt_appears_decision(&context);

        assert!(matches!(
            scan_decision.actions[0],
            SemanticInputAction::Approach { .. }
        ));
        assert_eq!(
            interact_decision.actions,
            vec![SemanticInputAction::Interact]
        );
    }

    #[test]
    fn dsl_compiles_declarative_recipe_into_plan() {
        let dsl = sample_dsl();
        let recipe = dsl
            .task(WorkflowTaskKind::SearchFor)
            .expect("search task should exist");
        let plan = recipe.compile_plan();

        assert_eq!(plan.required_detectors.len(), 1);
        assert_eq!(plan.required_entity_kinds, vec![EntityKind::Interactable]);
        assert!(plan.intents.iter().any(|intent| matches!(
            intent.semantic_action,
            Some(SemanticInputAction::Approach { .. })
        )));
        assert!(
            plan.intents.iter().any(|intent| matches!(
                intent.semantic_action,
                Some(SemanticInputAction::Interact)
            ))
        );
    }

    #[test]
    fn dsl_supports_conditions_branching_retries_and_cooldowns() {
        let dsl = sample_dsl();
        let recipe = dsl
            .task(WorkflowTaskKind::SearchFor)
            .expect("search task should exist");
        let detect = recipe
            .action_graph
            .iter()
            .find(|node| node.id == "detect-rock")
            .expect("detect node should exist");
        let wait = recipe
            .action_graph
            .iter()
            .find(|node| node.id == "wait-for-prompt")
            .expect("wait node should exist");

        assert!(matches!(detect.kind, WorkflowNodeKind::Detect));
        assert_eq!(
            detect
                .retry
                .as_ref()
                .expect("retry should exist")
                .max_attempts,
            3
        );
        assert_eq!(wait.next.len(), 2);
        assert_eq!(
            wait.cooldown
                .as_ref()
                .expect("cooldown should exist")
                .cooldown_ms,
            200
        );
    }

    #[test]
    fn dsl_evaluates_conditions_and_recovery_steps() {
        let dsl = sample_dsl();
        let recipe = dsl
            .task(WorkflowTaskKind::SearchFor)
            .expect("search task should exist");
        let world_model = sample_world_model();

        assert!(recipe.evaluate_conditions(&world_model));
        assert!(matches!(
            recipe.recovery[0],
            WorkflowRecoveryStep::RetryCurrentNode
        ));
        assert_eq!(
            recipe
                .backend_preference
                .as_ref()
                .expect("backend preference should exist")
                .preferred_backends[0],
            AdvancedProfileBackend::Inject
        );
    }

    #[test]
    fn dsl_supports_task_concepts_and_behavior_graphs() {
        let dsl = sample_dsl();
        let search = dsl
            .task(WorkflowTaskKind::SearchFor)
            .expect("search task should exist");
        let patrol = dsl
            .task(WorkflowTaskKind::PatrolRegion)
            .expect("patrol task should exist");

        let next = next_nodes(search, "wait-for-prompt");
        assert_eq!(next.len(), 2);
        assert_eq!(patrol.kind, WorkflowTaskKind::PatrolRegion);
        assert!(
            patrol
                .action_graph
                .iter()
                .any(|node| node.steps.iter().any(|step| matches!(
                    step,
                    WorkflowStep::Action {
                        action: SemanticInputAction::WalkTo { .. }
                    }
                )))
        );
    }

    #[test]
    fn roblox_pack_loads_as_generic_specialization() {
        let pack_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/roblox");
        let pack = load_app_pack_from_dir(&pack_dir).expect("roblox pack should load");

        assert_eq!(pack.manifest.id, "roblox");
        assert_eq!(pack.manifest.target_family, "roblox");
        assert_eq!(
            pack.manifest.backend_preferences,
            vec![AdvancedProfileBackend::Inject]
        );

        assert!(pack.detector("resource-rock-template").is_some());
        assert!(pack.detector("dirt-region-memory").is_some());
        assert!(pack.detector("prompt-ocr").is_some());

        assert_eq!(pack.movement_tuning.turn_step_milli_degrees, 10000);
        assert_eq!(pack.movement_tuning.arrival_threshold_millimeters, 850);
        assert_eq!(pack.movement_tuning.move_step_ms, 140);
        assert_eq!(pack.movement_tuning.patrol_region_radius_millimeters, 2200);
        assert_eq!(pack.movement_tuning.stuck_frame_window, 3);

        assert!(pack.profile_preset("resource-harvest").is_some());
        assert!(pack.profile_preset("region-patrol").is_some());

        let harvest = pack
            .task_recipe(WorkflowTaskKind::Approach)
            .expect("harvest recipe should exist");
        let patrol = pack
            .task_recipe(WorkflowTaskKind::PatrolRegion)
            .expect("patrol recipe should exist");
        let prompt = pack
            .task_recipe(WorkflowTaskKind::WaitForPrompt)
            .expect("prompt recipe should exist");

        let harvest_plan = harvest.compile_plan();
        let patrol_plan = patrol.compile_plan();
        let prompt_plan = prompt.compile_plan();

        assert_eq!(harvest_plan.task.kind, WorkflowTaskKind::Approach);
        assert_eq!(patrol_plan.task.kind, WorkflowTaskKind::PatrolRegion);
        assert_eq!(prompt_plan.task.kind, WorkflowTaskKind::WaitForPrompt);
        assert!(
            harvest_plan
                .required_entity_kinds
                .contains(&EntityKind::Interactable)
        );
        assert!(
            patrol_plan
                .required_entity_kinds
                .contains(&EntityKind::Region)
        );
        assert!(
            prompt_plan
                .required_entity_kinds
                .contains(&EntityKind::Prompt)
        );
        assert!(harvest_plan.intents.iter().any(|intent| matches!(
            intent.semantic_action,
            Some(SemanticInputAction::Approach { .. })
        )));
        assert!(patrol_plan.intents.iter().any(|intent| matches!(
            intent.semantic_action,
            Some(SemanticInputAction::WalkTo { .. })
        )));
        assert!(
            prompt_plan.intents.iter().any(|intent| matches!(
                intent.semantic_action,
                Some(SemanticInputAction::Interact)
            ))
        );
    }

    #[test]
    fn workflow_trace_produces_operator_facing_reasoning() {
        let mut trace = WorkflowExecutionTrace::default();
        trace.push(
            WorkflowTraceEventKind::ObservationAccepted,
            "accepted fresh prompt observation",
        );
        trace.push(
            WorkflowTraceEventKind::TaskSelected,
            "selected wait_for_prompt task",
        );
        trace.push(
            WorkflowTraceEventKind::IntentIssued,
            "issued interact intent because prompt remained visible",
        );

        let reasoning = trace.reasoning().expect("reasoning should exist");

        assert_eq!(
            reasoning.summary,
            "issued interact intent because prompt remained visible"
        );
        assert_eq!(reasoning.basis.len(), 3);
        assert!(reasoning.basis[0].contains("intent_issued"));
    }
}
