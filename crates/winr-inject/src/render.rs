use winr_perception::{
    DebugOverlayCommand, DetectorOverlay, ObservationCaptureContext, ObservationFrame,
    ObservationFrameHandle, ObservationPixelFormat, ObservationSourceData, OverlayKind,
    RenderDebugOverlaySurface, RenderFrameAnalyzer, RenderFrameAvailability, RenderFrameTiming,
    RenderHookBoundary, RenderObservationDetails, RenderSampleRegion, RenderSceneUseCase,
};
use winr_types::{
    AdvancedBackendCapabilities, AdvancedBinaryPayloadRef, AdvancedIpcTransportKind,
    AdvancedPayloadEncoding, AdvancedTargetRef, WinrResult,
};

pub trait RenderObservationBackend {
    fn hook_boundary(&self) -> RenderHookBoundary;
    fn capabilities(&self) -> AdvancedBackendCapabilities;
    fn frame_availability(&self) -> RenderFrameAvailability;
    fn capture_render_frame(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<ObservationFrame>;
    fn sample_regions(&self, frame: &ObservationFrame) -> Vec<RenderSampleRegion>;
    fn analyze_frame(
        &self,
        frame: &ObservationFrame,
        analyzer: &dyn RenderFrameAnalyzer,
    ) -> Vec<DetectorOverlay>;
    fn debug_overlay_surface(&self, frame: &ObservationFrame) -> RenderDebugOverlaySurface;
}

#[derive(Debug, Clone)]
pub struct StubRenderObserver {
    target: AdvancedTargetRef,
    boundary: RenderHookBoundary,
    present_count: u64,
}

impl StubRenderObserver {
    pub fn new(target: AdvancedTargetRef, boundary: RenderHookBoundary) -> Self {
        Self {
            target,
            boundary,
            present_count: 0,
        }
    }

    fn payload(&self, suffix: &str, byte_len: u64, description: &str) -> AdvancedBinaryPayloadRef {
        AdvancedBinaryPayloadRef {
            payload_id: format!(
                "{}-{}-{}",
                self.target
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                self.present_count,
                suffix
            ),
            encoding: AdvancedPayloadEncoding::RawBytes,
            byte_len,
            transport: AdvancedIpcTransportKind::SharedMemory,
            description: description.to_string(),
        }
    }
}

impl RenderObservationBackend for StubRenderObserver {
    fn hook_boundary(&self) -> RenderHookBoundary {
        self.boundary
    }

    fn capabilities(&self) -> AdvancedBackendCapabilities {
        AdvancedBackendCapabilities {
            render_observation: true,
            entity_tracking: true,
            ..Default::default()
        }
    }

    fn frame_availability(&self) -> RenderFrameAvailability {
        RenderFrameAvailability {
            frame_ready: true,
            present_count: self.present_count,
            dropped_since_last_capture: 0,
        }
    }

    fn capture_render_frame(
        &mut self,
        context: &ObservationCaptureContext,
    ) -> WinrResult<ObservationFrame> {
        self.present_count += 1;
        let sample_regions = vec![RenderSampleRegion {
            id: "center-crop".to_string(),
            left: 760,
            top: 420,
            width: 400,
            height: 240,
            payload: Some(self.payload("sample", 400 * 240 * 4, "center sample region")),
        }];
        let debug_overlay = RenderDebugOverlaySurface {
            development_only: true,
            commands: vec![DebugOverlayCommand {
                label: "focus region".to_string(),
                kind: OverlayKind::BoundingBoxes,
                left: 760,
                top: 420,
                width: 400,
                height: 240,
            }],
        };
        let frame = ObservationFrame::from_update(
            context.clone(),
            winr_types::AdvancedObservationUpdate {
                frame_id: context.frame_id,
                source: "render-hook".to_string(),
                detail: "captured at render presentation boundary".to_string(),
                payload: Some(self.payload("frame", 1920 * 1080 * 4, "render frame pixels")),
            },
            ObservationSourceData::RenderHookFrame {
                frame: ObservationFrameHandle {
                    payload: self.payload("frame", 1920 * 1080 * 4, "render frame pixels"),
                    width: 1920,
                    height: 1080,
                    pixel_format: ObservationPixelFormat::Bgra8,
                    row_stride_bytes: Some(1920 * 4),
                },
            },
        )
        .with_render_details(RenderObservationDetails {
            boundary: self.boundary,
            timing: RenderFrameTiming {
                present_timestamp_ms: context.timestamp_ms,
                frame_interval_ms: Some(16),
                capture_latency_ms: Some(2),
            },
            availability: self.frame_availability(),
            sample_regions,
            debug_overlay: Some(debug_overlay),
            intended_uses: vec![
                RenderSceneUseCase::VisibleSceneUnderstanding,
                RenderSceneUseCase::TemplateDetection,
                RenderSceneUseCase::ObjectDetection,
                RenderSceneUseCase::ActionCorrelation,
            ],
            does_not_claim_game_state_api: true,
            does_not_claim_background_input_channel: true,
        });

        Ok(frame)
    }

