// launch.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! LaunchServices request for an explicitly selected local application bundle.

use ai2screenwake_backend::contract::Fault;
use core_foundation::{base::TCFType, url::CFURL};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    ptr,
};

/// Launch-only native bootstrap port; the transport worker owns readiness/deadlines.
pub trait OwnerLauncher: Send + Sync {
    /// Request the configured app; success never means protection or IPC readiness.
    fn launch(&self) -> Result<(), Fault>;
}

/// Exact app-bundle selection supplied by the owner connector, not global discovery.
#[derive(Clone, Debug)]
pub struct BundleLauncher {
    bundle_path: PathBuf,
}

impl BundleLauncher {
    /// Configure an explicit bundle; launch validates its actual filesystem target.
    pub fn new(bundle_path: PathBuf) -> Self {
        Self { bundle_path }
    }
}

impl OwnerLauncher for BundleLauncher {
    fn launch(&self) -> Result<(), Fault> {
        launch_owner(&self.bundle_path)
    }
}

#[repr(C)]
struct LaunchSpec {
    app_url: core_foundation::url::CFURLRef,
    item_urls: *const c_void,
    parameters: *const c_void,
    flags: u32,
    async_context: *mut c_void,
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSOpenFromURLSpec(spec: *const LaunchSpec, launched_url: *mut *const c_void) -> i32;
}

/// Ask LaunchServices to open exactly this existing absolute app bundle.
///
/// The IPC owner supplies its authorized bundled-app path and separately waits
/// for an authenticated owner connection. Success means launch admission only,
/// not application readiness or protection. Never starts a new instance on purpose,
/// presents OS error UI, brings the app forward, or enables protection.
pub fn launch_owner(bundle_path: &Path) -> Result<(), Fault> {
    let invalid = || {
        Fault::new(
            "owner_bundle_invalid",
            "The owner app bundle is unavailable.",
            true,
        )
    };
    if !bundle_path.is_absolute() {
        return Err(invalid());
    }
    let path = bundle_path.canonicalize().map_err(|_| invalid())?;
    if !path.is_dir() || path.extension().is_none_or(|value| value != "app") {
        return Err(invalid());
    }
    let url = CFURL::from_path(&path, true).ok_or_else(invalid)?;
    let spec = LaunchSpec {
        app_url: url.as_concrete_TypeRef(),
        item_urls: ptr::null(),
        parameters: ptr::null(),
        flags: 0x0001_0000 | 0x0000_0200 | 0x0000_0100,
        async_context: ptr::null_mut(),
    };
    // URL remains alive; no output URL is requested/retained.
    let status = unsafe { LSOpenFromURLSpec(&spec, ptr::null_mut()) };
    if status != 0 {
        return Err(Fault::new(
            "owner_launch_failed",
            &format!("macOS could not start the owner app ({status})."),
            true,
        ));
    }
    Ok(())
}
// end launch.rs
