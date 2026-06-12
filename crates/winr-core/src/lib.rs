mod uia;

use std::{path::Path, thread, time::Duration};

use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use tracing::{debug, instrument, trace, warn};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
    CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    GetDIBits, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEINPUT, SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_HOME,
    VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB,
    VK_UP, VkKeyScanW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    MoveWindow, PostMessageW, SHOW_WINDOW_CMD, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SetCursorPos,
    SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows::core::{BOOL, PWSTR};
use winr_types::{
    InputActionResult, Rect, ScreenshotBackend, ScreenshotResult, WindowActionResult, WindowInfo,
    WindowSelector, WinrError, WinrResult, format_hwnd,
};

pub use uia::{uia_find, uia_invoke, uia_set_text, uia_tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListWindowsOptions {
    pub visible_only: bool,
}

#[instrument(skip(selector))]
pub fn list_windows(
    selector: &WindowSelector,
    options: ListWindowsOptions,
) -> WinrResult<Vec<WindowInfo>> {
    let windows = enumerate_windows()?;
    let filtered = windows
        .into_iter()
        .filter(|window| !options.visible_only || window.visible)
        .filter(|window| selector.matches(window))
        .collect::<Vec<_>>();

    debug!(
        visible_only = options.visible_only,
        matched = filtered.len(),
        "filtered windows"
    );

    Ok(filtered)
}

#[instrument]
pub fn foreground_window() -> WinrResult<WindowInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err(WinrError::Unsupported {
            message: "no foreground window is currently available".to_string(),
        });
    }

    build_window_info(hwnd)
}

#[instrument(skip(selector))]
pub fn window_info(selector: &WindowSelector) -> WinrResult<WindowInfo> {
    let windows = enumerate_windows()?;
    resolve_window_from_list(&windows, selector)
}

#[instrument(skip(selector))]
pub fn focus_window(selector: &WindowSelector) -> WinrResult<WindowInfo> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);

    debug!(hwnd = %window.hwnd, title = %window.title, "attempting foreground focus");
    let changed = unsafe { SetForegroundWindow(hwnd) }.as_bool();
    trace!(changed, "SetForegroundWindow returned");

    let foreground = unsafe { GetForegroundWindow() };
    if !changed || foreground != hwnd {
        warn!(
            requested = %window.hwnd,
            actual = format_hwnd(hwnd_value(foreground)),
            "foreground change did not succeed"
        );
        return Err(WinrError::ForegroundDenied);
    }

    build_window_info(hwnd)
}

#[instrument(skip(selector))]
pub fn restore_window(selector: &WindowSelector) -> WinrResult<WindowActionResult> {
    show_window(selector, SW_RESTORE, "restore")
}

#[instrument(skip(selector))]
pub fn minimize_window(selector: &WindowSelector) -> WinrResult<WindowActionResult> {
    show_window(selector, SW_MINIMIZE, "minimize")
}

#[instrument(skip(selector))]
pub fn maximize_window(selector: &WindowSelector) -> WinrResult<WindowActionResult> {
    show_window(selector, SW_MAXIMIZE, "maximize")
}

#[instrument(skip(selector))]
pub fn move_window(
    selector: &WindowSelector,
    x: i32,
    y: i32,
    width: Option<i32>,
    height: Option<i32>,
) -> WinrResult<WindowActionResult> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);
    let target_width = width.unwrap_or(window.rect.right - window.rect.left);
    let target_height = height.unwrap_or(window.rect.bottom - window.rect.top);

    debug!(
        hwnd = %window.hwnd,
        x,
        y,
        target_width,
        target_height,
        "moving window"
    );

    unsafe { MoveWindow(hwnd, x, y, target_width, target_height, true) }.map_err(|error| {
        WinrError::Unsupported {
            message: format!("MoveWindow failed for {}: {error}", window.hwnd),
        }
    })?;

    Ok(WindowActionResult {
        action: "move".to_string(),
        window: build_window_info(hwnd)?,
    })
}

