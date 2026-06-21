use winr_perception::{
    CameraHints, EntityKind, MemoryCameraState, MemoryObjectState, MemoryObservationDetails,
    MemoryObservationUseCase, MemoryPlayerState, MemoryPromptState, MemorySchemaVersion,
    MemoryStateProjector, ObservationCaptureContext, ObservationEntity, ObservationFrame,
    ObservationMovementState, ObservationSourceData, ObservationStateField, PlayerStateHints,
};
use winr_types::{AdvancedBackendCapabilities, AdvancedTargetRef, WinrResult};

pub trait MemoryObservationBackend {
    fn capabilities(&self) -> AdvancedBackendCapabilities;
    fn schema_version(&self) -> MemorySchemaVersion;
    fn read_memory_state(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<ObservationFrame>;
    fn snapshot_player_state(&self, frame: &ObservationFrame) -> Option<MemoryPlayerState>;
    fn snapshot_camera_state(&self, frame: &ObservationFrame) -> Option<MemoryCameraState>;
    fn snapshot_nearby_objects(&self, frame: &ObservationFrame) -> Vec<MemoryObjectState>;
    fn project_entities(
        &self,
        frame: &ObservationFrame,
        projector: &dyn MemoryStateProjector,
    ) -> Vec<ObservationEntity>;
}

#[derive(Debug, Clone)]
pub struct StubMemoryObserver {
    target: AdvancedTargetRef,
    snapshot_counter: u64,
}

impl StubMemoryObserver {
    pub fn new(target: AdvancedTargetRef) -> Self {
        Self {
            target,
            snapshot_counter: 0,
        }
    }
}

impl MemoryObservationBackend for StubMemoryObserver {
    fn capabilities(&self) -> AdvancedBackendCapabilities {
        AdvancedBackendCapabilities {
            memory_observation: true,
            entity_tracking: true,
            ..Default::default()
        }
    }

    fn schema_version(&self) -> MemorySchemaVersion {
        MemorySchemaVersion::V1
    }

