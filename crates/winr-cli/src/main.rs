use std::{
    io::{self, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use tracing::{debug, error, info, instrument};
use tracing_subscriber::{EnvFilter, fmt};
use winr_core::{
    ListWindowsOptions, MouseButton, ProfileRunEvent, ProfileRunOptions, close_window,
    focus_window, foreground_window, input_keys, input_sequence, input_text, list_windows,
    load_profile, maximize_window, minimize_window, mouse_click, mouse_click_window, move_window,
    resize_window, restore_window, run_profile, screenshot_desktop, screenshot_window, uia_find,
    uia_invoke, uia_set_text, uia_tree, window_info,
};
use winr_types::{
    ErrorResponse, InputActionResult, InputMode, ProfileRunResult, ScreenshotBackend,
    ScreenshotResult, SuccessResponse, UiaActionRequest, UiaActionResult, UiaElementInfo,
    UiaFindRequest, UiaFindResponse, UiaSelector, UiaSetTextRequest, UiaTreeMode,
    UiaTreeRequest, UiaTreeResponse, WindowActionResult, WindowInfo, WindowSelector, WinrError,
    format_hwnd, parse_hwnd,
};

fn main() {
    init_tracing();
    let cli = Cli::parse();
    debug!(?cli, "parsed CLI arguments");
    let json = cli.json;

    if let Err(error) = run(cli) {
        emit_error(json, &error);
        error!(code = error.code(), %error, "command failed");
        std::process::exit(1);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .without_time()
        .with_writer(io::stderr)
        .init();
}

#[derive(Debug, Parser)]
#[command(
    name = "winr",
    version,
    about = "Windows 11 window inspection and focus CLI"
)]
struct Cli {
    #[arg(long, global = true, help = "Emit stable JSON output")]
    json: bool,
    #[command(subcommand)]
    command: RootCommand,
}

#[derive(Debug, Subcommand)]
enum RootCommand {
    Windows {
        #[command(subcommand)]
        command: WindowsCommand,
    },
    Input {
        #[command(subcommand)]
        command: InputCommand,
    },
    Mouse {
        #[command(subcommand)]
        command: MouseCommand,
    },
    Screenshot {
        #[command(subcommand)]
        command: ScreenshotCommand,
    },
    Uia {
        #[command(subcommand)]
        command: UiaCommand,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    Window {
        #[command(subcommand)]
        command: WindowCommand,
    },
}

#[derive(Debug, Subcommand)]
enum WindowsCommand {
    List(ListArgs),
    Foreground,
}

#[derive(Debug, Subcommand)]
enum ScreenshotCommand {
    Desktop(DesktopScreenshotArgs),
    Window(WindowScreenshotArgs),
}

#[derive(Debug, Subcommand)]
enum InputCommand {
    Text(TextInputArgs),
    Keys(KeysInputArgs),
    Sequence(SequenceInputArgs),
}

#[derive(Debug, Subcommand)]
enum MouseCommand {
    Click(MouseClickArgs),
    ClickWindow(MouseClickWindowArgs),
}

#[derive(Debug, Subcommand)]
enum WindowCommand {
    Info(SelectorArgs),
    Focus(SelectorArgs),
    Restore(SelectorArgs),
    Minimize(SelectorArgs),
    Maximize(SelectorArgs),
    Move(MoveArgs),
    Resize(ResizeArgs),
    Close(CloseArgs),
}

#[derive(Debug, Subcommand)]
enum UiaCommand {
    Tree(UiaTreeArgs),
    Find(UiaFindArgs),
    Invoke(UiaActionArgs),
    SetText(UiaSetTextArgs),
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    Serve,
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    Run(ProfileRunArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long, help = "Return only visible windows")]
    visible: bool,
    #[command(flatten)]
    selector: SelectorArgs,
}

#[derive(Debug, Args)]
struct MoveArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long)]
    x: i32,
    #[arg(long)]
    y: i32,
    #[arg(long)]
    width: Option<i32>,
    #[arg(long)]
    height: Option<i32>,
}

#[derive(Debug, Args)]
struct ResizeArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long)]
    width: i32,
    #[arg(long)]
    height: i32,
}

