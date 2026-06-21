use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use image::{ImageReader, RgbaImage};
use tracing::{debug, info, instrument, trace, warn};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use winr_inject::{
    LiveRobloxRunOptions, inspect_live_roblox_session, prepare_profile_backend_for_frontend,
    resolve_backend_selection, run_live_roblox_workflow,
};
use winr_types::{
    AdvancedCapabilitySelection, AdvancedFrontend, AdvancedProfileBackend, MouseInputMode,
    LiveSessionInspection, ProfileAction, ProfileClickPoint, ProfileConfig, ProfileDetector,
    ProfileMouseButton, ProfileRunResult, ProfileWorkflowIntegration, WindowInfo, WindowSelector,
    WinrError, WinrResult,
};

use crate::{
    ListWindowsOptions, MouseButton, capture_window_live_image, focus_window, foreground_window,
    list_windows, mouse_click_window_with_mode, parse_selector_hwnd,
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
    WaitingForFocusReturn { selector: WindowSelector },
    FocusRestored { window: WindowInfo },
    DetectorMatched { x: i32, y: i32, pixel_count: u32 },
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
    on_event: F,
    should_stop: G,
) -> WinrResult<ProfileRunResult>
where
    F: FnMut(ProfileRunEvent),
    G: FnMut() -> bool,
{
    run_profile_for_frontend(
        profile,
        options,
        AdvancedFrontend::Cli,
        on_event,
        should_stop,
    )
}