    fn sample_regions(&self, frame: &ObservationFrame) -> Vec<RenderSampleRegion> {
        frame
            .render_details
            .as_ref()
            .map(|details| details.sample_regions.clone())
            .unwrap_or_default()
    }

    fn analyze_frame(
        &self,
        frame: &ObservationFrame,
        analyzer: &dyn RenderFrameAnalyzer,
    ) -> Vec<DetectorOverlay> {
        analyzer.analyze(frame)
    }

    fn debug_overlay_surface(&self, frame: &ObservationFrame) -> RenderDebugOverlaySurface {
        frame
            .render_details
            .as_ref()
            .and_then(|details| details.debug_overlay.clone())
            .unwrap_or(RenderDebugOverlaySurface {
                development_only: true,
                commands: Vec::new(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_types::AdvancedProfileBackend;

    struct StubAnalyzer;

    impl RenderFrameAnalyzer for StubAnalyzer {
        fn name(&self) -> &str {
            "stub-analyzer"
        }

        fn analyze(&self, _frame: &ObservationFrame) -> Vec<DetectorOverlay> {
            vec![DetectorOverlay {
                detector_id: "rock-template".to_string(),
                kind: OverlayKind::BoundingBoxes,
                label: "rock target".to_string(),
                payload: None,
            }]
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
            frame_id: 9,
            timestamp_ms: 1000,
            freshness_ms: 16,
        }
    }

    #[test]
    fn stub_render_observer_captures_timing_and_boundary() {
        let mut observer = StubRenderObserver::new(
            sample_context().target.clone(),
            RenderHookBoundary::DxgiPresent,
        );
        let frame = observer
            .capture_render_frame(&sample_context())
            .expect("render capture should succeed");

        assert_eq!(
            frame.metadata.source,
            winr_perception::ObservationSourceKind::RenderHookFrame
        );
        let details = frame.render_details.expect("render details should exist");
        assert_eq!(details.boundary, RenderHookBoundary::DxgiPresent);
        assert_eq!(details.timing.frame_interval_ms, Some(16));
        assert!(details.availability.frame_ready);
        assert!(details.does_not_claim_game_state_api);
        assert!(details.does_not_claim_background_input_channel);
    }

    #[test]
    fn stub_render_observer_supports_analysis_and_debug_overlay() {
        let mut observer = StubRenderObserver::new(
            sample_context().target.clone(),
            RenderHookBoundary::D3d11Present,
        );
        let frame = observer
            .capture_render_frame(&sample_context())
            .expect("render capture should succeed");

        let overlays = observer.analyze_frame(&frame, &StubAnalyzer);
        let debug_overlay = observer.debug_overlay_surface(&frame);
        let samples = observer.sample_regions(&frame);

        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].label, "rock target");
        assert!(debug_overlay.development_only);
        assert_eq!(debug_overlay.commands[0].kind, OverlayKind::BoundingBoxes);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn stub_render_observer_is_render_only_capability() {
        let observer =
            StubRenderObserver::new(sample_context().target, RenderHookBoundary::D3d12Present);
        let capabilities = observer.capabilities();

        assert!(capabilities.render_observation);
        assert!(capabilities.entity_tracking);
        assert!(!capabilities.injected_input);
        assert!(!capabilities.internal_interaction);
    }
}