#[instrument(skip(selector))]
pub fn resize_window(
    selector: &WindowSelector,
    width: i32,
    height: i32,
) -> WinrResult<WindowActionResult> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);

    debug!(hwnd = %window.hwnd, width, height, "resizing window");

    unsafe { MoveWindow(hwnd, window.rect.left, window.rect.top, width, height, true) }.map_err(
        |error| WinrError::Unsupported {
            message: format!("MoveWindow failed for {}: {error}", window.hwnd),
        },
    )?;

    Ok(WindowActionResult {
        action: "resize".to_string(),
        window: build_window_info(hwnd)?,
    })
}

#[instrument(skip(selector))]
pub fn close_window(selector: &WindowSelector) -> WinrResult<WindowActionResult> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);

    debug!(hwnd = %window.hwnd, title = %window.title, "posting WM_CLOSE");
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default()) }.map_err(
        |error| WinrError::Unsupported {
            message: format!("PostMessageW(WM_CLOSE) failed for {}: {error}", window.hwnd),
        },
    )?;

    Ok(WindowActionResult {
        action: "close".to_string(),
        window,
    })
}

#[instrument(skip(selector, text))]
pub fn input_text(
    selector: Option<&WindowSelector>,
    text: &str,
    focus_first: bool,
) -> WinrResult<InputActionResult> {
    let window = resolve_input_target(selector, focus_first)?;
    let inputs = unicode_inputs(text);

    debug!(
        text_len = text.encode_utf16().count(),
        focus_first,
        target = ?window.as_ref().map(|w| w.hwnd.clone()),
        "sending text input"
    );

    send_inputs(&inputs, "text")?;

    Ok(InputActionResult {
        action: "text".to_string(),
        details: text.to_string(),
        window,
    })
}

#[instrument(skip(selector, combo))]
pub fn input_keys(
    selector: Option<&WindowSelector>,
    combo: &str,
    focus_first: bool,
) -> WinrResult<InputActionResult> {
    let window = resolve_input_target(selector, focus_first)?;
    let inputs = combo_inputs(combo)?;

    debug!(
        combo,
        focus_first,
        input_count = inputs.len(),
        target = ?window.as_ref().map(|w| w.hwnd.clone()),
        "sending key combo"
    );

    send_inputs(&inputs, "keys")?;

    Ok(InputActionResult {
        action: "keys".to_string(),
        details: combo.to_string(),
        window,
    })
}

#[instrument(skip(selector, steps))]
pub fn input_sequence(
    selector: Option<&WindowSelector>,
    steps: &[String],
    focus_first: bool,
) -> WinrResult<InputActionResult> {
    let window = resolve_input_target(selector, focus_first)?;
    let mut inputs = Vec::new();

    for step in steps {
        if let Some(text) = step.strip_prefix("text:") {
            inputs.extend(unicode_inputs(text));
        } else {
            inputs.extend(combo_inputs(step)?);
        }
    }

    debug!(
        step_count = steps.len(),
        input_count = inputs.len(),
        focus_first,
        target = ?window.as_ref().map(|w| w.hwnd.clone()),
        "sending key sequence"
    );

    send_inputs(&inputs, "sequence")?;

    Ok(InputActionResult {
        action: "sequence".to_string(),
        details: steps.join(", "),
        window,
    })
}

#[instrument]
pub fn mouse_click(
    button: MouseButton,
    x: Option<i32>,
    y: Option<i32>,
) -> WinrResult<InputActionResult> {
    if let (Some(x), Some(y)) = (x, y) {
        debug!(x, y, "moving cursor before mouse click");
        unsafe { SetCursorPos(x, y) }.map_err(|error| WinrError::Unsupported {
            message: format!("SetCursorPos failed: {error}"),
        })?;
    } else if x.is_some() || y.is_some() {
        return Err(WinrError::Unsupported {
            message: "mouse click requires both x and y when either is provided".to_string(),
        });
    }

    let inputs = button.mouse_inputs();
    send_inputs(&inputs, "mouse click")?;

    Ok(InputActionResult {
        action: "mouse_click".to_string(),
        details: format!("button={}", button.as_str()),
        window: None,
    })
}

