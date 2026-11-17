// identity.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Resolve the same login-session location for the app and its stdio clients.

use ai2screenwake_backend::contract::Fault;
use std::{ffi::CStr, os::unix::ffi::OsStrExt, path::PathBuf};

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
// end identity.rs
