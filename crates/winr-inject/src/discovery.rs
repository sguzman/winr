use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};
use windows::core::{BOOL, PWSTR};
use winr_types::{
    AdvancedAttachableTarget, AdvancedBackendLifecycleState, AdvancedTargetDiscovery,
    AdvancedTargetRef, WindowSelector, WinrError, WinrResult, format_hwnd,
};

pub fn discover_attachable_targets(
    selector: &WindowSelector,
) -> WinrResult<AdvancedTargetDiscovery> {
    let windows = enumerate_windows()?;
    let candidates = windows
        .into_iter()
        .filter(|window| selector.matches_attachable_target(window))
        .collect::<Vec<_>>();

    Ok(AdvancedTargetDiscovery {
        selector: selector.clone(),
        candidate_count: candidates.len(),
        candidates,
    })
}

pub fn resolve_attachable_target(
    discovery: &AdvancedTargetDiscovery,
) -> WinrResult<AdvancedAttachableTarget> {
    match discovery.candidates.as_slice() {
        [] => Err(WinrError::WindowNotFound),
        [single] => Ok(single.clone()),
        many => Err(WinrError::Unsupported {
            message: format!(
                "{} attachable targets matched the advanced backend selector",
                many.len()
            ),
        }),
    }
}

fn enumerate_windows() -> WinrResult<Vec<AdvancedAttachableTarget>> {
    let mut handles = Vec::new();
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM((&mut handles as *mut Vec<HWND>) as isize),
        )
        .map_err(|error| WinrError::Unsupported {
            message: format!("EnumWindows failed during advanced target discovery: {error}"),
        })?;
    }

    handles
        .into_iter()
        .map(build_attachable_target)
        .collect::<WinrResult<Vec<_>>>()
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    true.into()
}

fn build_attachable_target(hwnd: HWND) -> WinrResult<AdvancedAttachableTarget> {
    let title = window_text(hwnd)?;
    let class_name = class_name(hwnd)?;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }

    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    let foreground = unsafe { GetForegroundWindow() } == hwnd;
    let exe = process_exe_name(pid);
    let mut notes = Vec::new();

    let lifecycle_state = if minimized {
        notes.push("window is minimized".to_string());
        AdvancedBackendLifecycleState::Discovered
    } else if visible {
        AdvancedBackendLifecycleState::Attachable
    } else {
        notes.push("window is not currently visible".to_string());
        AdvancedBackendLifecycleState::Discovered
    };

    if let Ok(rect) = window_rect(hwnd)
        && (rect.right - rect.left <= 0 || rect.bottom - rect.top <= 0)
    {
        notes.push("window bounds are empty".to_string());
    }

    Ok(AdvancedAttachableTarget {
        target: AdvancedTargetRef {
            hwnd: Some(format_hwnd(hwnd.0 as isize)),
            pid: Some(pid),
            exe: exe.clone(),
            window_class: Some(class_name.clone()),
            title_hint: Some(title.clone()),
        },
        lifecycle_state,
        title,
        class_name,
        exe,
        visible,
        minimized,
        foreground,
        notes,
    })
}

fn window_text(hwnd: HWND) -> WinrResult<String> {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buffer = vec![0u16; len as usize + 1];
    let read = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if read < 0 {
        return Err(WinrError::Unsupported {
            message: format!(
                "GetWindowTextW failed during advanced target discovery for {}",
                format_hwnd(hwnd.0 as isize)
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
            message: format!(
                "GetClassNameW failed during advanced target discovery for {}",
                format_hwnd(hwnd.0 as isize)
            ),
        });
    }
    Ok(String::from_utf16_lossy(&buffer[..read as usize]))
}

fn process_exe_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()? };
    let result = process_image_name_from_handle(handle);
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

fn process_image_name_from_handle(handle: HANDLE) -> Option<String> {
    let mut size = 260u32;
    let mut buffer = vec![0u16; size as usize];
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
    }
    let full = String::from_utf16_lossy(&buffer[..size as usize]);
    std::path::Path::new(&full)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn window_rect(hwnd: HWND) -> WinrResult<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|error| WinrError::Unsupported {
        message: format!(
            "GetWindowRect failed during advanced target discovery for {}: {error}",
            format_hwnd(hwnd.0 as isize)
        ),
    })?;
    Ok(rect)
}

trait AdvancedTargetSelectorMatch {
    fn matches_attachable_target(&self, target: &AdvancedAttachableTarget) -> bool;
}

impl AdvancedTargetSelectorMatch for WindowSelector {
    fn matches_attachable_target(&self, target: &AdvancedAttachableTarget) -> bool {
        self.hwnd.as_ref().is_none_or(|hwnd| {
            target
                .target
                .hwnd
                .as_ref()
                .is_some_and(|value| value.eq_ignore_ascii_case(hwnd))
        }) && self.pid.is_none_or(|pid| target.target.pid == Some(pid))
            && self
                .title_contains
                .as_ref()
                .is_none_or(|title| target.title.to_lowercase().contains(&title.to_lowercase()))
            && self
                .class_name
                .as_ref()
                .is_none_or(|class_name| target.class_name.eq_ignore_ascii_case(class_name))
            && self.exe.as_ref().is_none_or(|exe| {
                target
                    .exe
                    .as_ref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(exe))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target(hwnd: &str, title: &str) -> AdvancedAttachableTarget {
        AdvancedAttachableTarget {
            target: AdvancedTargetRef {
                hwnd: Some(hwnd.to_string()),
                pid: Some(42),
                exe: Some("RobloxPlayerBeta.exe".to_string()),
                window_class: Some("WINDOWSCLIENT".to_string()),
                title_hint: Some(title.to_string()),
            },
            lifecycle_state: AdvancedBackendLifecycleState::Attachable,
            title: title.to_string(),
            class_name: "WINDOWSCLIENT".to_string(),
            exe: Some("RobloxPlayerBeta.exe".to_string()),
            visible: true,
            minimized: false,
            foreground: false,
            notes: Vec::new(),
        }
    }

    #[test]
    fn selector_matches_attachable_target() {
        let selector = WindowSelector {
            title_contains: Some("roblox".to_string()),
            exe: Some("robloxplayerbeta.exe".to_string()),
            ..WindowSelector::default()
        };

        assert!(selector.matches_attachable_target(&sample_target("0x0000000000001234", "Roblox")));
    }

    #[test]
    fn resolve_attachable_target_rejects_multiple_matches() {
        let discovery = AdvancedTargetDiscovery {
            selector: WindowSelector::default(),
            candidate_count: 2,
            candidates: vec![
                sample_target("0x0000000000001234", "Roblox 1"),
                sample_target("0x0000000000005678", "Roblox 2"),
            ],
        };

        let error = resolve_attachable_target(&discovery).unwrap_err();
        assert!(matches!(error, WinrError::Unsupported { .. }));
    }
}
