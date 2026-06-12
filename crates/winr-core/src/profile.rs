use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tracing::{debug, info, instrument, trace, warn};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use winr_types::{
    ProfileAction, ProfileConfig, ProfileMouseButton, ProfileRunResult, WindowInfo,
    WindowSelector, WinrError, WinrResult,
};

use crate::{
    ListWindowsOptions, MouseButton, focus_window, foreground_window, list_windows,
    mouse_click_window, parse_selector_hwnd,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileRunOptions {
    pub wait_timeout: Option<Duration>,
    pub poll_interval: Duration,
    pub max_triggers: Option<u64>,
    pub focus_target: bool,
    pub arm_delay: Duration,
}

impl Default for ProfileRunOptions {
    fn default() -> Self {
        Self {
            wait_timeout: None,
            poll_interval: Duration::from_millis(250),
            max_triggers: None,
            focus_target: false,
            arm_delay: Duration::ZERO,
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

    if options.arm_delay > Duration::ZERO {
        info!(
            arm_delay_ms = options.arm_delay.as_millis() as u64,
            "arming profile before input loop"
        );
        thread::sleep(options.arm_delay);
    }

    let (button, click_x, click_y) = match &profile.action {
        ProfileAction::MouseClick { button, x, y } => {
            let (click_x, click_y) = resolve_click_point(&target, *x, *y)?;
            (*button, click_x, click_y)
        }
    };
    let every = Duration::from_millis(profile.schedule.every_ms);
    let mut fired = 0_u64;
    let target_selector = WindowSelector {
        hwnd: Some(target.hwnd.clone()),
        ..WindowSelector::default()
    };

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

        mouse_click_window(&target_selector, click_x, click_y, button.into(), false)?;
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

fn resolve_click_point(
    target: &WindowInfo,
    configured_x: Option<i32>,
    configured_y: Option<i32>,
) -> WinrResult<(i32, i32)> {
    match (configured_x, configured_y) {
        (Some(x), Some(y)) => Ok((x, y)),
        (Some(_), None) | (None, Some(_)) => Err(WinrError::Unsupported {
            message: "profile mouse click action requires both x and y when either is provided"
                .to_string(),
        }),
        (None, None) => cursor_point_in_target(target).or_else(|_| {
            let width = (target.rect.right - target.rect.left).max(1);
            let height = (target.rect.bottom - target.rect.top).max(1);
            let center = (width / 2, height / 2);
            warn!(
                hwnd = %target.hwnd,
                x = center.0,
                y = center.1,
                "cursor was not inside target window; falling back to window center"
            );
            Ok(center)
        }),
    }
}

fn cursor_point_in_target(target: &WindowInfo) -> WinrResult<(i32, i32)> {
    let hwnd = parse_selector_hwnd(&target.hwnd);
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.map_err(|error| WinrError::Unsupported {
        message: format!("GetCursorPos failed while resolving profile click point: {error}"),
    })?;
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return Err(WinrError::Unsupported {
            message: "ScreenToClient failed while resolving profile click point".to_string(),
        });
    }

    let width = (target.rect.right - target.rect.left).max(1);
    let height = (target.rect.bottom - target.rect.top).max(1);
    if point.x < 0 || point.y < 0 || point.x >= width || point.y >= height {
        return Err(WinrError::Unsupported {
            message: "current cursor is not inside the target window".to_string(),
        });
    }

    Ok((point.x, point.y))
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
x = 320
y = 320

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
        match profile.action {
            ProfileAction::MouseClick { x, y, .. } => {
                assert_eq!(x, Some(320));
                assert_eq!(y, Some(320));
            }
        }
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

    #[test]
    fn resolve_click_point_requires_both_coordinates_when_configured() {
        let target = WindowInfo {
            hwnd: "0x0000000000000001".to_string(),
            pid: 1,
            title: "Roblox".to_string(),
            class_name: "WINDOWSCLIENT".to_string(),
            exe: Some("RobloxPlayerBeta.exe".to_string()),
            visible: true,
            minimized: false,
            foreground: true,
            rect: winr_types::Rect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 600,
            },
        };

        let error = resolve_click_point(&target, Some(50), None).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