#[derive(Debug, Args)]
struct CloseArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(
        long,
        help = "Acknowledge the configured close confirmation requirement"
    )]
    force: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScreenshotBackendArg {
    Auto,
    Gdi,
    PrintWindow,
}

impl From<ScreenshotBackendArg> for ScreenshotBackend {
    fn from(value: ScreenshotBackendArg) -> Self {
        match value {
            ScreenshotBackendArg::Auto => ScreenshotBackend::Auto,
            ScreenshotBackendArg::Gdi => ScreenshotBackend::Gdi,
            ScreenshotBackendArg::PrintWindow => ScreenshotBackend::PrintWindow,
        }
    }
}

#[derive(Debug, Args)]
struct DesktopScreenshotArgs {
    #[arg(long)]
    out: PathBuf,
    #[arg(long, value_enum, default_value = "auto")]
    backend: ScreenshotBackendArg,
}

#[derive(Debug, Args)]
struct WindowScreenshotArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, value_enum, default_value = "auto")]
    backend: ScreenshotBackendArg,
}

#[derive(Debug, Args)]
struct ProfileRunArgs {
    path: PathBuf,
    #[arg(long, help = "Stop waiting after this many milliseconds if the target never appears")]
    wait_timeout_ms: Option<u64>,
    #[arg(long, default_value_t = 250, help = "Polling interval while waiting for the target window")]
    poll_interval_ms: u64,
    #[arg(long, help = "Stop automatically after this many clicks")]
    max_clicks: Option<u64>,
    #[arg(long, default_value_t = false, help = "Try to focus the matched target window before starting")]
    focus_target: bool,
    #[arg(long, default_value_t = 0, help = "Wait this many milliseconds after target acquisition before starting clicks")]
    arm_delay_ms: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InputModeArg {
    Foreground,
    Uia,
    Message,
}

impl From<InputModeArg> for InputMode {
    fn from(value: InputModeArg) -> Self {
        match value {
            InputModeArg::Foreground => InputMode::Foreground,
            InputModeArg::Uia => InputMode::Uia,
            InputModeArg::Message => InputMode::Message,
        }
    }
}

#[derive(Debug, Args)]
struct TextInputArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    focus_first: bool,
    #[arg(long, value_enum, default_value = "foreground")]
    input_mode: InputModeArg,
    text: String,
}

#[derive(Debug, Args)]
struct KeysInputArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    focus_first: bool,
    #[arg(long, value_enum, default_value = "foreground")]
    input_mode: InputModeArg,
    #[arg(long)]
    combo: String,
}

#[derive(Debug, Args)]
struct SequenceInputArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    focus_first: bool,
    #[arg(long, value_enum, default_value = "foreground")]
    input_mode: InputModeArg,
    #[arg(long = "step", required = true)]
    steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MouseButtonArg {
    Left,
    Right,
    Middle,
}

impl From<MouseButtonArg> for MouseButton {
    fn from(value: MouseButtonArg) -> Self {
        match value {
            MouseButtonArg::Left => MouseButton::Left,
            MouseButtonArg::Right => MouseButton::Right,
            MouseButtonArg::Middle => MouseButton::Middle,
        }
    }
}

#[derive(Debug, Args)]
struct MouseClickArgs {
    #[arg(long, value_enum, default_value = "left")]
    button: MouseButtonArg,
    #[arg(long)]
    x: Option<i32>,
    #[arg(long)]
    y: Option<i32>,
}

#[derive(Debug, Args)]
struct MouseClickWindowArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long)]
    x: i32,
    #[arg(long)]
    y: i32,
    #[arg(long, value_enum, default_value = "left")]
    button: MouseButtonArg,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    focus_first: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum UiaTreeModeArg {
    Control,
    Raw,
}

impl From<UiaTreeModeArg> for UiaTreeMode {
    fn from(value: UiaTreeModeArg) -> Self {
        match value {
            UiaTreeModeArg::Control => UiaTreeMode::Control,
            UiaTreeModeArg::Raw => UiaTreeMode::Raw,
        }
    }
}

