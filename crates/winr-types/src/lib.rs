use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type WinrResult<T> = Result<T, WinrError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowActionResult {
    pub action: String,
    pub window: WindowInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessResponse<T> {
    pub ok: bool,
    pub data: T,
}

impl<T> SuccessResponse<T> {
    pub fn new(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matches: Vec<WindowInfo>,
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
            Self::Unsupported { .. } => "Unsupported",
        }
    }

    pub fn to_error_response(&self) -> ErrorResponse {
        let matches = match self {
            Self::AmbiguousWindow { matches, .. } => matches.clone(),
            _ => Vec::new(),
        };

        ErrorResponse {
            ok: false,
            error: self.code().to_string(),
            message: self.to_string(),
            matches,
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
}