#[instrument(skip(selector))]
pub fn mouse_click_window(
    selector: &WindowSelector,
    x: i32,
    y: i32,
    button: MouseButton,
    focus_first: bool,
) -> WinrResult<InputActionResult> {
    let window = if focus_first {
        focus_window(selector)?
    } else {
        window_info(selector)?
    };
    let hwnd = parse_selector_hwnd(&window.hwnd);
    let mut point = POINT { x, y };
    let converted = unsafe { ClientToScreen(hwnd, &mut point) }.as_bool();
    if !converted {
        return Err(WinrError::Unsupported {
            message: format!("ClientToScreen failed for {}", window.hwnd),
        });
    }

    let mut original = POINT::default();
    unsafe { GetCursorPos(&mut original) }.map_err(|error| WinrError::Unsupported {
        message: format!("GetCursorPos failed: {error}"),
    })?;

    debug!(
        hwnd = %window.hwnd,
        client_x = x,
        client_y = y,
        screen_x = point.x,
        screen_y = point.y,
        button = button.as_str(),
        "clicking window-relative point"
    );

    unsafe { SetCursorPos(point.x, point.y) }.map_err(|error| WinrError::Unsupported {
        message: format!("SetCursorPos failed: {error}"),
    })?;
    let send_result = send_inputs(&button.mouse_inputs(), "mouse click-window");
    let _ = unsafe { SetCursorPos(original.x, original.y) };
    send_result?;

    Ok(InputActionResult {
        action: "mouse_click_window".to_string(),
        details: format!("button={} x={} y={}", button.as_str(), x, y),
        window: Some(window),
    })
}

#[instrument]
pub fn screenshot_desktop(out: &Path, backend: ScreenshotBackend) -> WinrResult<ScreenshotResult> {
    if matches!(backend, ScreenshotBackend::PrintWindow) {
        return Err(WinrError::Unsupported {
            message: "desktop screenshots support only the gdi or auto backend".to_string(),
        });
    }

    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    debug!(left, top, width, height, path = %out.display(), "capturing desktop screenshot");
    let image = capture_gdi(None, left, top, width, height)?;
    save_image(out, image, ScreenshotBackend::Gdi)
}

#[instrument(skip(selector))]
pub fn screenshot_window(
    selector: &WindowSelector,
    out: &Path,
    backend: ScreenshotBackend,
) -> WinrResult<ScreenshotResult> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);

    debug!(
        hwnd = %window.hwnd,
        title = %window.title,
        backend = backend.as_str(),
        path = %out.display(),
        "capturing window screenshot"
    );

    let (image, used_backend) = match backend {
        ScreenshotBackend::Auto => match capture_print_window(hwnd, &window) {
            Ok(image) => (image, ScreenshotBackend::PrintWindow),
            Err(error) => {
                warn!(%error, hwnd = %window.hwnd, "PrintWindow capture failed, falling back to gdi");
                (
                    capture_gdi(
                        Some(hwnd),
                        0,
                        0,
                        window.rect.right - window.rect.left,
                        window.rect.bottom - window.rect.top,
                    )?,
                    ScreenshotBackend::Gdi,
                )
            }
        },
        ScreenshotBackend::Gdi => (
            capture_gdi(
                Some(hwnd),
                0,
                0,
                window.rect.right - window.rect.left,
                window.rect.bottom - window.rect.top,
            )?,
            ScreenshotBackend::Gdi,
        ),
        ScreenshotBackend::PrintWindow => (
            capture_print_window(hwnd, &window)?,
            ScreenshotBackend::PrintWindow,
        ),
    };

    save_image(out, image, used_backend)
}

#[instrument]
pub fn enumerate_windows() -> WinrResult<Vec<WindowInfo>> {
    let mut handles = Vec::new();
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM((&mut handles as *mut Vec<HWND>) as isize),
        )
        .map_err(|error| WinrError::Unsupported {
            message: format!("EnumWindows failed: {error}"),
        })?;
    }

    debug!(count = handles.len(), "enumerated top-level window handles");

    handles
        .into_iter()
        .map(build_window_info)
        .collect::<WinrResult<Vec<_>>>()
}

