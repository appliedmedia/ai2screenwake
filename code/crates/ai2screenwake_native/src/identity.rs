// identity.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Resolve the same login-session location for the app and its stdio clients.

use ai2screenwake_backend::contract::Fault;
use std::{
    ffi::CStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SessionGetInfo(session: u32, actual: *mut u32, attributes: *mut u32) -> i32;
}

/// Resolve, but do not create, the canonical per-user/per-login runtime directory.
///
/// Uses Darwin's per-user temporary root and Security's inherited login session,
/// never TMPDIR, a shared /tmp name, or process getsid. Client IPC owns directory
/// creation, ownership/mode checks, locks, sockets and peer authorization.
pub fn runtime_directory() -> Result<PathBuf, Fault> {
    let fault = || {
        Fault::new(
            "runtime_identity_failed",
            "The login session is unavailable.",
            true,
        )
    };
    // No setuid/effective-identity transition is supported by this local utility.
    if unsafe { libc::getuid() } != unsafe { libc::geteuid() } {
        return Err(fault());
    }
    let mut session = 0;
    let mut attributes = 0;
    // callerSecuritySession=-1; output types exactly match AuthSession.h.
    let status = unsafe { SessionGetInfo(u32::MAX, &mut session, &mut attributes) };
    if status != 0 || session == 0 || session == u32::MAX || attributes & 0x0010 == 0 {
        return Err(fault());
    }
    // A zero length is the documented error result; include the terminating NUL.
    let length = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if length <= 1 {
        return Err(fault());
    }
    let mut bytes = vec![0_u8; length];
    let copied = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if copied != length {
        return Err(fault());
    }
    let value = CStr::from_bytes_with_nul(&bytes).map_err(|_| fault())?;
    let root = PathBuf::from(std::ffi::OsStr::from_bytes(value.to_bytes()));
    if !root.is_absolute() {
        return Err(fault());
    }
    let root = root.canonicalize().map_err(|_| fault())?;
    if !root.is_dir() {
        return Err(fault());
    }
    Ok(root.join(format!("ai2screenwake-{session:x}")))
}

fn log_fault() -> Fault {
    Fault::new(
        "readable_log_location",
        "The user log location or local time is unavailable.",
        true,
    )
}

/// Capture one real instant for the owner run; this does not create any file.
pub fn readable_log_path_in(home: &Path) -> Result<PathBuf, Fault> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| log_fault())?
        .as_secs();
    let home = match home.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => home
            .parent()
            .ok_or_else(log_fault)?
            .canonicalize()
            .map_err(|_| log_fault())?
            .join(home.file_name().ok_or_else(log_fault)?),
        Err(_) => return Err(log_fault()),
    };
    readable_log_path_at(&home, i64::try_from(seconds).map_err(|_| log_fault())?)
}

/// Format an injected instant using the OS local calendar, never a fabricated future time.
///
/// The caller owns `home`; fixtures pass a private root, not the actual user's home.
pub fn readable_log_path_at(home: &Path, seconds: i64) -> Result<PathBuf, Fault> {
    if !home.is_absolute()
        || home.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(log_fault());
    }
    let home = match home.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => home
            .parent()
            .ok_or_else(log_fault)?
            .canonicalize()
            .map_err(|_| log_fault())?
            .join(home.file_name().ok_or_else(log_fault)?),
        Err(_) => return Err(log_fault()),
    };
    let instant: libc::time_t = seconds;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r writes one complete tm on success; the captured input remains alive.
    if unsafe { libc::localtime_r(&instant, local.as_mut_ptr()) }.is_null() {
        return Err(log_fault());
    }
    let local = unsafe { local.assume_init() };
    let year = local.tm_year.checked_add(1900).ok_or_else(log_fault)?;
    let weekday = ["su", "mo", "tu", "we", "th", "fr", "sa"]
        .get(usize::try_from(local.tm_wday).map_err(|_| log_fault())?)
        .ok_or_else(log_fault)?;
    if !(1..=9999).contains(&year)
        || !(0..12).contains(&local.tm_mon)
        || !(1..=31).contains(&local.tm_mday)
        || !(0..24).contains(&local.tm_hour)
        || !(0..60).contains(&local.tm_min)
        || !(0..=60).contains(&local.tm_sec)
    {
        return Err(log_fault());
    }
    Ok(home.join("Library/Logs/AI2ScreenWake").join(format!(
        "{year:04}{:02}{:02}{weekday}{:02}{:02}{:02}_ai2sw.log",
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
        local.tm_sec
    )))
}

/// Resolve the real-UID account home without HOME/environment, then capture local naming once.
///
/// A failure is retained by the host for explicit Show Log, not a power-startup prerequisite.
pub fn readable_log_path() -> Result<PathBuf, Fault> {
    let uid = unsafe { libc::getuid() };
    if uid != unsafe { libc::geteuid() } {
        return Err(log_fault());
    }
    let length = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let length = usize::try_from(length).map_err(|_| log_fault())?;
    if length == 0 || length > 1024 * 1024 {
        return Err(log_fault());
    }
    let mut buffer = vec![0_u8; length];
    let mut account = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut found = std::ptr::null_mut();
    // SAFETY: all buffers and output pointers are writable and retained through the copied home.
    let result = unsafe {
        libc::getpwuid_r(
            uid,
            account.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if result != 0 || found.is_null() {
        return Err(log_fault());
    }
    let account = unsafe { account.assume_init() };
    if account.pw_dir.is_null() {
        return Err(log_fault());
    }
    let home = unsafe { CStr::from_ptr(account.pw_dir) };
    let home = PathBuf::from(std::ffi::OsStr::from_bytes(home.to_bytes()));
    let home = home.canonicalize().map_err(|_| log_fault())?;
    readable_log_path_in(&home)
}
// end identity.rs
