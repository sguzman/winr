use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type WinrResult<T> = Result<T, WinrError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WindowSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwnd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
}

impl WindowSelector {
    pub fn has_criteria(&self) -> bool {
        self.hwnd.is_some()
            || self.pid.is_some()
            || self.title_contains.is_some()
            || self.class_name.is_some()
            || self.exe.is_some()
    }

    pub fn matches(&self, info: &WindowInfo) -> bool {
        self.hwnd
            .as_ref()
            .is_none_or(|hwnd| info.hwnd.eq_ignore_ascii_case(hwnd))
            && self.pid.is_none_or(|pid| info.pid == pid)
            && self
                .title_contains
                .as_ref()
                .is_none_or(|title| contains_case_insensitive(&info.title, title))
            && self
                .class_name
                .as_ref()
                .is_none_or(|class_name| info.class_name.eq_ignore_ascii_case(class_name))
            && self.exe.as_ref().is_none_or(|exe| {
                info.exe
                    .as_ref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(exe))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WindowInfo {
    pub hwnd: String,
    pub pid: u32,
    pub title: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    pub visible: bool,
    pub minimized: bool,
    pub foreground: bool,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WindowActionResult {
    pub action: String,
    pub window: WindowInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScreenshotBackend {
    Auto,
    Gdi,
    PrintWindow,
}

impl ScreenshotBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gdi => "gdi",
            Self::PrintWindow => "print_window",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub backend: ScreenshotBackend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputActionResult {
    pub action: String,
    pub mode: InputMode,
    pub details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    Foreground,
    Uia,
    Message,
}

impl InputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Uia => "uia",
            Self::Message => "message",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileConfig {
    pub profile: ProfileMetadata,
    pub target: WindowSelector,
    pub action: ProfileAction,
    #[serde(default)]
    pub detector: Option<ProfileDetector>,
    pub schedule: ProfileSchedule,
    pub logging: ProfileLogging,
    pub safety: ProfileSafety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileAction {
    MouseClick {
        button: ProfileMouseButton,
        #[serde(default)]
        click_point: Option<ProfileClickPoint>,
        #[serde(default)]
        x: Option<i32>,
        #[serde(default)]
        y: Option<i32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileClickPoint {
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    LeftCenter,
    RightCenter,
    BottomLeft,
    BottomCenter,
    BottomRight,
    CurrentCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileDetector {
    ColorMatch {
        red: u8,
        green: u8,
        blue: u8,
        tolerance: u8,
        min_pixels: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSchedule {
    pub mode: String,
    pub every_ms: u64,
    #[serde(default)]
    pub random_delta_ms: u64,
    #[serde(default)]
    pub run_until_stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileLogging {
    pub level: String,
    pub mode: String,
    pub update_every_trigger: bool,
    pub template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSafety {
    pub require_visible_window: bool,
    pub require_foreground_window: bool,
    pub stop_on_focus_loss: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRunResult {
    pub profile_id: String,
    pub profile_name: String,
    pub clicks_fired: u64,
    pub target_window: WindowInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SuccessResponse<T> {
    pub ok: bool,
    pub data: T,
}

impl<T> SuccessResponse<T> {
    pub fn new(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matches: Vec<WindowInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub uia_matches: Vec<UiaElementInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiaTreeMode {
    Control,
    Raw,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_control_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl UiaSelector {
    pub fn has_criteria(&self) -> bool {
        self.automation_id.is_some()
            || self.name.is_some()
            || self.class_name.is_some()
            || self.localized_control_type.is_some()
            || self.control_type.is_some()
            || self.enabled.is_some()
    }

    pub fn matches(&self, node: &UiaElementInfo) -> bool {
        self.automation_id.as_ref().is_none_or(|value| {
            node.automation_id
                .as_ref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(value))
        }) && self.name.as_ref().is_none_or(|value| {
            contains_case_insensitive(node.name.as_deref().unwrap_or_default(), value)
        }) && self.class_name.as_ref().is_none_or(|value| {
            node.class_name
                .as_ref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(value))
        }) && self.localized_control_type.as_ref().is_none_or(|value| {
            node.localized_control_type
                .as_ref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(value))
        }) && self
            .control_type
            .is_none_or(|value| node.control_type == Some(value))
            && self.enabled.is_none_or(|value| node.enabled == Some(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaElementInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwnd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_control_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<UiaElementInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaTreeRequest {
    pub window: WindowSelector,
    #[serde(default)]
    pub mode: Option<UiaTreeMode>,
    #[serde(default)]
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaTreeResponse {
    pub window: WindowInfo,
    pub mode: UiaTreeMode,
    pub root: UiaElementInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaFindRequest {
    pub window: WindowSelector,
    pub element: UiaSelector,
    #[serde(default)]
    pub mode: Option<UiaTreeMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaFindResponse {
    pub window: WindowInfo,
    pub matches: Vec<UiaElementInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaActionRequest {
    pub window: WindowSelector,
    pub element: UiaSelector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaSetTextRequest {
    pub window: WindowSelector,
    pub element: UiaSelector,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UiaActionResult {
    pub action: String,
    pub window: WindowInfo,
    pub element: UiaElementInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WinrError {
    #[error("no windows matched the provided selector")]
    WindowNotFound,
    #[error("{count} windows matched the provided selector")]
    AmbiguousWindow {
        count: usize,
        matches: Vec<WindowInfo>,
    },
    #[error("windows denied the foreground change")]
    ForegroundDenied,
    #[error("integrity level mismatch prevented the operation")]
    IntegrityLevelDenied,
    #[error("permission denied: {reason}")]
    PermissionDenied { reason: String },
    #[error("capture failed with backend {backend}: {message}")]
    CaptureFailed { backend: String, message: String },
    #[error("unsupported for minimized window during {action}")]
    UnsupportedForMinimizedWindow { action: String },
    #[error("no UI Automation elements matched the provided selector")]
    UiaElementNotFound,
    #[error("{count} UI Automation elements matched the provided selector")]
    AmbiguousUiaElement {
        count: usize,
        matches: Vec<UiaElementInfo>,
    },
    #[error("unsupported operation: {message}")]
    Unsupported { message: String },
}

impl WinrError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::WindowNotFound => "WindowNotFound",
            Self::AmbiguousWindow { .. } => "AmbiguousWindow",
            Self::ForegroundDenied => "ForegroundDenied",
            Self::IntegrityLevelDenied => "IntegrityLevelDenied",
            Self::PermissionDenied { .. } => "PermissionDenied",
            Self::CaptureFailed { .. } => "CaptureFailed",
            Self::UnsupportedForMinimizedWindow { .. } => "UnsupportedForMinimizedWindow",
            Self::UiaElementNotFound => "UiaElementNotFound",
            Self::AmbiguousUiaElement { .. } => "AmbiguousUiaElement",
            Self::Unsupported { .. } => "Unsupported",
        }
    }

    pub fn to_error_response(&self) -> ErrorResponse {
        let matches = match self {
            Self::AmbiguousWindow { matches, .. } => matches.clone(),
            _ => Vec::new(),
        };
        let uia_matches = match self {
            Self::AmbiguousUiaElement { matches, .. } => matches.clone(),
            _ => Vec::new(),
        };

        ErrorResponse {
            ok: false,
            error: self.code().to_string(),
            message: match self {
                Self::AmbiguousUiaElement { count, .. } => {
                    format!("{count} UI Automation elements matched the provided selector")
                }
                _ => self.to_string(),
            },
            matches,
            uia_matches,
        }
    }
}

pub fn format_hwnd(hwnd: isize) -> String {
    format!("0x{hwnd:016X}")
}

pub fn parse_hwnd(value: &str) -> Result<isize, String> {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    u64::from_str_radix(without_prefix, 16)
        .map(|value| value as isize)
        .map_err(|_| format!("invalid HWND value: {value}"))
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_window() -> WindowInfo {
        WindowInfo {
            hwnd: "0x0000000000001234".to_string(),
            pid: 4242,
            title: "Untitled - Notepad".to_string(),
            class_name: "Notepad".to_string(),
            exe: Some("notepad.exe".to_string()),
            visible: true,
            minimized: false,
            foreground: true,
            rect: Rect {
                left: 10,
                top: 20,
                right: 400,
                bottom: 500,
            },
        }
    }

    #[test]
    fn formats_and_parses_hwnd() {
        let formatted = format_hwnd(0x1234);
        assert_eq!(formatted, "0x0000000000001234");
        assert_eq!(parse_hwnd(&formatted).unwrap(), 0x1234);
        assert_eq!(parse_hwnd("1234").unwrap(), 0x1234);
    }

    #[test]
    fn selector_matches_case_insensitively() {
        let window = sample_window();
        let selector = WindowSelector {
            hwnd: Some("0x0000000000001234".to_string()),
            pid: Some(4242),
            title_contains: Some("notepad".to_string()),
            class_name: Some("notepad".to_string()),
            exe: Some("NOTEPAD.EXE".to_string()),
        };

        assert!(selector.matches(&window));
    }

    #[test]
    fn selector_detects_missing_criteria() {
        assert!(!WindowSelector::default().has_criteria());
        assert!(
            WindowSelector {
                pid: Some(1),
                ..WindowSelector::default()
            }
            .has_criteria()
        );
    }

    #[test]
    fn serializes_error_response() {
        let error = WinrError::AmbiguousWindow {
            count: 1,
            matches: vec![sample_window()],
        };

        let json = serde_json::to_string(&error.to_error_response()).unwrap();
        assert!(json.contains("\"error\":\"AmbiguousWindow\""));
        assert!(json.contains("\"matches\""));
    }

    #[test]
    fn serializes_uia_error_response() {
        let error = WinrError::AmbiguousUiaElement {
            count: 1,
            matches: vec![UiaElementInfo {
                hwnd: Some("0x0000000000001234".to_string()),
                automation_id: Some("username".to_string()),
                name: Some("Username".to_string()),
                class_name: Some("Edit".to_string()),
                localized_control_type: Some("edit".to_string()),
                control_type: Some(50004),
                enabled: Some(true),
                rect: None,
                children: Vec::new(),
            }],
        };

        let json = serde_json::to_string(&error.to_error_response()).unwrap();
        assert!(json.contains("\"error\":\"AmbiguousUiaElement\""));
        assert!(json.contains("\"uia_matches\""));
    }

    #[test]
    fn serializes_window_info() {
        let json = serde_json::to_string(&sample_window()).unwrap();
        assert!(json.contains("\"hwnd\":\"0x0000000000001234\""));
        assert!(json.contains("\"class_name\":\"Notepad\""));
    }

    #[test]
    fn serializes_window_action_result() {
        let result = WindowActionResult {
            action: "focus".to_string(),
            window: sample_window(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"action\":\"focus\""));
        assert!(json.contains("\"window\""));
    }

    #[test]
    fn serializes_screenshot_result() {
        let result = ScreenshotResult {
            path: "target/test.png".to_string(),
            width: 100,
            height: 80,
            backend: ScreenshotBackend::Gdi,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"backend\":\"gdi\""));
        assert!(json.contains("\"path\":\"target/test.png\""));
    }

    #[test]
    fn serializes_input_action_result() {
        let result = InputActionResult {
            action: "text".to_string(),
            mode: InputMode::Foreground,
            details: "hello".to_string(),
            window: Some(sample_window()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"action\":\"text\""));
        assert!(json.contains("\"details\":\"hello\""));
    }

    #[test]
    fn serializes_input_mode() {
        let json = serde_json::to_string(&InputMode::Message).unwrap();
        assert_eq!(json, "\"message\"");
    }
}
