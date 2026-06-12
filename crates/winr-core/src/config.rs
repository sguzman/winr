use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::Deserialize;
use tracing::{debug, instrument};
use winr_types::{WindowInfo, WinrError, WinrResult};

static CONFIG: OnceLock<WinrResult<WinrConfig>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub struct WinrConfig {
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub allowlist: ProcessList,
    #[serde(default)]
    pub denylist: DenyList,
    #[serde(default)]
    pub mcp: McpConfig,
}

impl Default for WinrConfig {
    fn default() -> Self {
        Self {
            permissions: PermissionsConfig::default(),
            allowlist: ProcessList::default(),
            denylist: DenyList::default(),
            mcp: McpConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default = "default_true")]
    pub allow_input: bool,
    #[serde(default = "default_true")]
    pub allow_mouse: bool,
    #[serde(default = "default_true")]
    pub allow_screenshots: bool,
    #[serde(default)]
    pub allow_window_close: bool,
    #[serde(default = "default_true")]
    pub require_confirm_for_close: bool,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            allow_input: true,
            allow_mouse: true,
            allow_screenshots: true,
            allow_window_close: false,
            require_confirm_for_close: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProcessList {
    #[serde(default)]
    pub processes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DenyList {
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub titles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    #[serde(default = "default_mcp_bind")]
    pub bind: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_true")]
    pub log_tool_calls: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind: default_mcp_bind(),
            transport: default_transport(),
            log_tool_calls: true,
        }
    }
}

pub fn current_config() -> WinrResult<&'static WinrConfig> {
    CONFIG
        .get_or_init(load_config)
        .as_ref()
        .map_err(Clone::clone)
}

pub fn current_mcp_config() -> WinrResult<McpConfig> {
    Ok(current_config()?.mcp.clone())
}

#[instrument]
pub fn enforce_screenshot_permission(target: Option<&WindowInfo>) -> WinrResult<()> {
    let config = current_config()?;
    if !config.permissions.allow_screenshots {
        return Err(WinrError::PermissionDenied {
            reason: "screenshots are disabled by config".to_string(),
        });
    }

    if let Some(target) = target {
        enforce_target_policy(config, target, "screenshot")?;
    }

    Ok(())
}

#[instrument]
pub fn enforce_input_permission(target: &WindowInfo, action: &str) -> WinrResult<()> {
    let config = current_config()?;
    if !config.permissions.allow_input {
        return Err(WinrError::PermissionDenied {
            reason: "input automation is disabled by config".to_string(),
        });
    }

    enforce_target_policy(config, target, action)
}

#[instrument]
pub fn enforce_mouse_permission(target: Option<&WindowInfo>, action: &str) -> WinrResult<()> {
    let config = current_config()?;
    if !config.permissions.allow_mouse {
        return Err(WinrError::PermissionDenied {
            reason: "mouse automation is disabled by config".to_string(),
        });
    }

    if let Some(target) = target {
        enforce_target_policy(config, target, action)?;
    }

    Ok(())
}

#[instrument]
pub fn enforce_window_close_permission(target: &WindowInfo, force: bool) -> WinrResult<()> {
    let config = current_config()?;
    if !config.permissions.allow_window_close {
        return Err(WinrError::PermissionDenied {
            reason: "window close is disabled by config".to_string(),
        });
    }

    if config.permissions.require_confirm_for_close && !force {
        return Err(WinrError::PermissionDenied {
            reason: "window close requires --force or require_confirm_for_close=false".to_string(),
        });
    }

    enforce_target_policy(config, target, "window_close")
}

pub fn validate_window_ready_for_input(
    target: &WindowInfo,
    focus_first: bool,
    action: &str,
) -> WinrResult<()> {
    if target.minimized {
        return Err(WinrError::UnsupportedForMinimizedWindow {
            action: action.to_string(),
        });
    }

    if !focus_first && !target.foreground {
        return Err(WinrError::Unsupported {
            message: format!(
                "{action} requires the target window to already be foreground when focus_first=false"
            ),
        });
    }

    Ok(())
}

pub fn validate_window_ready_for_mouse(target: &WindowInfo, action: &str) -> WinrResult<()> {
    if target.minimized {
        return Err(WinrError::UnsupportedForMinimizedWindow {
            action: action.to_string(),
        });
    }

    Ok(())
}