#[instrument(skip(windows, selector))]
pub fn resolve_window_from_list(
    windows: &[WindowInfo],
    selector: &WindowSelector,
) -> WinrResult<WindowInfo> {
    let matches = windows
        .iter()
        .filter(|window| selector.matches(window))
        .cloned()
        .collect::<Vec<_>>();

    debug!(
        candidate_count = matches.len(),
        "resolved selector against window list"
    );

    match matches.len() {
        0 => Err(WinrError::WindowNotFound),
        1 => Ok(matches.into_iter().next().expect("single match present")),
        count => Err(WinrError::AmbiguousWindow { count, matches }),
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    true.into()
}

#[instrument]
fn build_window_info(hwnd: HWND) -> WinrResult<WindowInfo> {
    let title = window_text(hwnd)?;
    let class_name = class_name(hwnd)?;
    let pid = process_id(hwnd);
    let rect = window_rect(hwnd)?;
    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    let foreground = unsafe { GetForegroundWindow() } == hwnd;
    let exe = process_exe_name(pid);

    trace!(
        hwnd = %format_hwnd(hwnd_value(hwnd)),
        pid,
        visible,
        minimized,
        foreground,
        title = %title,
        class_name = %class_name,
        exe = ?exe,
        "built window info"
    );

    Ok(WindowInfo {
        hwnd: format_hwnd(hwnd_value(hwnd)),
        pid,
        title,
        class_name,
        exe,
        visible,
        minimized,
        foreground,
        rect,
    })
}

fn window_text(hwnd: HWND) -> WinrResult<String> {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buffer = vec![0u16; len as usize + 1];
    let read = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if read < 0 {
        return Err(WinrError::Unsupported {
            message: format!(
                "GetWindowTextW failed for {}",
                format_hwnd(hwnd_value(hwnd))
            ),
        });
    }

    Ok(String::from_utf16_lossy(&buffer[..read as usize]))
}

fn class_name(hwnd: HWND) -> WinrResult<String> {
    let mut buffer = vec![0u16; 256];
    let read = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if read == 0 {
        return Err(WinrError::Unsupported {
            message: format!("GetClassNameW failed for {}", format_hwnd(hwnd_value(hwnd))),
        });
    }

    Ok(String::from_utf16_lossy(&buffer[..read as usize]))
}

fn process_id(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

fn window_rect(hwnd: HWND) -> WinrResult<Rect> {
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect).map_err(|error| WinrError::Unsupported {
            message: format!(
                "GetWindowRect failed for {}: {error}",
                format_hwnd(hwnd_value(hwnd))
            ),
        })?;
    }

    Ok(Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn process_exe_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let result = query_process_image_name(handle);
    let _ = unsafe { CloseHandle(handle) };
    result
}

fn query_process_image_name(handle: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
    }

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    let file_name = Path::new(&path).file_name()?.to_str()?.to_string();
    Some(file_name)
}

fn parse_selector_hwnd(hwnd: &str) -> HWND {
    let numeric = winr_types::parse_hwnd(hwnd).unwrap_or_default();
    HWND(numeric as usize as *mut _)
}

fn hwnd_value(hwnd: HWND) -> isize {
    hwnd.0 as usize as isize
}

fn capture_gdi(
    hwnd: Option<HWND>,
    source_x: i32,
    source_y: i32,
    width: i32,
    height: i32,
) -> WinrResult<RgbaImage> {
    if width <= 0 || height <= 0 {
        return Err(WinrError::CaptureFailed {
            backend: "gdi".to_string(),
            message: "capture dimensions must be positive".to_string(),
        });
    }

    let source_dc = unsafe { GetDC(hwnd) };
    if source_dc.0.is_null() {
        return Err(WinrError::CaptureFailed {
            backend: "gdi".to_string(),
            message: "GetDC returned a null device context".to_string(),
        });
    }

    let memory_dc = unsafe { CreateCompatibleDC(Some(source_dc)) };
    if memory_dc.0.is_null() {
        unsafe { ReleaseDC(hwnd, source_dc) };
        return Err(WinrError::CaptureFailed {
            backend: "gdi".to_string(),
            message: "CreateCompatibleDC returned a null device context".to_string(),
        });
    }

    let bitmap = unsafe { CreateCompatibleBitmap(source_dc, width, height) };
    if bitmap.0.is_null() {
        cleanup_dc(hwnd, source_dc, memory_dc, None);
        return Err(WinrError::CaptureFailed {
            backend: "gdi".to_string(),
            message: "CreateCompatibleBitmap returned a null bitmap".to_string(),
        });
    }

    let previous = unsafe { SelectObject(memory_dc, HGDIOBJ(bitmap.0)) };
    let result = capture_bits_from_dc(
        "gdi",
        hwnd,
        source_dc,
        memory_dc,
        bitmap,
        previous,
        CaptureMode::BitBlt {
            source_x,
            source_y,
            width,
            height,
        },
    );
    cleanup_dc(hwnd, source_dc, memory_dc, Some(bitmap));
    result
}