#[derive(Debug, Args, Clone, Default)]
struct UiaSelectorArgs {
    #[arg(long = "automation-id")]
    automation_id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long = "uia-class")]
    class_name: Option<String>,
    #[arg(long = "control-kind")]
    localized_control_type: Option<String>,
    #[arg(long = "control-type")]
    control_type: Option<i32>,
    #[arg(long)]
    enabled: Option<bool>,
}

impl UiaSelectorArgs {
    fn into_selector(self) -> UiaSelector {
        UiaSelector {
            automation_id: self.automation_id,
            name: self.name,
            class_name: self.class_name,
            localized_control_type: self.localized_control_type,
            control_type: self.control_type,
            enabled: self.enabled,
        }
    }
}

#[derive(Debug, Args)]
struct UiaTreeArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long, value_enum, default_value = "control")]
    mode: UiaTreeModeArg,
    #[arg(long, default_value_t = 4)]
    max_depth: u32,
}

#[derive(Debug, Args)]
struct UiaFindArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[command(flatten)]
    element: UiaSelectorArgs,
    #[arg(long, value_enum, default_value = "control")]
    mode: UiaTreeModeArg,
}

#[derive(Debug, Args)]
struct UiaActionArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[command(flatten)]
    element: UiaSelectorArgs,
}

#[derive(Debug, Args)]
struct UiaSetTextArgs {
    #[command(flatten)]
    selector: SelectorArgs,
    #[command(flatten)]
    element: UiaSelectorArgs,
    #[arg(long)]
    text: String,
}

#[derive(Debug, Args, Clone, Default)]
struct SelectorArgs {
    #[arg(long, value_parser = parse_hwnd_arg, help = "Window handle in hexadecimal")]
    hwnd: Option<String>,
    #[arg(long, help = "Process identifier")]
    pid: Option<u32>,
    #[arg(long = "title", help = "Case-insensitive window title substring")]
    title_contains: Option<String>,
    #[arg(long = "class", help = "Window class name")]
    class_name: Option<String>,
    #[arg(long, help = "Executable name like notepad.exe")]
    exe: Option<String>,
}

impl SelectorArgs {
    fn into_selector(self) -> WindowSelector {
        WindowSelector {
            hwnd: self.hwnd,
            pid: self.pid,
            title_contains: self.title_contains,
            class_name: self.class_name,
            exe: self.exe,
        }
    }
}

