use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};
use tracing::{debug, error, info, instrument};
use tracing_subscriber::{EnvFilter, fmt};
use winr_core::{ListWindowsOptions, focus_window, foreground_window, list_windows, window_info};
use winr_types::{
    ErrorResponse, SuccessResponse, WindowInfo, WindowSelector, WinrError, format_hwnd, parse_hwnd,
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
enum WindowCommand {
    Info(SelectorArgs),
    Focus(SelectorArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long, help = "Return only visible windows")]
    visible: bool,
    #[command(flatten)]
    selector: SelectorArgs,
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
        },
    }
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

fn io_error(error: io::Error) -> WinrError {
    WinrError::Unsupported {
        message: format!("failed to write output: {error}"),
    }
}
