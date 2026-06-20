use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use winr_types::{AdvancedProfileBackend, AdvancedTargetRef};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationMetadata {
    pub backend: AdvancedProfileBackend,
    pub source: ObservationSourceKind,
    pub frame_id: u64,
    pub timestamp_ms: u64,
    pub freshness_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservationFrame {
    pub target: AdvancedTargetRef,
    pub metadata: ObservationMetadata,
    #[serde(default)]
    pub detectors: Vec<DetectorDescriptor>,
    #[serde(default)]
    pub entities: Vec<ObservationEntity>,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub trait ObservationFrameSource {
    fn source_kind(&self) -> ObservationSourceKind;
    fn describe_detectors(&self) -> Vec<DetectorDescriptor>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_frame_serializes_cleanly() {
        let frame = ObservationFrame {
            target: AdvancedTargetRef {
                hwnd: Some("0x0000000000001234".to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            metadata: ObservationMetadata {
                backend: AdvancedProfileBackend::Inject,
                source: ObservationSourceKind::RenderHookFrame,
                frame_id: 7,
                timestamp_ms: 1000,
                freshness_ms: 16,
            },
            detectors: vec![DetectorDescriptor {
                id: "rock-template".to_string(),
                name: "Rock Template".to_string(),
                kind: DetectorKind::TemplateMatch,
            }],
            entities: vec![ObservationEntity {
                id: "rock-1".to_string(),
                kind: EntityKind::Interactable,
                label: "Rock".to_string(),
                confidence: 0.92,
                tags: vec!["resource".to_string()],
            }],
            notes: vec!["sample".to_string()],
        };

        let json = serde_json::to_string(&frame).expect("frame should serialize");
        assert!(json.contains("\"render_hook_frame\""));
        assert!(json.contains("\"rock-1\""));
    }
}
