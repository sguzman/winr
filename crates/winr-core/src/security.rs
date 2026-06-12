use std::ffi::c_void;

use tracing::{debug, instrument};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use winr_types::{WinrError, WinrResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntegrityLevel {
    Untrusted,
    Low,
    Medium,
    High,
    System,
    Protected,
}

impl IntegrityLevel {
    fn from_rid(rid: u32) -> Self {
        match rid {
            0x0000..=0x0FFF => Self::Untrusted,
            0x1000..=0x1FFF => Self::Low,
            0x2000..=0x2FFF => Self::Medium,
            0x3000..=0x3FFF => Self::High,
            0x4000..=0x4FFF => Self::System,
            0x5000..=u32::MAX => Self::Protected,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::System => "system",
            Self::Protected => "protected",
        }
    }
}

#[instrument]
pub fn enforce_integrity_level_for_pid(pid: u32, action: &str) -> WinrResult<()> {
    let current = current_process_integrity_level()?;
    let target = process_integrity_level(pid)?;
    debug!(
        pid,
        current = current.as_str(),
        target = target.as_str(),
        action,
        "compared process integrity levels"
    );

    if target > current {
        return Err(WinrError::IntegrityLevelDenied);
    }

    Ok(())
}

fn current_process_integrity_level() -> WinrResult<IntegrityLevel> {
    let process = unsafe { GetCurrentProcess() };
    token_integrity_level_for_process(process)
}

fn process_integrity_level(pid: u32) -> WinrResult<IntegrityLevel> {
    let process =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map_err(|_| {
            WinrError::PermissionDenied {
                reason: format!("unable to query process {pid} for integrity level"),
            }
        })?;
    let result = token_integrity_level_for_process(process);
    let _ = unsafe { CloseHandle(process) };
    result
}

fn token_integrity_level_for_process(process: HANDLE) -> WinrResult<IntegrityLevel> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.map_err(|_| {
        WinrError::PermissionDenied {
            reason: "unable to open process token for integrity inspection".to_string(),
        }
    })?;

    let result = token_integrity_level(token);
    let _ = unsafe { CloseHandle(token) };
    result
}

fn token_integrity_level(token: HANDLE) -> WinrResult<IntegrityLevel> {
    let mut len = 0u32;
    let _ = unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len) };
    if len == 0 {
        return Err(WinrError::PermissionDenied {
            reason: "token integrity information is unavailable".to_string(),
        });
    }

    let mut buffer = vec![0u8; len as usize];
    unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr() as *mut c_void),
            len,
            &mut len,
        )
    }
    .map_err(|_| WinrError::PermissionDenied {
        reason: "failed to read token integrity information".to_string(),
    })?;

    let label = unsafe { &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
    let sid = label.Label.Sid;
    if !unsafe { IsValidSid(sid) }.as_bool() {
        return Err(WinrError::PermissionDenied {
            reason: "token integrity SID is invalid".to_string(),
        });
    }

    let count = unsafe { *GetSidSubAuthorityCount(sid) } as u32;
    if count == 0 {
        return Err(WinrError::PermissionDenied {
            reason: "token integrity SID has no sub-authorities".to_string(),
        });
    }

    let rid = unsafe { *GetSidSubAuthority(sid, count - 1) };
    Ok(IntegrityLevel::from_rid(rid))
}