fn enforce_target_policy(config: &WinrConfig, target: &WindowInfo, action: &str) -> WinrResult<()> {
    if let Some(exe) = &target.exe {
        if config
            .denylist
            .processes
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(exe))
        {
            return Err(WinrError::PermissionDenied {
                reason: format!("{action} denied for process '{exe}'"),
            });
        }
    }

    if config
        .denylist
        .titles
        .iter()
        .any(|entry| contains_case_insensitive(&target.title, entry))
    {
        return Err(WinrError::PermissionDenied {
            reason: format!("{action} denied for title '{}'", target.title),
        });
    }

    if !config.allowlist.processes.is_empty() {
        let matches = target.exe.as_ref().is_some_and(|exe| {
            config
                .allowlist
                .processes
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(exe))
        });

        if !matches {
            let exe = target.exe.as_deref().unwrap_or("<unknown>");
            return Err(WinrError::PermissionDenied {
                reason: format!("{action} denied because process '{exe}' is not allowlisted"),
            });
        }
    }

    Ok(())
}

fn load_config() -> WinrResult<WinrConfig> {
    let path = config_path();
    match path {
        Some(path) if path.exists() => load_config_from_path(&path),
        Some(path) => {
            debug!(path = %path.display(), "winr config not found, using defaults");
            Ok(WinrConfig::default())
        }
        None => {
            debug!("unable to resolve config directory, using defaults");
            Ok(WinrConfig::default())
        }
    }
}

fn load_config_from_path(path: &Path) -> WinrResult<WinrConfig> {
    let contents = fs::read_to_string(path).map_err(|error| WinrError::Unsupported {
        message: format!("failed to read config {}: {error}", path.display()),
    })?;
    let config =
        toml::from_str::<WinrConfig>(&contents).map_err(|error| WinrError::Unsupported {
            message: format!("failed to parse config {}: {error}", path.display()),
        })?;
    debug!(path = %path.display(), "loaded winr config");
    Ok(config)
}

fn config_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WINR_CONFIG") {
        return Some(PathBuf::from(path));
    }

    dirs::config_dir().map(|dir| dir.join("winr").join("config.toml"))
}

fn default_true() -> bool {
    true
}

fn default_mcp_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_window(title: &str, exe: &str) -> WindowInfo {
        WindowInfo {
            hwnd: "0x0000000000000010".to_string(),
            pid: 10,
            title: title.to_string(),
            class_name: "Demo".to_string(),
            exe: Some(exe.to_string()),
            visible: true,
            minimized: false,
            foreground: true,
            rect: winr_types::Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
        }
    }

    #[test]
    fn parses_sample_config() {
        let config = toml::from_str::<WinrConfig>(
            r#"
[permissions]
allow_input = true
allow_mouse = false
allow_screenshots = true
allow_window_close = true
require_confirm_for_close = false

[allowlist]
processes = ["notepad.exe"]

[denylist]
processes = ["KeePassXC.exe"]
titles = ["Password"]
"#,
        )
        .unwrap();

        assert!(config.permissions.allow_input);
        assert!(!config.permissions.allow_mouse);
        assert_eq!(config.allowlist.processes, vec!["notepad.exe"]);
        assert_eq!(config.denylist.titles, vec!["Password"]);
    }

    #[test]
    fn denylist_blocks_target() {
        let config = WinrConfig {
            denylist: DenyList {
                processes: vec!["keepassxc.exe".to_string()],
                titles: vec!["bank".to_string()],
            },
            ..WinrConfig::default()
        };

        let process_error =
            enforce_target_policy(&config, &sample_window("Safe", "KeePassXC.exe"), "input")
                .unwrap_err();
        assert!(matches!(process_error, WinrError::PermissionDenied { .. }));

        let title_error = enforce_target_policy(
            &config,
            &sample_window("Bank Login", "notepad.exe"),
            "input",
        )
        .unwrap_err();
        assert!(matches!(title_error, WinrError::PermissionDenied { .. }));
    }

    #[test]
    fn allowlist_requires_match() {
        let config = WinrConfig {
            allowlist: ProcessList {
                processes: vec!["notepad.exe".to_string()],
            },
            ..WinrConfig::default()
        };

        let error = enforce_target_policy(&config, &sample_window("Untitled", "calc.exe"), "input")
            .unwrap_err();
        assert!(matches!(error, WinrError::PermissionDenied { .. }));
    }

    #[test]
    fn input_validation_rejects_background_target_without_focus() {
        let mut window = sample_window("Untitled", "notepad.exe");
        window.foreground = false;

        let error = validate_window_ready_for_input(&window, false, "input_text").unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }

    #[test]
    fn mouse_validation_rejects_minimized_target() {
        let mut window = sample_window("Untitled", "notepad.exe");
        window.minimized = true;

        let error = validate_window_ready_for_mouse(&window, "mouse_click_window").unwrap_err();
        assert!(matches!(
            error,
            WinrError::UnsupportedForMinimizedWindow { .. }
        ));
    }
}
