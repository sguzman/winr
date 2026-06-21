use winr_types::{AdvancedBackendCapabilities, AdvancedTargetRef};
use winr_workflows::{
    InputSinkKind, InputSinkMapping, InputSinkPreference, SemanticInputAction, SemanticInputTarget,
    preferred_input_sink,
};

pub trait LayeredInputBackend {
    fn capabilities(&self) -> AdvancedBackendCapabilities;
    fn supported_sinks(&self) -> Vec<InputSinkKind>;
    fn map_action(
        &self,
        action: &SemanticInputAction,
        preference: Option<&InputSinkPreference>,
    ) -> Option<InputSinkMapping>;
    fn describe_mapping(&self, mapping: &InputSinkMapping) -> String;
}

#[derive(Debug, Clone)]
pub struct StubLayeredInputBackend {
    pub target: AdvancedTargetRef,
    pub capabilities: AdvancedBackendCapabilities,
}

impl StubLayeredInputBackend {
    pub fn new(target: AdvancedTargetRef) -> Self {
        Self {
            target,
            capabilities: AdvancedBackendCapabilities {
                foreground_input: true,
                message_input: true,
                injected_input: true,
                semantic_navigation: true,
                internal_interaction: true,
                ..Default::default()
            },
        }
    }

    fn detail_for_action(&self, sink: InputSinkKind, action: &SemanticInputAction) -> String {
        match (sink, action) {
            (InputSinkKind::SemanticInternalAction, SemanticInputAction::Approach { target }) => {
                format!(
                    "semantic approach via internal controller to {}",
                    describe_target(target)
                )
            }
            (InputSinkKind::SemanticInternalAction, SemanticInputAction::WalkTo { target }) => {
                format!(
                    "semantic walk_to via internal controller to {}",
                    describe_target(target)
                )
            }
            (InputSinkKind::SemanticInternalAction, _) => {
                "semantic internal action controller".to_string()
            }
            (InputSinkKind::InjectedRawInput, _) => {
                "map semantic intent to injected raw input shim".to_string()
            }
            (InputSinkKind::Win32Message, _) => {
                "map semantic intent to classic Win32 message sequence".to_string()
            }
            (InputSinkKind::Win32Foreground, _) => {
                "map semantic intent to foreground Win32 input".to_string()
            }
        }
    }
}

impl LayeredInputBackend for StubLayeredInputBackend {
    fn capabilities(&self) -> AdvancedBackendCapabilities {
        self.capabilities.clone()
    }

    fn supported_sinks(&self) -> Vec<InputSinkKind> {
        let mut sinks = Vec::new();
        if self.capabilities.internal_interaction || self.capabilities.semantic_navigation {
            sinks.push(InputSinkKind::SemanticInternalAction);
        }
        if self.capabilities.injected_input {
            sinks.push(InputSinkKind::InjectedRawInput);
        }
        if self.capabilities.message_input {
            sinks.push(InputSinkKind::Win32Message);
        }
        if self.capabilities.foreground_input {
            sinks.push(InputSinkKind::Win32Foreground);
        }
        sinks
    }

    fn map_action(
        &self,
        action: &SemanticInputAction,
        preference: Option<&InputSinkPreference>,
    ) -> Option<InputSinkMapping> {
        let sink = preferred_input_sink(&self.capabilities, preference, action)?;
        Some(InputSinkMapping {
            sink,
            action: action.clone(),
            detail: self.detail_for_action(sink, action),
        })
    }

    fn describe_mapping(&self, mapping: &InputSinkMapping) -> String {
        format!(
            "target={} sink={:?} detail={}",
            self.target
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            mapping.sink,
            mapping.detail
        )
    }
}

fn describe_target(target: &SemanticInputTarget) -> String {
    match target {
        SemanticInputTarget::CurrentTarget => "current_target".to_string(),
        SemanticInputTarget::EntityId { entity_id } => format!("entity:{entity_id}"),
        SemanticInputTarget::RegionId { region_id } => format!("region:{region_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> AdvancedTargetRef {
        AdvancedTargetRef {
            hwnd: Some("0x0000000000001234".to_string()),
            pid: Some(42),
            exe: Some("RobloxPlayerBeta.exe".to_string()),
            window_class: Some("WINDOWSCLIENT".to_string()),
            title_hint: Some("Roblox".to_string()),
        }
    }

    #[test]
    fn layered_backend_prefers_semantic_internal_actions() {
        let backend = StubLayeredInputBackend::new(sample_target());
        let mapping = backend
            .map_action(
                &SemanticInputAction::Approach {
                    target: SemanticInputTarget::EntityId {
                        entity_id: "rock-1".to_string(),
                    },
                },
                None,
            )
            .expect("semantic mapping should resolve");

        assert_eq!(mapping.sink, InputSinkKind::SemanticInternalAction);
        assert!(mapping.detail.contains("semantic approach"));
    }

    #[test]
    fn layered_backend_can_fall_back_to_injected_input() {
        let mut backend = StubLayeredInputBackend::new(sample_target());
        backend.capabilities.internal_interaction = false;
        backend.capabilities.semantic_navigation = false;
        backend.capabilities.message_input = false;
        backend.capabilities.foreground_input = false;

        let mapping = backend
            .map_action(&SemanticInputAction::MoveForward { duration_ms: 150 }, None)
            .expect("injected fallback should resolve");

        assert_eq!(mapping.sink, InputSinkKind::InjectedRawInput);
    }

    #[test]
    fn layered_backend_supports_preferred_message_mapping_for_simple_actions() {
        let mut backend = StubLayeredInputBackend::new(sample_target());
        backend.capabilities.internal_interaction = false;
        backend.capabilities.semantic_navigation = false;
        backend.capabilities.injected_input = false;

        let mapping = backend
            .map_action(
                &SemanticInputAction::Interact,
                Some(&InputSinkPreference {
                    ordered_sinks: vec![
                        InputSinkKind::Win32Message,
                        InputSinkKind::Win32Foreground,
                    ],
                }),
            )
            .expect("message mapping should resolve");

        assert_eq!(mapping.sink, InputSinkKind::Win32Message);
    }
}
