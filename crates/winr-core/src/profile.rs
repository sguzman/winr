use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tracing::{debug, info, instrument, trace, warn};
use winr_types::{
    ProfileAction, ProfileConfig, ProfileMouseButton, ProfileRunResult, WindowInfo,
    WindowSelector, WinrError, WinrResult,
};

use crate::{
    ListWindowsOptions, MouseButton, focus_window, foreground_window, list_windows, mouse_click,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileRunOptions {
    pub wait_timeout: Option<Duration>,
    pub poll_interval: Duration,
    pub max_triggers: Option<u64>,
    pub focus_target: bool,
}

impl Default for ProfileRunOptions {
    fn default() -> Self {
        Self {
            wait_timeout: None,
            poll_interval: Duration::from_millis(250),
            max_triggers: None,
            focus_target: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileRunEvent {
    WaitingForTarget { selector: WindowSelector },
    TargetAcquired { window: WindowInfo },
    TriggerFired { count: u64 },
    Stopped { count: u64, reason: String },
}

#[instrument(skip(path))]
pub fn load_profile(path: &Path) -> WinrResult<ProfileConfig> {
    let raw = fs::read_to_string(path).map_err(|error| WinrError::Unsupported {
        message: format!("failed to read profile {}: {error}", path.display()),
    })?;
    parse_profile(&raw)
}

pub fn parse_profile(raw: &str) -> WinrResult<ProfileConfig> {
    toml::from_str(raw).map_err(|error| WinrError::Unsupported {
        message: format!("failed to parse profile: {error}"),
    })
}

#[instrument(skip(profile, on_event, should_stop))]
pub fn run_profile<F, G>(
    profile: &ProfileConfig,
    options: ProfileRunOptions,
    mut on_event: F,
    mut should_stop: G,
) -> WinrResult<ProfileRunResult>
where
    F: FnMut(ProfileRunEvent),
    G: FnMut() -> bool,
{
    validate_profile(profile)?;

    let started_at = Instant::now();
    let selector = profile.target.clone();
    let target = loop {
        if should_stop() {
            return Err(WinrError::Unsupported {
                message: "profile run cancelled before the target window appeared".to_string(),
            });
        }

        on_event(ProfileRunEvent::WaitingForTarget {
            selector: selector.clone(),
        });

        if let Some(window) = resolve_profile_target(profile, options.focus_target)? {
            info!(
                hwnd = %window.hwnd,
                title = %window.title,
                profile_id = %profile.profile.id,
                "profile target resolved"
            );
            on_event(ProfileRunEvent::TargetAcquired {
                window: window.clone(),
            });
            break window;
        }

        if options
            .wait_timeout
            .is_some_and(|timeout| started_at.elapsed() >= timeout)
        {
            return Err(WinrError::WindowNotFound);
        }

        thread::sleep(options.poll_interval);
    };

    let button = match profile.action {
        ProfileAction::MouseClick { button } => button,
    };
    let every = Duration::from_millis(profile.schedule.every_ms);
    let mut fired = 0_u64;

    loop {
        if should_stop() {
            info!(fired, "profile stop requested by signal");
            on_event(ProfileRunEvent::Stopped {
                count: fired,
                reason: "received ctrl+c".to_string(),
            });
            break;
        }

        if profile.safety.stop_on_focus_loss {
            let foreground = foreground_window()?;
            if !profile.target.matches(&foreground) {
                warn!(
                    expected_title = ?profile.target.title_contains,
                    actual = %foreground.hwnd,
                    fired,
                    "profile target lost foreground; stopping"
                );
                on_event(ProfileRunEvent::Stopped {
                    count: fired,
                    reason: "target lost foreground".to_string(),
                });
                break;
            }
        }

        mouse_click(button.into(), None, None)?;
        fired += 1;
        on_event(ProfileRunEvent::TriggerFired { count: fired });

        if let Some(limit) = options.max_triggers {
            if fired >= limit {
                info!(fired, "profile reached max_triggers and is stopping");
                on_event(ProfileRunEvent::Stopped {
                    count: fired,
                    reason: "reached max_triggers".to_string(),
                });
                break;
            }
        }

        let delta = random_delta(profile.schedule.random_delta_ms);
        trace!(fired, sleep_ms = every.as_millis() as u64 + delta, "sleeping between triggers");
        thread::sleep(every + Duration::from_millis(delta));
    }

    Ok(ProfileRunResult {
        profile_id: profile.profile.id.clone(),
        profile_name: profile.profile.name.clone(),
        clicks_fired: fired,
        target_window: target,
    })
}

fn validate_profile(profile: &ProfileConfig) -> WinrResult<()> {
    if !profile.target.has_criteria() {
        return Err(WinrError::Unsupported {
            message: "profile target must include at least one selector field".to_string(),
        });
    }

    if profile.schedule.mode != "interval" {
        return Err(WinrError::Unsupported {
            message: format!(
                "profile schedule mode '{}' is unsupported; only 'interval' is implemented",
                profile.schedule.mode
            ),
        });
    }

    if profile.schedule.every_ms == 0 {
        return Err(WinrError::Unsupported {
            message: "profile schedule every_ms must be greater than zero".to_string(),
        });
    }

    if !profile.safety.require_foreground_window {
        return Err(WinrError::Unsupported {
            message: "mouse click profiles currently require require_foreground_window=true"
                .to_string(),
        });
    }

    if !matches!(profile.action, ProfileAction::MouseClick { .. }) {
        return Err(WinrError::Unsupported {
            message: "unsupported profile action".to_string(),
        });
    }

    Ok(())
}

fn resolve_profile_target(profile: &ProfileConfig, focus_target: bool) -> WinrResult<Option<WindowInfo>> {
    let mut matches = list_windows(
        &profile.target,
        ListWindowsOptions {
            visible_only: profile.safety.require_visible_window,
        },
    )?;
    matches.retain(|window| !window.minimized);
    matches.sort_by(|left, right| {
        right.foreground
            .cmp(&left.foreground)
            .then_with(|| left.hwnd.cmp(&right.hwnd))
            .then_with(|| left.title.cmp(&right.title))
    });

    let first = matches.into_iter().next();
    let Some(window) = first else {
        return Ok(None);
    };

    if profile.safety.require_foreground_window && !window.foreground {
        if focus_target {
            let selector = WindowSelector {
                hwnd: Some(window.hwnd.clone()),
                ..WindowSelector::default()
            };
            match focus_window(&selector) {
                Ok(focused) => {
                    debug!(
                        hwnd = %focused.hwnd,
                        title = %focused.title,
                        "focused profile target before start"
                    );
                    return Ok(Some(focused));
                }
                Err(error) => {
                    warn!(
                        hwnd = %window.hwnd,
                        title = %window.title,
                        %error,
                        "failed to focus profile target before start"
                    );
                    return Ok(None);
                }
            }
        }

        debug!(
            hwnd = %window.hwnd,
            title = %window.title,
            "profile target exists but is not foreground yet"
        );
        return Ok(None);
    }

    debug!(
        hwnd = %window.hwnd,
        title = %window.title,
        foreground = window.foreground,
        visible = window.visible,
        "resolved profile target candidate"
    );
    Ok(Some(window))
}

fn random_delta(max_delta_ms: u64) -> u64 {
    if max_delta_ms == 0 {
        return 0;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % (max_delta_ms + 1)
}

impl From<ProfileMouseButton> for MouseButton {
    fn from(value: ProfileMouseButton) -> Self {
        match value {
            ProfileMouseButton::Left => MouseButton::Left,
            ProfileMouseButton::Right => MouseButton::Right,
            ProfileMouseButton::Middle => MouseButton::Middle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ProfileConfig {
        parse_profile(
            r#"
[profile]
id = "demo"
name = "Demo"
description = "Demo profile"
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
random_delta_ms = 20
run_until_stopped = true

[logging]
level = "info"
mode = "single_line_counter"
update_every_trigger = true
template = "autoclicks fired: {count}"

[safety]
require_visible_window = true
require_foreground_window = true
stop_on_focus_loss = true
"#,
        )
        .expect("sample profile should parse")
    }

    #[test]
    fn parses_profile_toml() {
        let profile = sample_profile();
        assert_eq!(profile.profile.id, "demo");
        assert_eq!(profile.schedule.every_ms, 50);
        assert!(profile.safety.require_foreground_window);
    }

    #[test]
    fn profile_validation_requires_foreground_for_mouse_clicks() {
        let mut profile = sample_profile();
        profile.safety.require_foreground_window = false;
        let error = validate_profile(&profile).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn random_delta_stays_within_bounds() {
        for _ in 0..32 {
            let delta = random_delta(20);
            assert!(delta <= 20);
        }
    }
}
