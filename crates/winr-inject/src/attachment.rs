use winr_types::{
    AdvancedAttachment, AdvancedAttachmentEvent, AdvancedAttachmentHealth,
    AdvancedAttachmentHealthStatus, AdvancedAttachmentPolicy, AdvancedReattachMode,
    WindowSelector, WinrResult,
};

use crate::{discover_attachable_targets, resolve_attachable_target};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSupervisor {
    pub attachment: AdvancedAttachment,
}

impl AttachmentSupervisor {
    pub fn attach(
        selector: &WindowSelector,
        policy: AdvancedAttachmentPolicy,
    ) -> WinrResult<(Self, AdvancedAttachmentEvent)> {
        let discovery = discover_attachable_targets(selector)?;
        let target = resolve_attachable_target(&discovery)?;
        let attachment = AdvancedAttachment {
            selector: selector.clone(),
            policy,
            target: target.clone(),
            health: AdvancedAttachmentHealth {
                status: AdvancedAttachmentHealthStatus::Healthy,
                heartbeat_failures: 0,
                last_error: None,
            },
        };

        Ok((
            Self { attachment },
            AdvancedAttachmentEvent::Attached { target },
        ))
    }

    pub fn heartbeat(&mut self) -> AdvancedAttachmentEvent {
        let discovery = match discover_attachable_targets(&self.attachment.selector) {
            Ok(discovery) => discovery,
            Err(error) => {
                self.register_heartbeat_failure(error.to_string());
                return AdvancedAttachmentEvent::HeartbeatFailed {
                    detail: self
                        .attachment
                        .health
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "unknown heartbeat error".to_string()),
                    failures: self.attachment.health.heartbeat_failures,
                };
            }
        };

        match resolve_attachable_target(&discovery) {
            Ok(target) => {
                let current_pid = self.attachment.target.target.pid;
                let next_pid = target.target.pid;

                if current_pid == next_pid {
                    self.attachment.target = target.clone();
                    self.attachment.health = AdvancedAttachmentHealth {
                        status: AdvancedAttachmentHealthStatus::Healthy,
                        heartbeat_failures: 0,
                        last_error: None,
                    };
                    return AdvancedAttachmentEvent::HeartbeatHealthy { target };
                }

                if self.attachment.policy.reattach_mode == AdvancedReattachMode::IfProcessRestarted
                {
                    self.attachment.target = target.clone();
                    self.attachment.health = AdvancedAttachmentHealth {
                        status: AdvancedAttachmentHealthStatus::Healthy,
                        heartbeat_failures: 0,
                        last_error: None,
                    };
                    return AdvancedAttachmentEvent::Reattached {
                        previous_pid: current_pid,
                        target,
                    };
                }

                self.register_heartbeat_failure(
                    "target process changed and reattach is disabled".to_string(),
                );
                AdvancedAttachmentEvent::HeartbeatFailed {
                    detail: self
                        .attachment
                        .health
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "unknown heartbeat error".to_string()),
                    failures: self.attachment.health.heartbeat_failures,
                }
            }
            Err(error) => {
                self.register_heartbeat_failure(error.to_string());
                AdvancedAttachmentEvent::HeartbeatFailed {
                    detail: self
                        .attachment
                        .health
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "unknown heartbeat error".to_string()),
                    failures: self.attachment.health.heartbeat_failures,
                }
            }
        }
    }

    pub fn detach(&mut self, detail: impl Into<String>) -> AdvancedAttachmentEvent {
        self.attachment.health = AdvancedAttachmentHealth {
            status: AdvancedAttachmentHealthStatus::Lost,
            heartbeat_failures: self.attachment.health.heartbeat_failures,
            last_error: Some(detail.into()),
        };

        AdvancedAttachmentEvent::Detached {
            detail: self
                .attachment
                .health
                .last_error
                .clone()
                .unwrap_or_else(|| "detached".to_string()),
        }
    }

    fn register_heartbeat_failure(&mut self, detail: String) {
        let failures = self.attachment.health.heartbeat_failures + 1;
        let status = if failures >= self.attachment.policy.heartbeat_failure_threshold {
            AdvancedAttachmentHealthStatus::Lost
        } else {
            AdvancedAttachmentHealthStatus::Stale
        };

        self.attachment.health = AdvancedAttachmentHealth {
            status,
            heartbeat_failures: failures,
            last_error: Some(detail),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winr_types::{
        AdvancedAttachableTarget,
        AdvancedBackendLifecycleState, AdvancedIntegrityLevel, AdvancedProcessArchitecture,
        AdvancedProcessMetadata, AdvancedTargetRef,
    };

    fn sample_target(pid: u32) -> AdvancedAttachableTarget {
        AdvancedAttachableTarget {
            target: AdvancedTargetRef {
                hwnd: Some("0x0000000000001111".to_string()),
                pid: Some(pid),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some("Roblox".to_string()),
            },
            lifecycle_state: AdvancedBackendLifecycleState::Attachable,
            title: "Roblox".to_string(),
            class_name: "WINDOWSCLIENT".to_string(),
            exe: Some("RobloxPlayerBeta.exe".to_string()),
            visible: true,
            minimized: false,
            foreground: false,
            process: AdvancedProcessMetadata {
                architecture: AdvancedProcessArchitecture::X64,
                integrity_level: AdvancedIntegrityLevel::Medium,
                loaded_modules: vec!["RobloxPlayerBeta.exe".to_string()],
                executable_path: Some("C:\\RobloxPlayerBeta.exe".to_string()),
                likely_rendering_window: Some("0x0000000000001111".to_string()),
            },
            notes: Vec::new(),
        }
    }

    #[test]
    fn detach_marks_attachment_lost() {
        let mut supervisor = AttachmentSupervisor {
            attachment: AdvancedAttachment {
                selector: WindowSelector::default(),
                policy: AdvancedAttachmentPolicy::default(),
                target: sample_target(42),
                health: AdvancedAttachmentHealth {
                    status: AdvancedAttachmentHealthStatus::Healthy,
                    heartbeat_failures: 0,
                    last_error: None,
                },
            },
        };

        let event = supervisor.detach("manual shutdown");
        assert!(matches!(event, AdvancedAttachmentEvent::Detached { .. }));
        assert_eq!(
            supervisor.attachment.health.status,
            AdvancedAttachmentHealthStatus::Lost
        );
    }

    #[test]
    fn heartbeat_failure_threshold_marks_lost() {
        let mut supervisor = AttachmentSupervisor {
            attachment: AdvancedAttachment {
                selector: WindowSelector::default(),
                policy: AdvancedAttachmentPolicy {
                    reattach_mode: AdvancedReattachMode::Never,
                    heartbeat_failure_threshold: 2,
                },
                target: sample_target(42),
                health: AdvancedAttachmentHealth {
                    status: AdvancedAttachmentHealthStatus::Healthy,
                    heartbeat_failures: 0,
                    last_error: None,
                },
            },
        };

        supervisor.register_heartbeat_failure("first".to_string());
        assert_eq!(
            supervisor.attachment.health.status,
            AdvancedAttachmentHealthStatus::Stale
        );

        supervisor.register_heartbeat_failure("second".to_string());
        assert_eq!(
            supervisor.attachment.health.status,
            AdvancedAttachmentHealthStatus::Lost
        );
    }
}