fn capture_print_window(hwnd: HWND, window: &WindowInfo) -> WinrResult<RgbaImage> {
    let width = window.rect.right - window.rect.left;
    let height = window.rect.bottom - window.rect.top;
    if width <= 0 || height <= 0 {
        return Err(WinrError::CaptureFailed {
            backend: "print_window".to_string(),
            message: "window dimensions must be positive".to_string(),
        });
    }

    let source_dc = unsafe { GetDC(Some(hwnd)) };
    if source_dc.0.is_null() {
        return Err(WinrError::CaptureFailed {
            backend: "print_window".to_string(),
            message: "GetDC returned a null device context".to_string(),
        });
    }

    let memory_dc = unsafe { CreateCompatibleDC(Some(source_dc)) };
    if memory_dc.0.is_null() {
        unsafe { ReleaseDC(Some(hwnd), source_dc) };
        return Err(WinrError::CaptureFailed {
            backend: "print_window".to_string(),
            message: "CreateCompatibleDC returned a null device context".to_string(),
        });
    }

    let bitmap = unsafe { CreateCompatibleBitmap(source_dc, width, height) };
    if bitmap.0.is_null() {
        cleanup_dc(Some(hwnd), source_dc, memory_dc, None);
        return Err(WinrError::CaptureFailed {
            backend: "print_window".to_string(),
            message: "CreateCompatibleBitmap returned a null bitmap".to_string(),
        });
    }

    let previous = unsafe { SelectObject(memory_dc, HGDIOBJ(bitmap.0)) };
    let result = capture_bits_from_dc(
        "print_window",
        Some(hwnd),
        source_dc,
        memory_dc,
        bitmap,
        previous,
        CaptureMode::PrintWindow { width, height },
    );
    cleanup_dc(Some(hwnd), source_dc, memory_dc, Some(bitmap));
    result
}

enum CaptureMode {
    BitBlt {
        source_x: i32,
        source_y: i32,
        width: i32,
        height: i32,
    },
    PrintWindow {
        width: i32,
        height: i32,
    },
}

fn capture_bits_from_dc(
    backend: &str,
    hwnd: Option<HWND>,
    source_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    mode: CaptureMode,
) -> WinrResult<RgbaImage> {
    let (width, height) = match mode {
        CaptureMode::BitBlt {
            source_x,
            source_y,
            width,
            height,
        } => {
            unsafe {
                BitBlt(
                    memory_dc,
                    0,
                    0,
                    width,
                    height,
                    Some(source_dc),
                    source_x,
                    source_y,
                    SRCCOPY | CAPTUREBLT,
                )
            }
            .map_err(|error| WinrError::CaptureFailed {
                backend: backend.to_string(),
                message: format!("BitBlt failed: {error}"),
            })?;
            (width, height)
        }
        CaptureMode::PrintWindow { width, height } => {
            let captured = unsafe {
                PrintWindow(
                    hwnd.expect("window handle"),
                    memory_dc,
                    PRINT_WINDOW_FLAGS(0),
                )
            }
            .as_bool();
            if !captured {
                return Err(WinrError::CaptureFailed {
                    backend: backend.to_string(),
                    message: "PrintWindow returned false".to_string(),
                });
            }
            (width, height)
        }
    };

    unsafe {
        SelectObject(memory_dc, previous);
    }

    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let buffer_len = (width as usize) * (height as usize) * 4;
    let mut buffer = vec![0u8; buffer_len];
    let scanlines = unsafe {
        GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };

    if scanlines == 0 {
        return Err(WinrError::CaptureFailed {
            backend: backend.to_string(),
            message: "GetDIBits returned zero scanlines".to_string(),
        });
    }

    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(width as u32, height as u32, buffer).ok_or_else(
        || WinrError::CaptureFailed {
            backend: backend.to_string(),
            message: "failed to construct RGBA image buffer".to_string(),
        },
    )
}

fn cleanup_dc(hwnd: Option<HWND>, source_dc: HDC, memory_dc: HDC, bitmap: Option<HBITMAP>) {
    if let Some(bitmap) = bitmap {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
    }
    unsafe {
        let _ = DeleteDC(memory_dc);
        let _ = ReleaseDC(hwnd, source_dc);
    }
}