    fn read_memory_state(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<ObservationFrame> {
        self.snapshot_counter += 1;
        let snapshot_id = format!(
            "pid-{}-snapshot-{}",
            self.target
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            self.snapshot_counter
        );
        let player_state = MemoryPlayerState {
            world_position_millimeters: Some([10000, 0, -4000]),
            velocity_millimeters_per_second: Some([250, 0, 0]),
            movement_state: Some(ObservationMovementState::Walking),
            active_tool: Some("pickaxe".to_string()),
            active_modes: vec!["harvesting".to_string()],
        };
        let camera_state = MemoryCameraState {
            yaw_milli_degrees: Some(90000),
            pitch_milli_degrees: Some(-12000),
            field_of_view_milli_degrees: None,
            mode: Some("third_person".to_string()),
        };
        let nearby_objects = vec![
            MemoryObjectState {
                id: "rock-1".to_string(),
                kind: "resource_node".to_string(),
                label: "Rock".to_string(),
                world_position_millimeters: Some([10800, 0, -3900]),
                distance_millimeters: Some(1200),
                interactable: true,
            },
            MemoryObjectState {
                id: "dirt-patch-1".to_string(),
                kind: "patrol_region".to_string(),
                label: "Dirt Patch".to_string(),
                world_position_millimeters: Some([9600, 0, -4300]),
                distance_millimeters: Some(700),
                interactable: false,
            },
        ];
        let prompts = vec![MemoryPromptState {
            id: "prompt-1".to_string(),
            label: "Press E".to_string(),
            visible: true,
            distance_millimeters: Some(850),
        }];
        let entities = vec![
            ObservationEntity {
                id: "player".to_string(),
                kind: EntityKind::Player,
                label: "Local Player".to_string(),
                confidence: 1.0,
                tags: vec!["memory".to_string()],
            },
            ObservationEntity {
                id: "rock-1".to_string(),
                kind: EntityKind::Interactable,
                label: "Rock".to_string(),
                confidence: 0.98,
                tags: vec!["memory".to_string(), "resource".to_string()],
            },
            ObservationEntity {
                id: "dirt-patch-1".to_string(),
                kind: EntityKind::Region,
                label: "Dirt Patch".to_string(),
                confidence: 0.95,
                tags: vec!["memory".to_string(), "patrol".to_string()],
            },
        ];

        let mut frame = ObservationFrame::from_update(
            context.clone(),
            winr_types::AdvancedObservationUpdate {
                frame_id: context.frame_id,
                source: "memory-reader".to_string(),
                detail: "captured normalized memory snapshot".to_string(),
                payload: None,
            },
            ObservationSourceData::MemoryState {
                snapshot_id: snapshot_id.clone(),
                state_fields: vec![
                    ObservationStateField {
                        key: "player.position_mm".to_string(),
                        value: "[10000,0,-4000]".to_string(),
                    },
                    ObservationStateField {
                        key: "prompt.visible".to_string(),
                        value: "true".to_string(),
                    },
                    ObservationStateField {
                        key: "objects.nearby".to_string(),
                        value: "2".to_string(),
                    },
                ],
            },
        )
        .with_memory_details(MemoryObservationDetails {
            schema_version: self.schema_version(),
            snapshot_id,
            intended_uses: vec![
                MemoryObservationUseCase::PlayerState,
                MemoryObservationUseCase::CameraState,
                MemoryObservationUseCase::PromptState,
                MemoryObservationUseCase::InteractableDiscovery,
                MemoryObservationUseCase::ObjectInventory,
            ],
            player_state: Some(player_state.clone()),
            camera_state: Some(camera_state.clone()),
            prompts,
            nearby_objects: nearby_objects.clone(),
            raw_layout_hidden: true,
        });

        frame.player_state_hints = Some(PlayerStateHints {
            world_position: Some([10.0, 0.0, -4.0]),
            velocity: Some([0.25, 0.0, 0.0]),
            health_percent: Some(1.0),
            movement_state: ObservationMovementState::Walking,
            active_modes: vec!["harvesting".to_string()],
        });
        frame.camera_hints = Some(CameraHints {
            yaw_degrees: Some(90.0),
            pitch_degrees: Some(-12.0),
            field_of_view_degrees: Some(70.0),
            camera_mode: Some("third_person".to_string()),
        });
        frame.entities = entities;
        frame.notes.push(
            "memory observation is normalized and intentionally hides raw offsets".to_string(),
        );

        Ok(frame.with_confidence_summary(0.98))
    }

    fn snapshot_player_state(&self, frame: &ObservationFrame) -> Option<MemoryPlayerState> {
        frame
            .memory_details
            .as_ref()
            .and_then(|details| details.player_state.clone())
    }

    fn snapshot_camera_state(&self, frame: &ObservationFrame) -> Option<MemoryCameraState> {
        frame
            .memory_details
            .as_ref()
            .and_then(|details| details.camera_state.clone())
    }

    fn snapshot_nearby_objects(&self, frame: &ObservationFrame) -> Vec<MemoryObjectState> {
        frame
            .memory_details
            .as_ref()
            .map(|details| details.nearby_objects.clone())
            .unwrap_or_default()
    }

    fn project_entities(
        &self,
        frame: &ObservationFrame,
        projector: &dyn MemoryStateProjector,
    ) -> Vec<ObservationEntity> {
        projector.project_entities(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_types::AdvancedProfileBackend;

    struct StubProjector;

    impl MemoryStateProjector for StubProjector {
        fn name(&self) -> &str {
            "stub-projector"
        }

        fn project_entities(&self, frame: &ObservationFrame) -> Vec<ObservationEntity> {
            frame
                .memory_details
                .as_ref()
                .map(|details| {
                    details
                        .nearby_objects
                        .iter()
                        .map(|object| ObservationEntity {
                            id: object.id.clone(),
                            kind: if object.interactable {
                                EntityKind::Interactable
                            } else {
                                EntityKind::Region
                            },
                            label: object.label.clone(),
                            confidence: 0.9,
                            tags: vec!["projected".to_string()],
                        })
                        .collect()
                })
                .unwrap_or_default()
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
            frame_id: 12,
            timestamp_ms: 1000,
            freshness_ms: 8,
        }
    }

    #[test]
    fn stub_memory_observer_reads_versioned_normalized_state() {
        let mut observer = StubMemoryObserver::new(sample_context().target.clone());
        let frame = observer
            .read_memory_state(&sample_context())
            .expect("memory capture should succeed");

        assert_eq!(
            frame.metadata.source,
            winr_perception::ObservationSourceKind::MemoryState
        );
        let details = frame.memory_details.expect("memory details should exist");
        assert_eq!(details.schema_version, MemorySchemaVersion::V1);
        assert!(details.raw_layout_hidden);
        assert_eq!(details.nearby_objects.len(), 2);
    }

    #[test]
    fn stub_memory_observer_exposes_player_camera_and_objects() {
        let mut observer = StubMemoryObserver::new(sample_context().target.clone());
        let frame = observer
            .read_memory_state(&sample_context())
            .expect("memory capture should succeed");

        let player = observer
            .snapshot_player_state(&frame)
            .expect("player state should exist");
        let camera = observer
            .snapshot_camera_state(&frame)
            .expect("camera state should exist");
        let objects = observer.snapshot_nearby_objects(&frame);

        assert_eq!(player.active_tool.as_deref(), Some("pickaxe"));
        assert_eq!(camera.mode.as_deref(), Some("third_person"));
        assert_eq!(objects[0].kind, "resource_node");
    }

    #[test]
    fn stub_memory_observer_projects_entities_without_raw_layouts() {
        let mut observer = StubMemoryObserver::new(sample_context().target);
        let frame = observer
            .read_memory_state(&sample_context())
            .expect("memory capture should succeed");

        let projected = observer.project_entities(&frame, &StubProjector);

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].label, "Rock");
        assert_eq!(projected[1].kind, EntityKind::Region);
    }
}