#[instrument(skip(cli))]
fn run(cli: Cli) -> Result<(), WinrError> {
    info!(json = cli.json, "executing CLI command");

    match cli.command {
        RootCommand::Windows { command } => match command {
            WindowsCommand::List(args) => {
                let selector = args.selector.into_selector();
                let windows = list_windows(
                    &selector,
                    ListWindowsOptions {
                        visible_only: args.visible,
                    },
                )?;
                emit(cli.json, &windows)
            }
            WindowsCommand::Foreground => {
                let window = foreground_window()?;
                emit(cli.json, &window)
            }
        },
        RootCommand::Input { command } => match command {
            InputCommand::Text(args) => {
                let selector = args.selector.into_selector();
                let result = input_text(
                    selector.has_criteria().then_some(&selector),
                    &args.text,
                    args.focus_first,
                    args.input_mode.into(),
                )?;
                emit(cli.json, &result)
            }
            InputCommand::Keys(args) => {
                let selector = args.selector.into_selector();
                let result = input_keys(
                    selector.has_criteria().then_some(&selector),
                    &args.combo,
                    args.focus_first,
                    args.input_mode.into(),
                )?;
                emit(cli.json, &result)
            }
            InputCommand::Sequence(args) => {
                let selector = args.selector.into_selector();
                let result = input_sequence(
                    selector.has_criteria().then_some(&selector),
                    &args.steps,
                    args.focus_first,
                    args.input_mode.into(),
                )?;
                emit(cli.json, &result)
            }
        },
        RootCommand::Mouse { command } => match command {
            MouseCommand::Click(args) => {
                let result = mouse_click(args.button.into(), args.x, args.y)?;
                emit(cli.json, &result)
            }
            MouseCommand::ClickWindow(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = mouse_click_window(
                    &selector,
                    args.x,
                    args.y,
                    args.button.into(),
                    args.focus_first,
                )?;
                emit(cli.json, &result)
            }
        },
        RootCommand::Screenshot { command } => match command {
            ScreenshotCommand::Desktop(args) => {
                let result = screenshot_desktop(&args.out, args.backend.into())?;
                emit(cli.json, &result)
            }
            ScreenshotCommand::Window(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = screenshot_window(&selector, &args.out, args.backend.into())?;
                emit(cli.json, &result)
            }
        },
        RootCommand::Uia { command } => match command {
            UiaCommand::Tree(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = uia_tree(&UiaTreeRequest {
                    window: selector,
                    mode: Some(args.mode.into()),
                    max_depth: Some(args.max_depth),
                })?;
                emit(cli.json, &result)
            }
            UiaCommand::Find(args) => {
                let window = require_selector(args.selector.into_selector())?;
                let element = require_uia_selector(args.element.into_selector())?;
                let result = uia_find(&UiaFindRequest {
                    window,
                    element,
                    mode: Some(args.mode.into()),
                })?;
                emit(cli.json, &result)
            }
            UiaCommand::Invoke(args) => {
                let window = require_selector(args.selector.into_selector())?;
                let element = require_uia_selector(args.element.into_selector())?;
                let result = uia_invoke(&UiaActionRequest { window, element })?;
                emit(cli.json, &result)
            }
            UiaCommand::SetText(args) => {
                let window = require_selector(args.selector.into_selector())?;
                let element = require_uia_selector(args.element.into_selector())?;
                let result = uia_set_text(&UiaSetTextRequest {
                    window,
                    element,
                    text: args.text,
                })?;
                emit(cli.json, &result)
            }
        },
        RootCommand::Mcp { command } => match command {
            McpCommand::Serve => {
                winr_mcp::serve_stdio_blocking().map_err(|error| WinrError::Unsupported {
                    message: format!("failed to serve MCP over stdio: {error}"),
                })
            }
        },
        RootCommand::Profile { command } => match command {
            ProfileCommand::Run(args) => {
                let profile = load_profile(&args.path)?;
                let options = ProfileRunOptions {
                    wait_timeout: args.wait_timeout_ms.map(Duration::from_millis),
                    poll_interval: Duration::from_millis(args.poll_interval_ms),
                    max_triggers: args.max_clicks,
                    focus_target: args.focus_target,
                    arm_delay: Duration::from_millis(args.arm_delay_ms),
                };
                let result = run_profile_with_console(&profile, options, cli.json)?;
                emit(cli.json, &result)
            }
        },
        RootCommand::Window { command } => match command {
            WindowCommand::Info(args) => {
                let selector = require_selector(args.into_selector())?;
                let window = window_info(&selector)?;
                emit(cli.json, &window)
            }
            WindowCommand::Focus(args) => {
                let selector = require_selector(args.into_selector())?;
                let window = focus_window(&selector)?;
                emit(cli.json, &window)
            }
            WindowCommand::Restore(args) => {
                let selector = require_selector(args.into_selector())?;
                let result = restore_window(&selector)?;
                emit(cli.json, &result)
            }
            WindowCommand::Minimize(args) => {
                let selector = require_selector(args.into_selector())?;
                let result = minimize_window(&selector)?;
                emit(cli.json, &result)
            }
            WindowCommand::Maximize(args) => {
                let selector = require_selector(args.into_selector())?;
                let result = maximize_window(&selector)?;
                emit(cli.json, &result)
            }
            WindowCommand::Move(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = move_window(&selector, args.x, args.y, args.width, args.height)?;
                emit(cli.json, &result)
            }
            WindowCommand::Resize(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = resize_window(&selector, args.width, args.height)?;
                emit(cli.json, &result)
            }
            WindowCommand::Close(args) => {
                let selector = require_selector(args.selector.into_selector())?;
                let result = close_window(&selector, args.force)?;
                emit(cli.json, &result)
            }
        },
    }
}

