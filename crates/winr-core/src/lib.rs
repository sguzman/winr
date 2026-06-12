use std::path::Path;

use tracing::{debug, instrument, trace, warn};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
};
use windows::core::{BOOL, PWSTR};
use winr_types::{Rect, WindowInfo, WindowSelector, WinrError, WinrResult, format_hwnd};

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
}
