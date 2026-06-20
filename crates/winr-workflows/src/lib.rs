use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use winr_perception::{DetectorDescriptor, EntityKind, ObservationFrame};
use winr_types::AdvancedProfileBackend;

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

pub trait AppWorkflowPack {
    fn manifest(&self) -> AppPackManifest;
    fn supported_tasks(&self) -> Vec<WorkflowTaskDefinition>;
    fn default_plan(&self, task: WorkflowTaskKind) -> Option<WorkflowPlan>;
}

pub trait WorkflowPlanner {
    fn can_plan(&self, frame: &ObservationFrame) -> bool;
    fn plan(&self, frame: &ObservationFrame, task: WorkflowTaskKind) -> Option<WorkflowPlan>;
}

impl AppPackRegistry {
    pub fn register(&mut self, manifest: AppPackManifest) {
        self.packs.push(manifest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RobloxPack;

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
                }],
            })
        }
    }

    #[test]
    fn registry_can_hold_generic_pack_manifest() {
        let mut registry = AppPackRegistry::default();
        registry.register(RobloxPack.manifest());
        assert_eq!(registry.packs.len(), 1);
        assert_eq!(registry.packs[0].id, "roblox");
    }
}