fn run_profile_with_console(
    profile: &winr_types::ProfileConfig,
    options: ProfileRunOptions,
    json: bool,
) -> Result<ProfileRunResult, WinrError> {
    let stop_requested = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&stop_requested);
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::SeqCst);
    })
    .map_err(|error| WinrError::Unsupported {
        message: format!("failed to install Ctrl+C handler: {error}"),
    })?;

    let mut stderr = io::stderr().lock();
    let mut last_count = 0_u64;
    let mut rendered_progress = false;
    let mut announced_wait = false;
    let mut acquired_target = false;

    let result = run_profile(
        profile,
        options,
        |event| match event {
            ProfileRunEvent::WaitingForTarget { selector } => {
                if json || announced_wait {
                    return;
                }
                let _ = writeln!(
                    stderr,
                    "waiting for target window: title={:?} exe={:?} class={:?}",
                    selector.title_contains, selector.exe, selector.class_name
                );
                announced_wait = true;
            }
            ProfileRunEvent::TargetAcquired { window } => {
                if json || acquired_target {
                    return;
                }
                let _ = writeln!(stderr, "target acquired: {} {}", window.hwnd, window.title);
                acquired_target = true;
            }
            ProfileRunEvent::DetectorMatched { .. } => {}
            ProfileRunEvent::TriggerFired { count } => {
                last_count = count;
                if json {
                    return;
                }
                let _ = write!(stderr, "\rautoclicks fired: {count}");
                let _ = stderr.flush();
                rendered_progress = true;
            }
            ProfileRunEvent::Stopped { count, reason } => {
                last_count = count;
                if json {
                    return;
                }
                if rendered_progress {
                    let _ = writeln!(stderr, "\rautoclicks fired: {count}");
                }
                let _ = writeln!(stderr, "profile stopped: {reason}");
            }
        },
        || stop_requested.load(Ordering::SeqCst),
    );

    if !json && rendered_progress && result.is_err() {
        let _ = writeln!(stderr, "\rautoclicks fired: {last_count}");
    }

    result
}

fn emit_error(json: bool, error: &WinrError) {
    if json {
        let payload: ErrorResponse = error.to_error_response();
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        if serde_json::to_writer_pretty(&mut handle, &payload).is_ok() {
            let _ = writeln!(handle);
            return;
        }
    }

    let _ = writeln!(io::stderr(), "{}: {}", error.code(), error);
}

fn require_selector(selector: WindowSelector) -> Result<WindowSelector, WinrError> {
    if selector.has_criteria() {
        Ok(selector)
    } else {
        Err(WinrError::Unsupported {
            message: "at least one selector flag is required".to_string(),
        })
    }
}

fn require_uia_selector(selector: UiaSelector) -> Result<UiaSelector, WinrError> {
    if selector.has_criteria() {
        Ok(selector)
    } else {
        Err(WinrError::Unsupported {
            message: "at least one UI Automation selector flag is required".to_string(),
        })
    }
}

fn parse_hwnd_arg(value: &str) -> Result<String, String> {
    parse_hwnd(value).map(format_hwnd)
}

fn emit<T>(json: bool, value: &T) -> Result<(), WinrError>
where
    T: serde::Serialize + HumanOutput,
{
    if json {
        let payload = SuccessResponse::new(value);
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        serde_json::to_writer_pretty(&mut handle, &payload).map_err(|error| {
            WinrError::Unsupported {
                message: format!("failed to serialize JSON output: {error}"),
            }
        })?;
        writeln!(handle).map_err(|error| WinrError::Unsupported {
            message: format!("failed to flush JSON output: {error}"),
        })?;
        return Ok(());
    }

    value.write_human(&mut io::stdout())
}

trait HumanOutput {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError>;
}

impl HumanOutput for WindowInfo {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "hwnd: {}", self.hwnd).map_err(io_error)?;
        writeln!(writer, "pid: {}", self.pid).map_err(io_error)?;
        writeln!(writer, "title: {}", self.title).map_err(io_error)?;
        writeln!(writer, "class: {}", self.class_name).map_err(io_error)?;
        writeln!(
            writer,
            "exe: {}",
            self.exe.as_deref().unwrap_or("<unknown>")
        )
        .map_err(io_error)?;
        writeln!(writer, "visible: {}", self.visible).map_err(io_error)?;
        writeln!(writer, "minimized: {}", self.minimized).map_err(io_error)?;
        writeln!(writer, "foreground: {}", self.foreground).map_err(io_error)?;
        writeln!(
            writer,
            "rect: left={} top={} right={} bottom={}",
            self.rect.left, self.rect.top, self.rect.right, self.rect.bottom
        )
        .map_err(io_error)
    }
}

