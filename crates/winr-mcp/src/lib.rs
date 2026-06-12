use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use rmcp::{
    ServiceExt,
    handler::server::{tool::IntoCallToolResult, wrapper::Parameters},
    model::CallToolResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use winr_core::{
    ListWindowsOptions, focus_window as core_focus_window, input_keys as core_input_keys,
    input_text as core_input_text, list_windows as core_list_windows,
    mouse_click as core_mouse_click, mouse_click_window as core_mouse_click_window,
    move_window as core_move_window, restore_window as core_restore_window,
    screenshot_window as core_screenshot_window, uia_find as core_uia_find,
    uia_invoke as core_uia_invoke, uia_set_text as core_uia_set_text,
    uia_tree as core_uia_tree, window_info as core_window_info,
};
use winr_types::{
    ErrorResponse, InputActionResult, ScreenshotBackend, ScreenshotResult, SuccessResponse,
    UiaActionRequest, UiaActionResult, UiaFindRequest, UiaFindResponse, UiaSetTextRequest,
    UiaTreeRequest, UiaTreeResponse, WindowActionResult, WindowInfo, WindowSelector, WinrError,
};

#[derive(Debug, Clone, Default)]
pub struct WinrMcpServer;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowsListParams {
    #[serde(default)]
    pub visible_only: bool,
    #[serde(default)]
    pub selector: WindowSelector,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowSelectorParams {
    pub selector: WindowSelector,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowMoveParams {
    pub selector: WindowSelector,
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowScreenshotParams {
    pub selector: WindowSelector,
    #[serde(default)]
    pub out: Option<String>,
    #[serde(default)]
    pub backend: Option<ScreenshotBackend>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InputKeysParams {
    #[serde(default)]
    pub selector: Option<WindowSelector>,
    pub combo: String,
    #[serde(default = "default_true")]
    pub focus_first: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InputTextParams {
    #[serde(default)]
    pub selector: Option<WindowSelector>,
    pub text: String,
    #[serde(default = "default_true")]
    pub focus_first: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MouseClickParams {
    #[serde(default)]
    pub selector: Option<WindowSelector>,
    #[serde(default)]
    pub button: Option<MouseButtonParam>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default = "default_true")]
    pub focus_first: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MouseButtonParam {
    Left,
    Right,
    Middle,
}

impl From<MouseButtonParam> for winr_core::MouseButton {
    fn from(value: MouseButtonParam) -> Self {
        match value {
            MouseButtonParam::Left => winr_core::MouseButton::Left,
            MouseButtonParam::Right => winr_core::MouseButton::Right,
            MouseButtonParam::Middle => winr_core::MouseButton::Middle,
        }
    }
}

pub async fn serve_stdio() -> anyhow::Result<()> {
    info!("starting winr MCP stdio server");
    let transport = rmcp::transport::io::stdio();
    WinrMcpServer
        .serve(transport)
        .await
        .context("failed to start winr MCP server")?
        .waiting()
        .await
        .context("winr MCP server exited with an error")?;
    Ok(())
}

pub fn serve_stdio_blocking() -> anyhow::Result<()> {
    tokio::runtime::Runtime::new()
        .context("failed to create tokio runtime for winr MCP server")?
        .block_on(serve_stdio())
}

#[rmcp::tool_router(server_handler)]
impl WinrMcpServer {
    #[rmcp::tool(name = "windows_list", description = "List top-level desktop windows")]
    #[instrument(skip(self, params))]
    async fn windows_list(
        &self,
        Parameters(params): Parameters<WindowsListParams>,
    ) -> McpToolResult<Vec<WindowInfo>> {
        from_winr(core_list_windows(
            &params.selector,
            ListWindowsOptions {
                visible_only: params.visible_only,
            },
        ))
    }

    #[rmcp::tool(name = "window_info", description = "Inspect one matching top-level window")]
    #[instrument(skip(self, params))]
    async fn window_info(
        &self,
        Parameters(params): Parameters<WindowSelectorParams>,
    ) -> McpToolResult<WindowInfo> {
        from_winr(core_window_info(&params.selector))
    }

    #[rmcp::tool(name = "window_focus", description = "Bring a matching window to the foreground")]
    #[instrument(skip(self, params))]
    async fn window_focus(
        &self,
        Parameters(params): Parameters<WindowSelectorParams>,
    ) -> McpToolResult<WindowInfo> {
        from_winr(core_focus_window(&params.selector))
    }

    #[rmcp::tool(name = "window_restore", description = "Restore a minimized or hidden window")]
    #[instrument(skip(self, params))]
    async fn window_restore(
        &self,
        Parameters(params): Parameters<WindowSelectorParams>,
    ) -> McpToolResult<WindowActionResult> {
        from_winr(core_restore_window(&params.selector))
    }

    #[rmcp::tool(name = "window_move", description = "Move and optionally resize a window")]
    #[instrument(skip(self, params))]
    async fn window_move(
        &self,
        Parameters(params): Parameters<WindowMoveParams>,
    ) -> McpToolResult<WindowActionResult> {
        from_winr(core_move_window(
            &params.selector,
            params.x,
            params.y,
            params.width,
            params.height,
        ))
    }

    #[rmcp::tool(name = "window_screenshot", description = "Capture a screenshot of a window")]
    #[instrument(skip(self, params))]
    async fn window_screenshot(
        &self,
        Parameters(params): Parameters<WindowScreenshotParams>,
    ) -> McpToolResult<ScreenshotResult> {
        let out = params.out.map(PathBuf::from).unwrap_or_else(default_screenshot_path);
        let backend = params.backend.unwrap_or(ScreenshotBackend::Auto);
        from_winr(core_screenshot_window(&params.selector, &out, backend))
    }

    #[rmcp::tool(name = "input_send_keys", description = "Send a key combination to a window")]
    #[instrument(skip(self, params))]
    async fn input_send_keys(
        &self,
        Parameters(params): Parameters<InputKeysParams>,
    ) -> McpToolResult<InputActionResult> {
        from_winr(core_input_keys(
            params.selector.as_ref(),
            &params.combo,
            params.focus_first,
        ))
    }

    #[rmcp::tool(name = "input_send_text", description = "Send text input to a window")]
    #[instrument(skip(self, params))]
    async fn input_send_text(
        &self,
        Parameters(params): Parameters<InputTextParams>,
    ) -> McpToolResult<InputActionResult> {
        from_winr(core_input_text(
            params.selector.as_ref(),
            &params.text,
            params.focus_first,
        ))
    }

    #[rmcp::tool(name = "mouse_click", description = "Click at screen coordinates or within a selected window")]
    #[instrument(skip(self, params))]
    async fn mouse_click(
        &self,
        Parameters(params): Parameters<MouseClickParams>,
    ) -> McpToolResult<InputActionResult> {
        let button = params.button.unwrap_or(MouseButtonParam::Left).into();
        let result = match params.selector {
            Some(selector) => {
                let x = params.x.ok_or_else(|| WinrError::Unsupported {
                    message: "mouse_click with a selector requires x and y coordinates".to_string(),
                });
                let y = params.y.ok_or_else(|| WinrError::Unsupported {
                    message: "mouse_click with a selector requires x and y coordinates".to_string(),
                });
                match (x, y) {
                    (Ok(x), Ok(y)) => {
                        core_mouse_click_window(&selector, x, y, button, params.focus_first)
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            }
            None => core_mouse_click(button, params.x, params.y),
        };

        from_winr(result)
    }

    #[rmcp::tool(name = "uia_tree", description = "Read the UI Automation tree for a window")]
    #[instrument(skip(self, params))]
    async fn uia_tree(
        &self,
        Parameters(params): Parameters<UiaTreeRequest>,
    ) -> McpToolResult<UiaTreeResponse> {
        from_winr(core_uia_tree(&params))
    }

    #[rmcp::tool(name = "uia_find", description = "Find UI Automation elements within a window")]
    #[instrument(skip(self, params))]
    async fn uia_find(
        &self,
        Parameters(params): Parameters<UiaFindRequest>,
    ) -> McpToolResult<UiaFindResponse> {
        from_winr(core_uia_find(&params))
    }

    #[rmcp::tool(name = "uia_invoke", description = "Invoke a UI Automation element")]
    #[instrument(skip(self, params))]
    async fn uia_invoke(
        &self,
        Parameters(params): Parameters<UiaActionRequest>,
    ) -> McpToolResult<UiaActionResult> {
        from_winr(core_uia_invoke(&params))
    }

    #[rmcp::tool(name = "uia_set_text", description = "Set the value of a UI Automation element")]
    #[instrument(skip(self, params))]
    async fn uia_set_text(
        &self,
        Parameters(params): Parameters<UiaSetTextRequest>,
    ) -> McpToolResult<UiaActionResult> {
        from_winr(core_uia_set_text(&params))
    }
}

#[derive(Debug, Clone)]
pub struct McpToolResult<T>(Result<SuccessResponse<T>, ErrorResponse>);

impl<T> McpToolResult<T> {
    fn success(data: T) -> Self {
        Self(Ok(SuccessResponse::new(data)))
    }

    fn error(error: &WinrError) -> Self {
        Self(Err(error.to_error_response()))
    }
}

impl<T: Serialize + JsonSchema + 'static> JsonSchema for McpToolResult<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        SuccessResponse::<T>::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        SuccessResponse::<T>::json_schema(generator)
    }
}

impl<T: Serialize + JsonSchema + 'static> IntoCallToolResult for McpToolResult<T> {
    fn into_call_tool_result(self) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.0 {
            Ok(payload) => {
                let value = serde_json::to_value(payload).map_err(|error| {
                    rmcp::ErrorData::internal_error(
                        format!("failed to serialize winr MCP success payload: {error}"),
                        None,
                    )
                })?;
                Ok(CallToolResult::structured(value))
            }
            Err(payload) => {
                let value = serde_json::to_value(payload).map_err(|error| {
                    rmcp::ErrorData::internal_error(
                        format!("failed to serialize winr MCP error payload: {error}"),
                        None,
                    )
                })?;
                Ok(CallToolResult::structured_error(value))
            }
        }
    }
}

fn from_winr<T>(result: Result<T, WinrError>) -> McpToolResult<T> {
    match result {
        Ok(value) => McpToolResult::success(value),
        Err(error) => {
            debug!(code = error.code(), %error, "returning structured MCP error");
            McpToolResult::error(&error)
        }
    }
}

fn default_screenshot_path() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir()
        .join("winr")
        .join(format!("window-{millis}.png"))
}

const fn default_true() -> bool {
    true
}
