use tracing::{debug, instrument};
use winr_types::{
    AdvancedBackendCapabilities, AdvancedBackendHello, AdvancedBackendLifecycleState,
    AdvancedBackendSelection, AdvancedProfileBackend, ProfileConfig, WinrError, WinrResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedBackendSession {
    pub hello: AdvancedBackendHello,
}

#[instrument(skip(profile))]
pub fn prepare_profile_backend(profile: &ProfileConfig) -> WinrResult<AdvancedBackendSession> {
    debug!(
        profile_id = %profile.profile.id,
        backend = profile.execution.backend.as_str(),
        "preparing advanced backend session"
    );

    Err(WinrError::Unsupported {
        message: format!(
            "advanced backend '{}' is not implemented yet for profile '{}'",
            profile.execution.backend.as_str(),
            profile.profile.id
        ),
    })
}

pub fn resolve_backend_selection(profile: &ProfileConfig) -> AdvancedBackendSelection {
    match profile.execution.backend {
        AdvancedProfileBackend::Auto => AdvancedBackendSelection {
            requested: AdvancedProfileBackend::Auto,
            resolved: inferred_backend(profile),
        },
        backend => AdvancedBackendSelection {
            requested: backend,
            resolved: backend,
        },
    }
}

fn inferred_backend(profile: &ProfileConfig) -> AdvancedProfileBackend {
    match profile.action {
        winr_types::ProfileAction::MouseClick {
            input_mode: Some(winr_types::MouseInputMode::Message),
            ..
        } => AdvancedProfileBackend::Message,
        _ => AdvancedProfileBackend::Foreground,
    }
}

pub fn stub_hello(profile: &ProfileConfig) -> AdvancedBackendHello {
    AdvancedBackendHello {
        protocol_version: 1,
        backend: AdvancedProfileBackend::Inject,
        lifecycle_state: AdvancedBackendLifecycleState::Discovered,
        capabilities: AdvancedBackendCapabilities::default(),
        target: winr_types::AdvancedTargetRef {
            hwnd: profile.target.hwnd.clone(),
            pid: profile.target.pid,
            exe: profile.target.exe.clone(),
            window_class: profile.target.class_name.clone(),
            title_hint: profile.target.title_contains.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ProfileConfig {
        toml::from_str(
            r#"
[profile]
id = "inject-demo"
name = "Inject Demo"
description = "Test profile"
version = "1"

[target]
title_contains = "Roblox"
exe = "RobloxPlayerBeta.exe"

[action]
kind = "mouse_click"
button = "left"

[schedule]
mode = "interval"
every_ms = 50

[logging]
level = "info"
mode = "single_line_counter"
update_every_trigger = true
template = "count={count}"

[safety]
require_visible_window = true
require_foreground_window = true
stop_on_focus_loss = true
"#,
        )
        .expect("sample profile should parse")
    }

    #[test]
    fn auto_backend_defaults_to_foreground() {
        let profile = sample_profile();
        let selection = resolve_backend_selection(&profile);
        assert_eq!(selection.requested, AdvancedProfileBackend::Auto);
        assert_eq!(selection.resolved, AdvancedProfileBackend::Foreground);
    }

    #[test]
    fn explicit_inject_backend_returns_stub_error() {
        let mut profile = sample_profile();
        profile.execution.backend = AdvancedProfileBackend::Inject;
        let error = prepare_profile_backend(&profile).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
