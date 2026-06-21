use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use windows::Win32::System::Threading::{
    IsWow64Process, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};
use windows::core::{BOOL, PWSTR};
use winr_types::{
    AdvancedAttachableTarget, AdvancedBackendLifecycleState, AdvancedIntegrityLevel,
    AdvancedProcessArchitecture, AdvancedProcessMetadata, AdvancedTargetDiscovery,
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
    let mut candidates = candidates;
    candidates.sort_by(|left, right| {
        right
            .foreground
            .cmp(&left.foreground)
            .then_with(|| right.visible.cmp(&left.visible))
            .then_with(|| left.minimized.cmp(&right.minimized))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.target.hwnd.cmp(&right.target.hwnd))
    });

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
    let executable_path = process_image_path(pid);
    let exe = executable_path
        .as_deref()
        .and_then(|full| std::path::Path::new(full).file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let process = process_metadata(pid, hwnd, executable_path.clone());
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
        process,
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

fn process_image_path(pid: u32) -> Option<String> {
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
    Some(String::from_utf16_lossy(&buffer[..size as usize]))
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

fn process_metadata(
    pid: u32,
    hwnd: HWND,
    executable_path: Option<String>,
) -> AdvancedProcessMetadata {
    AdvancedProcessMetadata {
        architecture: process_architecture(pid),
        integrity_level: process_integrity_level(pid),
        loaded_modules: loaded_module_names(pid),
        executable_path,
        likely_rendering_window: Some(format_hwnd(hwnd.0 as isize)),
    }
}

fn process_architecture(pid: u32) -> AdvancedProcessArchitecture {
    if pid == 0 {
        return AdvancedProcessArchitecture::Unknown;
    }

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => handle,
        Err(_) => return AdvancedProcessArchitecture::Unknown,
    };

    let mut wow64 = BOOL::default();
    let result = unsafe { IsWow64Process(handle, &mut wow64) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if result.is_err() {
        return AdvancedProcessArchitecture::Unknown;
    }

    #[cfg(target_pointer_width = "64")]
    {
        if wow64.as_bool() {
            AdvancedProcessArchitecture::X86
        } else {
            AdvancedProcessArchitecture::X64
        }
    }

    #[cfg(not(target_pointer_width = "64"))]
    {
        if wow64.as_bool() {
            AdvancedProcessArchitecture::Unknown
        } else {
            AdvancedProcessArchitecture::X86
        }
    }
}

fn process_integrity_level(pid: u32) -> AdvancedIntegrityLevel {
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => handle,
        Err(_) => return AdvancedIntegrityLevel::Unknown,
    };

    let mut token = HANDLE::default();
    let opened = unsafe { OpenProcessToken(handle, TOKEN_QUERY, &mut token) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if opened.is_err() {
        return AdvancedIntegrityLevel::Unknown;
    }

    let result = token_integrity_level(token);
    unsafe {
        let _ = CloseHandle(token);
    }
    result
}

fn token_integrity_level(token: HANDLE) -> AdvancedIntegrityLevel {
    let mut len = 0u32;
    let _ = unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len) };
    if len == 0 {
        return AdvancedIntegrityLevel::Unknown;
    }

    let mut buffer = vec![0u8; len as usize];
    let info = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr() as *mut c_void),
            len,
            &mut len,
        )
    };
    if info.is_err() {
        return AdvancedIntegrityLevel::Unknown;
    }

    let label = unsafe { &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
    let sid = label.Label.Sid;
    if !unsafe { IsValidSid(sid) }.as_bool() {
        return AdvancedIntegrityLevel::Unknown;
    }

    let count = unsafe { *GetSidSubAuthorityCount(sid) } as u32;
    if count == 0 {
        return AdvancedIntegrityLevel::Unknown;
    }

    let rid = unsafe { *GetSidSubAuthority(sid, count - 1) };
    match rid {
        0x0000..=0x0FFF => AdvancedIntegrityLevel::Untrusted,
        0x1000..=0x1FFF => AdvancedIntegrityLevel::Low,
        0x2000..=0x2FFF => AdvancedIntegrityLevel::Medium,
        0x3000..=0x3FFF => AdvancedIntegrityLevel::High,
        0x4000..=0x4FFF => AdvancedIntegrityLevel::System,
        0x5000..=u32::MAX => AdvancedIntegrityLevel::Protected,
    }
}

fn loaded_module_names(pid: u32) -> Vec<String> {
    if pid == 0 {
        return Vec::new();
    }

    let snapshot =
        match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) } {
            Ok(handle) => handle,
            Err(_) => return Vec::new(),
        };

    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    let mut modules = Vec::new();

    if unsafe { Module32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let name_len = entry
                .szModule
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(entry.szModule.len());
            modules.push(String::from_utf16_lossy(&entry.szModule[..name_len]));

            if unsafe { Module32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }
    modules
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
            process: AdvancedProcessMetadata {
                architecture: AdvancedProcessArchitecture::X64,
                integrity_level: AdvancedIntegrityLevel::Medium,
                loaded_modules: vec!["RobloxPlayerBeta.exe".to_string()],
                executable_path: Some("C:\\RobloxPlayerBeta.exe".to_string()),
                likely_rendering_window: Some(hwnd.to_string()),
            },
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