fn save_image(
    out: &Path,
    image: RgbaImage,
    backend: ScreenshotBackend,
) -> WinrResult<ScreenshotResult> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| WinrError::CaptureFailed {
            backend: backend.as_str().to_string(),
            message: format!(
                "failed to create parent directory {}: {error}",
                parent.display()
            ),
        })?;
    }

    let width = image.width();
    let height = image.height();
    DynamicImage::ImageRgba8(image)
        .save(out)
        .map_err(|error| WinrError::CaptureFailed {
            backend: backend.as_str().to_string(),
            message: format!("failed to save screenshot to {}: {error}", out.display()),
        })?;

    Ok(ScreenshotResult {
        path: out.display().to_string(),
        width,
        height,
        backend,
    })
}

fn show_window(
    selector: &WindowSelector,
    cmd: SHOW_WINDOW_CMD,
    action: &'static str,
) -> WinrResult<WindowActionResult> {
    let window = window_info(selector)?;
    let hwnd = parse_selector_hwnd(&window.hwnd);

    debug!(hwnd = %window.hwnd, action, "calling ShowWindow");
    let changed = unsafe { ShowWindow(hwnd, cmd) }.as_bool();
    trace!(changed, action, "ShowWindow returned");

    Ok(WindowActionResult {
        action: action.to_string(),
        window: build_window_info(hwnd)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }

    fn mouse_inputs(self) -> Vec<INPUT> {
        let (down, up) = match self {
            Self::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            Self::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            Self::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        };

        vec![mouse_input(down), mouse_input(up)]
    }
}

fn resolve_input_target(
    selector: Option<&WindowSelector>,
    focus_first: bool,
) -> WinrResult<Option<WindowInfo>> {
    match selector {
        Some(selector) => {
            let window = if focus_first {
                focus_window(selector)?
            } else {
                window_info(selector)?
            };
            if focus_first {
                thread::sleep(Duration::from_millis(40));
            }
            Ok(Some(window))
        }
        None => Ok(Some(foreground_window()?)),
    }
}

fn send_inputs(inputs: &[INPUT], action: &str) -> WinrResult<()> {
    if inputs.is_empty() {
        return Err(WinrError::Unsupported {
            message: format!("no inputs were generated for {action}"),
        });
    }

    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(WinrError::Unsupported {
            message: format!(
                "SendInput sent {sent} of {} events for {action}",
                inputs.len()
            ),
        });
    }

    trace!(action, sent, "SendInput completed");
    Ok(())
}

fn unicode_inputs(text: &str) -> Vec<INPUT> {
    let mut inputs = Vec::new();
    for code_unit in text.encode_utf16() {
        inputs.push(key_input(VIRTUAL_KEY(0), KEYEVENTF_UNICODE, code_unit));
        inputs.push(key_input(
            VIRTUAL_KEY(0),
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
            code_unit,
        ));
    }
    inputs
}

fn combo_inputs(combo: &str) -> WinrResult<Vec<INPUT>> {
    let mut modifiers = Vec::new();
    let mut key = None;

    for raw_part in combo.split('+') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some(modifier) = named_modifier(part) {
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
            continue;
        }

        if key.is_some() {
            return Err(WinrError::Unsupported {
                message: format!("combo '{combo}' contains more than one primary key"),
            });
        }

        let (vk, implicit_modifiers) = parse_key_token(part)?;
        for modifier in implicit_modifiers {
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
        }
        key = Some(vk);
    }

    let key = key.ok_or_else(|| WinrError::Unsupported {
        message: format!("combo '{combo}' does not contain a primary key"),
    })?;

    let mut inputs = Vec::new();
    for modifier in &modifiers {
        inputs.push(key_input(*modifier, KEYBD_EVENT_FLAGS(0), 0));
    }
    inputs.push(key_input(key, KEYBD_EVENT_FLAGS(0), 0));
    inputs.push(key_input(key, KEYEVENTF_KEYUP, 0));
    for modifier in modifiers.into_iter().rev() {
        inputs.push(key_input(modifier, KEYEVENTF_KEYUP, 0));
    }

    Ok(inputs)
}