#[instrument(skip(profile, on_event, should_stop))]
pub fn run_profile_for_frontend<F, G>(
    profile: &ProfileConfig,
    options: ProfileRunOptions,
    frontend: AdvancedFrontend,
    mut on_event: F,
    mut should_stop: G,
) -> WinrResult<ProfileRunResult>
where
    F: FnMut(ProfileRunEvent),
    G: FnMut() -> bool,
{
    validate_profile(profile)?;
    if should_run_live_roblox_workflow(profile) {
        return run_live_roblox_workflow(
            profile,
            frontend,
            LiveRobloxRunOptions {
                poll_interval: Duration::from_millis(profile_workflow_tick_interval_ms(profile)),
                max_steps: options.max_triggers,
            },
            |count, _inspection| {
                on_event(ProfileRunEvent::TriggerFired { count });
            },
            should_stop,
        );
    }

    let prepared_detector = prepare_detector(profile.detector.as_ref())?;
    let backend_selection = resolve_backend_selection(profile, frontend);
    let backend_used = backend_selection.resolved;

    info!(
        requested_backend = backend_selection.requested.as_str(),
        resolved_backend = backend_used.as_str(),
        profile_id = %profile.profile.id,
        "resolved profile backend selection"
    );

    if backend_used == AdvancedProfileBackend::Inject {
        let _ = prepare_profile_backend_for_frontend(profile, frontend)?;
    }

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

    let (button, input_mode, click_x, click_y) = match &profile.action {
        ProfileAction::MouseClick {
            button,
            input_mode,
            click_point,
            x,
            y,
        } => {
            let (click_x, click_y) = resolve_click_point(&target, *click_point, *x, *y)?;
            (
                *button,
                input_mode.unwrap_or(MouseInputMode::Foreground),
                click_x,
                click_y,
            )
        }
    };
    let every = Duration::from_millis(profile.schedule.every_ms);
    let mut fired = 0_u64;
    let target_selector = WindowSelector {
        hwnd: Some(target.hwnd.clone()),
        ..WindowSelector::default()
    };
    let mut detector_armed = true;
    let mut waiting_for_focus_return = false;

    loop {
        if should_stop() {
            info!(fired, "profile stop requested by signal");
            on_event(ProfileRunEvent::Stopped {
                count: fired,
                reason: "received ctrl+c".to_string(),
            });
            break;
        }

        if profile.safety.require_foreground_window {
            let foreground = foreground_window()?;
            if !profile.target.matches(&foreground) {
                if profile.safety.pause_on_focus_loss {
                    if !waiting_for_focus_return {
                        warn!(
                            expected_title = ?profile.target.title_contains,
                            actual = %foreground.hwnd,
                            fired,
                            "profile target lost foreground; pausing until focus returns"
                        );
                        on_event(ProfileRunEvent::WaitingForFocusReturn {
                            selector: profile.target.clone(),
                        });
                        waiting_for_focus_return = true;
                    }
                    thread::sleep(options.poll_interval);
                    continue;
                }

                if profile.safety.stop_on_focus_loss {
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
            } else if waiting_for_focus_return {
                info!(
                    hwnd = %foreground.hwnd,
                    title = %foreground.title,
                    fired,
                    "profile target regained foreground; resuming"
                );
                on_event(ProfileRunEvent::FocusRestored { window: foreground });
                waiting_for_focus_return = false;
            }
        }

        let mut clicked = false;
        if let Some(detector) = prepared_detector.as_ref() {
            let capture = capture_window_live_image(&target)?;
            if let Some(match_result) = detect_match(&capture, detector)? {
                let (client_x, client_y) =
                    detector_match_to_client_coords(&target, match_result.x, match_result.y)?;
                on_event(ProfileRunEvent::DetectorMatched {
                    x: client_x,
                    y: client_y,
                    pixel_count: match_result.pixel_count,
                });
                if detector_armed {
                    mouse_click_window_with_mode(
                        &target_selector,
                        client_x,
                        client_y,
                        button.into(),
                        false,
                        input_mode,
                    )?;
                    fired += 1;
                    on_event(ProfileRunEvent::TriggerFired { count: fired });
                    detector_armed = false;
                    clicked = true;
                }
            } else {
                detector_armed = true;
            }
        } else {
            mouse_click_window_with_mode(
                &target_selector,
                click_x,
                click_y,
                button.into(),
                false,
                input_mode,
            )?;
            fired += 1;
            on_event(ProfileRunEvent::TriggerFired { count: fired });
            clicked = true;
        }

        if clicked && let Some(limit) = options.max_triggers {
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
        trace!(
            fired,
            sleep_ms = every.as_millis() as u64 + delta,
            "sleeping between triggers"
        );
        thread::sleep(every + Duration::from_millis(delta));
    }

    Ok(ProfileRunResult {
        profile_id: profile.profile.id.clone(),
        profile_name: profile.profile.name.clone(),
        clicks_fired: fired,
        backend_used,
        target_window: target,
    })
}

pub fn describe_profile_workflow(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
) -> ProfileWorkflowIntegration {
    let backend_selection = resolve_backend_selection(profile, frontend);
    let capability_selection = describe_capability_selection(profile, frontend);

    ProfileWorkflowIntegration {
        workflow_id: profile.profile.id.clone(),
        workflow_name: profile.profile.name.clone(),
        workflow_surface: "profile_v1".to_string(),
        frontend,
        target: winr_types::AdvancedTargetRef {
            hwnd: profile.target.hwnd.clone(),
            pid: profile.target.pid,
            exe: profile.target.exe.clone(),
            window_class: profile.target.class_name.clone(),
            title_hint: profile.target.title_contains.clone(),
        },
        backend_selection,
        capability_selection: capability_selection.clone(),
        available_backends: winr_inject::catalog_for_frontend(frontend).backends,
    }
}

pub fn inspect_live_profile_session(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
) -> WinrResult<LiveSessionInspection> {
    inspect_live_roblox_session(profile, frontend)
}

fn describe_capability_selection(
    profile: &ProfileConfig,
    frontend: AdvancedFrontend,
) -> AdvancedCapabilitySelection {
    let requirements = winr_inject::capability_requirements_for_profile(profile);
    let catalog = winr_inject::catalog_for_frontend(frontend);
    winr_inject::select_backend_by_capabilities(&catalog, &requirements)
}

fn should_run_live_roblox_workflow(profile: &ProfileConfig) -> bool {
    profile.execution.backend == AdvancedProfileBackend::Inject && profile.workflow.is_some()
}

fn profile_workflow_tick_interval_ms(profile: &ProfileConfig) -> u64 {
    profile
        .workflow
        .as_ref()
        .map(|workflow| workflow.tick_interval_ms)
        .filter(|value| *value > 0)
        .unwrap_or(profile.schedule.every_ms)
}

fn validate_profile(profile: &ProfileConfig) -> WinrResult<()> {
    let action_input_mode = match &profile.action {
        ProfileAction::MouseClick { input_mode, .. } => {
            input_mode.unwrap_or(MouseInputMode::Foreground)
        }
    };

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

    if action_input_mode == MouseInputMode::Foreground && !profile.safety.require_foreground_window
    {
        return Err(WinrError::Unsupported {
            message: "mouse click profiles currently require require_foreground_window=true"
                .to_string(),
        });
    }

    if profile.safety.stop_on_focus_loss && profile.safety.pause_on_focus_loss {
        return Err(WinrError::Unsupported {
            message: "profile safety cannot enable both stop_on_focus_loss and pause_on_focus_loss"
                .to_string(),
        });
    }

    if !matches!(profile.action, ProfileAction::MouseClick { .. }) {
        return Err(WinrError::Unsupported {
            message: "unsupported profile action".to_string(),
        });
    }

    if let Some(ProfileDetector::ColorMatch {
        tolerance,
        min_pixels,
        ..
    }) = &profile.detector
    {
        if *tolerance == 0 {
            return Err(WinrError::Unsupported {
                message: "color detector tolerance must be greater than zero".to_string(),
            });
        }
        if *min_pixels == 0 {
            return Err(WinrError::Unsupported {
                message: "color detector min_pixels must be greater than zero".to_string(),
            });
        }
    }

    if let Some(ProfileDetector::TemplateMatch {
        image_path,
        min_match_percent,
        sample_stride,
        ..
    }) = &profile.detector
    {
        if image_path.trim().is_empty() {
            return Err(WinrError::Unsupported {
                message: "template detector image_path must not be empty".to_string(),
            });
        }
        if *min_match_percent == 0 || *min_match_percent > 100 {
            return Err(WinrError::Unsupported {
                message: "template detector min_match_percent must be between 1 and 100"
                    .to_string(),
            });
        }
        if *sample_stride == 0 {
            return Err(WinrError::Unsupported {
                message: "template detector sample_stride must be greater than zero".to_string(),
            });
        }
    }

    Ok(())
}

fn resolve_profile_target(
    profile: &ProfileConfig,
    focus_target: bool,
) -> WinrResult<Option<WindowInfo>> {
    let mut matches = list_windows(
        &profile.target,
        ListWindowsOptions {
            visible_only: profile.safety.require_visible_window,
        },
    )?;
    matches.retain(|window| !window.minimized);
    matches.sort_by(|left, right| {
        right
            .foreground
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
    configured_point: Option<ProfileClickPoint>,
    configured_x: Option<i32>,
    configured_y: Option<i32>,
) -> WinrResult<(i32, i32)> {
    if let Some(click_point) = configured_point {
        if configured_x.is_some() || configured_y.is_some() {
            return Err(WinrError::Unsupported {
                message:
                    "profile mouse click action cannot combine click_point with x or y coordinates"
                        .to_string(),
            });
        }
        return resolve_named_click_point(target, click_point);
    }

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

fn resolve_named_click_point(
    target: &WindowInfo,
    click_point: ProfileClickPoint,
) -> WinrResult<(i32, i32)> {
    let width = (target.rect.right - target.rect.left).max(1);
    let height = (target.rect.bottom - target.rect.top).max(1);

    let point = match click_point {
        ProfileClickPoint::Center => (width / 2, height / 2),
        ProfileClickPoint::TopLeft => (1.min(width - 1), 1.min(height - 1)),
        ProfileClickPoint::TopCenter => (width / 2, 1.min(height - 1)),
        ProfileClickPoint::TopRight => ((width - 2).max(0), 1.min(height - 1)),
        ProfileClickPoint::LeftCenter => (1.min(width - 1), height / 2),
        ProfileClickPoint::RightCenter => ((width - 2).max(0), height / 2),
        ProfileClickPoint::BottomLeft => (1.min(width - 1), (height - 2).max(0)),
        ProfileClickPoint::BottomCenter => (width / 2, (height - 2).max(0)),
        ProfileClickPoint::BottomRight => ((width - 2).max(0), (height - 2).max(0)),
        ProfileClickPoint::CurrentCursor => cursor_point_in_target(target)?,
    };

    Ok(point)
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

fn detector_match_to_client_coords(
    target: &WindowInfo,
    local_capture_x: i32,
    local_capture_y: i32,
) -> WinrResult<(i32, i32)> {
    let hwnd = parse_selector_hwnd(&target.hwnd);
    let mut point = POINT {
        x: target.rect.left + local_capture_x,
        y: target.rect.top + local_capture_y,
    };
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return Err(WinrError::Unsupported {
            message: "ScreenToClient failed while translating detector match coordinates"
                .to_string(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DetectorMatch {
    x: i32,
    y: i32,
    pixel_count: u32,
}

enum PreparedDetector {
    ColorMatch {
        red: u8,
        green: u8,
        blue: u8,
        tolerance: u8,
        min_pixels: u32,
    },
    TemplateMatch {
        template: RgbaImage,
        pixel_tolerance: u8,
        min_match_percent: u8,
        sample_stride: u32,
    },
}

fn prepare_detector(detector: Option<&ProfileDetector>) -> WinrResult<Option<PreparedDetector>> {
    let Some(detector) = detector else {
        return Ok(None);
    };

    match detector {
        ProfileDetector::ColorMatch {
            red,
            green,
            blue,
            tolerance,
            min_pixels,
        } => Ok(Some(PreparedDetector::ColorMatch {
            red: *red,
            green: *green,
            blue: *blue,
            tolerance: *tolerance,
            min_pixels: *min_pixels,
        })),
        ProfileDetector::TemplateMatch {
            image_path,
            pixel_tolerance,
            min_match_percent,
            sample_stride,
        } => {
            let template = load_template_image(image_path)?;
            Ok(Some(PreparedDetector::TemplateMatch {
                template,
                pixel_tolerance: *pixel_tolerance,
                min_match_percent: *min_match_percent,
                sample_stride: *sample_stride,
            }))
        }
    }
}

fn load_template_image(path: &str) -> WinrResult<RgbaImage> {
    let full_path = PathBuf::from(path);
    let image = ImageReader::open(&full_path)
        .map_err(|error| WinrError::Unsupported {
            message: format!(
                "failed to open template image {}: {error}",
                full_path.display()
            ),
        })?
        .decode()
        .map_err(|error| WinrError::Unsupported {
            message: format!(
                "failed to decode template image {}: {error}",
                full_path.display()
            ),
        })?;
    Ok(image.to_rgba8())
}

fn detect_match(
    image: &RgbaImage,
    detector: &PreparedDetector,
) -> WinrResult<Option<DetectorMatch>> {
    match detector {
        PreparedDetector::ColorMatch {
            red,
            green,
            blue,
            tolerance,
            min_pixels,
        } => Ok(detect_color_match(
            image,
            (*red, *green, *blue),
            *tolerance,
            *min_pixels,
        )),
        PreparedDetector::TemplateMatch {
            template,
            pixel_tolerance,
            min_match_percent,
            sample_stride,
        } => Ok(detect_template_match(
            image,
            template,
            *pixel_tolerance,
            *min_match_percent,
            *sample_stride,
        )),
    }
}

fn detect_color_match(
    image: &RgbaImage,
    target: (u8, u8, u8),
    tolerance: u8,
    min_pixels: u32,
) -> Option<DetectorMatch> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let mut matches = vec![false; width * height];

    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x as u32, y as u32);
            if color_within_tolerance(pixel.0[0], pixel.0[1], pixel.0[2], target, tolerance) {
                matches[y * width + x] = true;
            }
        }
    }

    let mut visited = vec![false; width * height];
    let mut best: Option<DetectorMatch> = None;

    for start_y in 0..height {
        for start_x in 0..width {
            let start_index = start_y * width + start_x;
            if !matches[start_index] || visited[start_index] {
                continue;
            }

            let mut stack = vec![(start_x, start_y)];
            visited[start_index] = true;
            let mut pixel_count = 0_u32;
            let mut sum_x = 0_u64;
            let mut sum_y = 0_u64;

            while let Some((x, y)) = stack.pop() {
                pixel_count += 1;
                sum_x += x as u64;
                sum_y += y as u64;

                for (nx, ny) in neighbors(x, y, width, height) {
                    let index = ny * width + nx;
                    if matches[index] && !visited[index] {
                        visited[index] = true;
                        stack.push((nx, ny));
                    }
                }
            }

            if pixel_count < min_pixels {
                continue;
            }

            let candidate = DetectorMatch {
                x: (sum_x / pixel_count as u64) as i32,
                y: (sum_y / pixel_count as u64) as i32,
                pixel_count,
            };

            if best.is_none_or(|current| candidate.pixel_count > current.pixel_count) {
                best = Some(candidate);
            }
        }
    }

    best
}

fn detect_template_match(
    image: &RgbaImage,
    template: &RgbaImage,
    pixel_tolerance: u8,
    min_match_percent: u8,
    sample_stride: u32,
) -> Option<DetectorMatch> {
    let image_width = image.width();
    let image_height = image.height();
    let template_width = template.width();
    let template_height = template.height();
    if template_width == 0
        || template_height == 0
        || template_width > image_width
        || template_height > image_height
    {
        return None;
    }

    let stride = sample_stride.max(1) as usize;
    let mut best_score = 0_u32;
    let mut best_total = 0_u32;
    let mut best_position = None;

    let max_y = (image_height - template_height) as usize;
    let max_x = (image_width - template_width) as usize;
    for offset_y in (0..=max_y).step_by(stride) {
        for offset_x in (0..=max_x).step_by(stride) {
            let mut matched = 0_u32;
            let mut total = 0_u32;

            for template_y in (0..template_height as usize).step_by(stride) {
                for template_x in (0..template_width as usize).step_by(stride) {
                    let template_pixel = template.get_pixel(template_x as u32, template_y as u32).0;
                    let image_pixel = image
                        .get_pixel(
                            (offset_x + template_x) as u32,
                            (offset_y + template_y) as u32,
                        )
                        .0;
                    total += 1;
                    if color_within_tolerance(
                        image_pixel[0],
                        image_pixel[1],
                        image_pixel[2],
                        (template_pixel[0], template_pixel[1], template_pixel[2]),
                        pixel_tolerance,
                    ) {
                        matched += 1;
                    }
                }
            }

            if total == 0 {
                continue;
            }

            let percent = matched * 100 / total;
            if percent >= min_match_percent as u32
                && (matched > best_score || (matched == best_score && total > best_total))
            {
                best_score = matched;
                best_total = total;
                best_position = Some((offset_x as i32, offset_y as i32));
            }
        }
    }

    best_position.map(|(offset_x, offset_y)| DetectorMatch {
        x: offset_x + random_click_offset(template_width as i32),
        y: offset_y + random_click_offset(template_height as i32),
        pixel_count: best_score,
    })
}

fn random_click_offset(size: i32) -> i32 {
    let min = (size / 4).max(1);
    let max = ((size * 3) / 4).max(min + 1);
    let span = (max - min).max(1) as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or(0);
    min + (nanos % span) as i32
}

fn neighbors(x: usize, y: usize, width: usize, height: usize) -> [(usize, usize); 4] {
    [
        (x.saturating_sub(1), y),
        ((x + 1).min(width.saturating_sub(1)), y),
        (x, y.saturating_sub(1)),
        (x, (y + 1).min(height.saturating_sub(1))),
    ]
}

fn color_within_tolerance(
    red: u8,
    green: u8,
    blue: u8,
    target: (u8, u8, u8),
    tolerance: u8,
) -> bool {
    red.abs_diff(target.0) <= tolerance
        && green.abs_diff(target.1) <= tolerance
        && blue.abs_diff(target.2) <= tolerance
}

#[cfg(test)]
fn detect_all_matching_pixels(image: &RgbaImage, target: (u8, u8, u8), tolerance: u8) -> u32 {
    let mut pixel_count = 0_u32;
    for (_, _, pixel) in image.enumerate_pixels() {
        if color_within_tolerance(pixel.0[0], pixel.0[1], pixel.0[2], target, tolerance) {
            pixel_count += 1;
        }
    }
    pixel_count
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
click_point = "center"

[detector]
kind = "color_match"
red = 179
green = 48
blue = 218
tolerance = 40
min_pixels = 200

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
pause_on_focus_loss = false
"#,
        )
        .expect("sample profile should parse")
    }

    #[test]
    fn parses_profile_toml() {
        let profile = sample_profile();
        assert_eq!(profile.profile.id, "demo");
        assert_eq!(profile.execution.backend, AdvancedProfileBackend::Auto);
        assert_eq!(profile.schedule.every_ms, 50);
        assert!(profile.safety.require_foreground_window);
        match profile.detector.as_ref().expect("detector should exist") {
            ProfileDetector::ColorMatch {
                red,
                green,
                blue,
                tolerance,
                min_pixels,
            } => {
                assert_eq!((*red, *green, *blue), (179, 48, 218));
                assert_eq!(*tolerance, 40);
                assert_eq!(*min_pixels, 200);
            }
            ProfileDetector::TemplateMatch { .. } => {
                panic!("sample profile should use the color detector in this unit test")
            }
        }
        match profile.action {
            ProfileAction::MouseClick {
                click_point, x, y, ..
            } => {
                assert_eq!(click_point, Some(ProfileClickPoint::Center));
                assert_eq!(x, None);
                assert_eq!(y, None);
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
    fn profile_validation_allows_background_message_mouse_clicks() {
        let mut profile = sample_profile();
        let ProfileAction::MouseClick { input_mode, .. } = &mut profile.action;
        *input_mode = Some(MouseInputMode::Message);
        profile.safety.require_foreground_window = false;
        validate_profile(&profile).expect("message-mode mouse click profile should validate");
    }

    #[test]
    fn profile_validation_rejects_conflicting_focus_loss_modes() {
        let mut profile = sample_profile();
        profile.safety.pause_on_focus_loss = true;
        let error = validate_profile(&profile).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn profile_workflow_description_uses_shared_selection_surface() {
        let profile = sample_profile();
        let integration = describe_profile_workflow(&profile, AdvancedFrontend::Mcp);

        assert_eq!(integration.workflow_id, "demo");
        assert_eq!(integration.workflow_surface, "profile_v1");
        assert_eq!(integration.frontend, AdvancedFrontend::Mcp);
        assert_eq!(
            integration.backend_selection.frontend,
            AdvancedFrontend::Mcp
        );
        assert_eq!(
            integration.capability_selection.selected_backend,
            Some(AdvancedProfileBackend::Foreground)
        );
        assert!(!integration.available_backends.is_empty());
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

        let error = resolve_click_point(&target, None, Some(50), None).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn resolve_click_point_rejects_mixed_named_and_explicit_coordinates() {
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

        let error =
            resolve_click_point(&target, Some(ProfileClickPoint::Center), Some(50), Some(60))
                .unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn resolve_named_click_point_center_uses_window_center() {
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

        let point = resolve_named_click_point(&target, ProfileClickPoint::Center)
            .expect("center point should resolve");
        assert_eq!(point, (400, 300));
    }

    #[test]
    fn resolve_click_point_explicit_coordinates_still_work() {
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

        let point =
            resolve_click_point(&target, None, Some(320), Some(320)).expect("point should work");
        assert_eq!(point, (320, 320));
    }

    #[test]
    fn resolve_named_click_point_bottom_right_stays_in_bounds() {
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
                right: 5,
                bottom: 5,
            },
        };

        let point = resolve_named_click_point(&target, ProfileClickPoint::BottomRight)
            .expect("bottom-right point should resolve");
        assert_eq!(point, (3, 3));
    }

    #[test]
    fn resolve_click_point_rejects_partial_explicit_coordinates() {
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

        let error = resolve_click_point(&target, None, Some(50), None).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn detect_color_match_finds_cluster_centroid() {
        let mut image = RgbaImage::new(10, 10);
        for y in 4..7 {
            for x in 2..5 {
                image.put_pixel(x, y, image::Rgba([180, 50, 220, 255]));
            }
        }

        let found =
            detect_color_match(&image, (179, 48, 218), 5, 4).expect("cluster should be detected");
        assert_eq!(found.x, 3);
        assert_eq!(found.y, 5);
        assert_eq!(found.pixel_count, 9);
    }

    #[test]
    fn detect_color_match_ignores_small_matches() {
        let mut image = RgbaImage::new(10, 10);
        image.put_pixel(1, 1, image::Rgba([180, 50, 220, 255]));

        let found = detect_color_match(&image, (179, 48, 218), 5, 2);
        assert!(found.is_none());
    }

    #[test]
    fn detect_color_match_prefers_largest_connected_cluster() {
        let mut image = RgbaImage::new(120, 120);
        for y in 10..20 {
            for x in 10..20 {
                image.put_pixel(x, y, image::Rgba([180, 50, 220, 255]));
            }
        }
        for y in 80..110 {
            for x in 70..105 {
                image.put_pixel(x, y, image::Rgba([180, 50, 220, 255]));
            }
        }

        let total = detect_all_matching_pixels(&image, (179, 48, 218), 5);
        assert_eq!(total, 1150);

        let found = detect_color_match(&image, (179, 48, 218), 5, 20)
            .expect("largest cluster should be selected");
        assert_eq!(found.pixel_count, 1050);
        assert!((found.x - 87).abs() <= 1);
        assert!((found.y - 94).abs() <= 1);
    }

    #[test]
    fn detector_match_event_uses_capture_local_coordinates() {
        let mut image = RgbaImage::new(20, 20);
        for y in 8..11 {
            for x in 5..8 {
                image.put_pixel(x, y, image::Rgba([180, 50, 220, 255]));
            }
        }

        let found = detect_match(
            &image,
            &PreparedDetector::ColorMatch {
                red: 179,
                green: 48,
                blue: 218,
                tolerance: 5,
                min_pixels: 4,
            },
        )
        .expect("detector call should succeed")
        .expect("match should exist");

        assert_eq!(found.x, 6);
        assert_eq!(found.y, 9);
    }
}