impl HumanOutput for Vec<WindowInfo> {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        for window in self {
            writeln!(
                writer,
                "{} pid={} visible={} minimized={} foreground={} class=\"{}\" exe=\"{}\" title=\"{}\"",
                window.hwnd,
                window.pid,
                window.visible,
                window.minimized,
                window.foreground,
                window.class_name,
                window.exe.as_deref().unwrap_or("<unknown>"),
                window.title.replace('"', "'")
            )
            .map_err(io_error)?;
        }

        Ok(())
    }
}

impl HumanOutput for WindowActionResult {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "action: {}", self.action).map_err(io_error)?;
        self.window.write_human(writer)
    }
}

impl HumanOutput for ScreenshotResult {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "path: {}", self.path).map_err(io_error)?;
        writeln!(writer, "width: {}", self.width).map_err(io_error)?;
        writeln!(writer, "height: {}", self.height).map_err(io_error)?;
        writeln!(writer, "backend: {}", self.backend.as_str()).map_err(io_error)
    }
}

impl HumanOutput for InputActionResult {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "action: {}", self.action).map_err(io_error)?;
        writeln!(writer, "mode: {}", self.mode.as_str()).map_err(io_error)?;
        writeln!(writer, "details: {}", self.details).map_err(io_error)?;
        if let Some(window) = &self.window {
            window.write_human(writer)?;
        }
        Ok(())
    }
}

impl HumanOutput for ProfileRunResult {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "profile_id: {}", self.profile_id).map_err(io_error)?;
        writeln!(writer, "profile_name: {}", self.profile_name).map_err(io_error)?;
        writeln!(writer, "clicks_fired: {}", self.clicks_fired).map_err(io_error)?;
        self.target_window.write_human(writer)
    }
}

impl HumanOutput for UiaTreeResponse {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "window: {}", self.window.hwnd).map_err(io_error)?;
        writeln!(
            writer,
            "mode: {}",
            match self.mode {
                UiaTreeMode::Control => "control",
                UiaTreeMode::Raw => "raw",
            }
        )
        .map_err(io_error)?;
        write_uia_node(writer, &self.root, 0)
    }
}

impl HumanOutput for UiaFindResponse {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "window: {}", self.window.hwnd).map_err(io_error)?;
        for node in &self.matches {
            write_uia_node(writer, node, 0)?;
        }
        Ok(())
    }
}

impl HumanOutput for UiaActionResult {
    fn write_human<W: Write>(&self, writer: &mut W) -> Result<(), WinrError> {
        writeln!(writer, "action: {}", self.action).map_err(io_error)?;
        writeln!(writer, "window: {}", self.window.hwnd).map_err(io_error)?;
        if let Some(details) = &self.details {
            writeln!(writer, "details: {}", details).map_err(io_error)?;
        }
        write_uia_node(writer, &self.element, 0)
    }
}

fn write_uia_node<W: Write>(
    writer: &mut W,
    node: &UiaElementInfo,
    depth: usize,
) -> Result<(), WinrError> {
    let indent = "  ".repeat(depth);
    writeln!(
        writer,
        "{indent}- name=\"{}\" automation_id=\"{}\" class=\"{}\" kind=\"{}\" control_type={} enabled={} hwnd={}{}",
        node.name.as_deref().unwrap_or(""),
        node.automation_id.as_deref().unwrap_or(""),
        node.class_name.as_deref().unwrap_or(""),
        node.localized_control_type.as_deref().unwrap_or(""),
        node.control_type
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<unknown>".to_string()),
        node.enabled
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<unknown>".to_string()),
        node.hwnd.as_deref().unwrap_or(""),
        match node.rect {
            Some(rect) => format!(
                " rect=({}, {}, {}, {})",
                rect.left, rect.top, rect.right, rect.bottom
            ),
            None => String::new(),
        }
    )
    .map_err(io_error)?;

    for child in &node.children {
        write_uia_node(writer, child, depth + 1)?;
    }

    Ok(())
}

fn io_error(error: io::Error) -> WinrError {
    WinrError::Unsupported {
        message: format!("failed to write output: {error}"),
    }
}