fn named_modifier(token: &str) -> Option<VIRTUAL_KEY> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(VK_CONTROL),
        "alt" => Some(VK_MENU),
        "shift" => Some(VK_SHIFT),
        "win" | "meta" | "super" => Some(VK_LWIN),
        _ => None,
    }
}

fn parse_key_token(token: &str) -> WinrResult<(VIRTUAL_KEY, Vec<VIRTUAL_KEY>)> {
    if let Some(vk) = named_key(token) {
        return Ok((vk, Vec::new()));
    }

    if token.chars().count() == 1 {
        let ch = token.encode_utf16().next().expect("single char");
        let mapping = unsafe { VkKeyScanW(ch) };
        if mapping == -1 {
            return Err(WinrError::Unsupported {
                message: format!("unsupported key token '{token}'"),
            });
        }

        let vk = VIRTUAL_KEY((mapping as u16) & 0x00FF);
        let mut modifiers = Vec::new();
        let state = ((mapping as u16) >> 8) & 0x00FF;
        if state & 0x01 != 0 {
            modifiers.push(VK_SHIFT);
        }
        if state & 0x02 != 0 {
            modifiers.push(VK_CONTROL);
        }
        if state & 0x04 != 0 {
            modifiers.push(VK_MENU);
        }
        return Ok((vk, modifiers));
    }

    Err(WinrError::Unsupported {
        message: format!("unsupported key token '{token}'"),
    })
}

fn named_key(token: &str) -> Option<VIRTUAL_KEY> {
    match token.to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(VK_RETURN),
        "tab" => Some(VK_TAB),
        "esc" | "escape" => Some(VK_ESCAPE),
        "backspace" => Some(VK_BACK),
        "delete" | "del" => Some(VK_DELETE),
        "space" => Some(VK_SPACE),
        "left" => Some(VK_LEFT),
        "right" => Some(VK_RIGHT),
        "up" => Some(VK_UP),
        "down" => Some(VK_DOWN),
        "home" => Some(VK_HOME),
        "end" => Some(VK_END),
        "pageup" => Some(VK_PRIOR),
        "pagedown" => Some(VK_NEXT),
        "f1" => Some(VK_F1),
        "f2" => Some(VK_F2),
        "f3" => Some(VK_F3),
        "f4" => Some(VK_F4),
        "f5" => Some(VK_F5),
        "f6" => Some(VK_F6),
        "f7" => Some(VK_F7),
        "f8" => Some(VK_F8),
        "f9" => Some(VK_F9),
        "f10" => Some(VK_F10),
        "f11" => Some(VK_F11),
        "f12" => Some(VK_F12),
        _ => None,
    }
}

fn key_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS, scan: u16) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(hwnd: &str, title: &str) -> WindowInfo {
        WindowInfo {
            hwnd: hwnd.to_string(),
            pid: 1,
            title: title.to_string(),
            class_name: "Demo".to_string(),
            exe: Some("demo.exe".to_string()),
            visible: true,
            minimized: false,
            foreground: false,
            rect: Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
        }
    }

    #[test]
    fn resolve_window_returns_not_found() {
        let selector = WindowSelector {
            title_contains: Some("missing".to_string()),
            ..WindowSelector::default()
        };

        let error = resolve_window_from_list(&[], &selector).unwrap_err();
        assert!(matches!(error, WinrError::WindowNotFound));
    }

    #[test]
    fn resolve_window_returns_ambiguous_matches() {
        let windows = vec![
            make_window("0x0000000000000001", "same"),
            make_window("0x0000000000000002", "same"),
        ];
        let selector = WindowSelector {
            title_contains: Some("same".to_string()),
            ..WindowSelector::default()
        };

        let error = resolve_window_from_list(&windows, &selector).unwrap_err();
        match error {
            WinrError::AmbiguousWindow { count, matches } => {
                assert_eq!(count, 2);
                assert_eq!(matches.len(), 2);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn combo_parser_supports_ctrl_l() {
        let inputs = combo_inputs("ctrl+l").unwrap();
        assert_eq!(inputs.len(), 4);
    }

    #[test]
    fn combo_parser_rejects_multiple_primary_keys() {
        let error = match combo_inputs("a+b") {
            Ok(_) => panic!("expected combo parser to reject multiple primary keys"),
            Err(error) => error,
        };
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
