// system.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Public Quartz session tap, CFPreferences reads and fail-closed change fencing.

use super::{
    Configuration, CornerApi, CornerCallback, CornerRegistration, Display, PermissionOutcome,
    Point, fault,
};
use ai2screenwake_backend::contract::Fault;
use core_foundation::{
    base::{CFRelease, CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::{CFDictionary, CFDictionaryRef},
    number::CFNumber,
    propertylist,
    runloop::{
        CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRemoveSource, CFRunLoopRunInMode,
        CFRunLoopSourceRef, kCFRunLoopDefaultMode,
    },
    string::{CFString, CFStringRef},
};
use std::{
    ffi::c_void,
    fs::OpenOptions,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type Event = *mut c_void;
type Port = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    origin: Point,
    size: Size,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    width: f64,
    height: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> Rect;
    fn CGEventCreate(source: *const c_void) -> Event;
    fn CGEventGetLocation(event: Event) -> Point;
    fn CGEventSetLocation(event: Event, location: Point);
    fn CGEventSetType(event: Event, kind: u32);
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: unsafe extern "C" fn(*mut c_void, u32, Event, *mut c_void) -> Event,
        context: *mut c_void,
    ) -> Port;
    fn CGEventTapEnable(port: Port, enabled: bool);
    fn CGEventTapIsEnabled(port: Port) -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFPreferencesCopyAppValue(key: CFStringRef, app: CFStringRef) -> CFTypeRef;
    fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: *const c_void) -> CFTypeRef;
    fn CFBundleGetMainBundle() -> *const c_void;
    fn CFBundleGetIdentifier(bundle: *const c_void) -> CFStringRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: Port,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFMachPortInvalidate(port: Port);
}

/// Real production boundary. No helper is allowed to request the app's grant.
pub struct SystemCorners;

fn number(key: &str, absent: i64) -> Result<i64, Fault> {
    let app = CFString::new("com.apple.dock");
    let key = CFString::new(key);
    let value =
        unsafe { CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), app.as_concrete_TypeRef()) };
    if value.is_null() {
        return Ok(absent);
    }
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value
        .downcast::<CFNumber>()
        .and_then(|value| value.to_i64())
        .ok_or_else(|| fault("corner_preference_invalid"))
}

fn displays() -> Result<Vec<Display>, Fault> {
    let mut ids = [0; 32];
    let mut count = 0;
    if unsafe { CGGetActiveDisplayList(32, ids.as_mut_ptr(), &mut count) } != 0
        || count == 0
        || count >= 32
    {
        return Err(fault("corner_displays_unavailable"));
    }
    Ok(ids[..count as usize]
        .iter()
        .map(|id| {
            let rect = unsafe { CGDisplayBounds(*id) };
            Display {
                origin: rect.origin,
                width: rect.size.width,
                height: rect.size.height,
            }
        })
        .collect())
}

impl CornerApi for SystemCorners {
    fn configuration(&self) -> Result<Configuration, Fault> {
        let mut actions = [1; 4];
        let mut modifiers = [0; 4];
        for (index, corner) in ["tl", "tr", "bl", "br"].iter().enumerate() {
            actions[index] = number(&format!("wvous-{corner}-corner"), 1)?;
            modifiers[index] = number(&format!("wvous-{corner}-modifier"), 0)?
                .try_into()
                .map_err(|_| fault("corner_modifier_invalid"))?;
        }
        Configuration::from_preferences(actions, modifiers, displays()?)
    }

    fn permission(&self, request: bool) -> PermissionOutcome {
        if unsafe { AXIsProcessTrusted() } != 0 {
            return PermissionOutcome::Checked(Ok(()));
        }
        let denied = Fault::new(
            "corner_accessibility_required",
            "Allow AI2ScreenWake in System Settings > Privacy & Security > Accessibility, then turn No Sleep on again.",
            true,
        );
        if request {
            let bundle = unsafe { CFBundleGetMainBundle() };
            if !bundle.is_null() {
                let identifier = unsafe { CFBundleGetIdentifier(bundle) };
                if !identifier.is_null()
                    && unsafe { CFString::wrap_under_get_rule(identifier) }
                        == "com.appliedmedia.ScreenWake"
                {
                    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
                    let options =
                        CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
                    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
                    return PermissionOutcome::Requested(Err(denied));
                }
            }
        }
        PermissionOutcome::Checked(Err(denied))
    }

    fn install(
        &self,
        configuration: &Configuration,
        callback: CornerCallback,
    ) -> Result<Box<dyn CornerRegistration>, Fault> {
        let directory = crate::identity::readable_log_path()?
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .ok_or_else(|| fault("corner_home_unavailable"))?
            .join("Preferences");
        let fence = Arc::new(PreferenceFence::open(
            directory.join("com.apple.dock.plist"),
            configuration,
        )?);
        // Watch first, then reread: configuration changes during startup cannot
        // become a ready source with the old action mapping.
        if &self.configuration()? != configuration || !fence.fresh() {
            return Err(fault("corner_configuration_changed"));
        }
        let mut registration = Registration {
            port: ptr::null_mut(),
            source: ptr::null_mut(),
            context: Box::new(Context {
                callback,
                fence,
                configuration: configuration.clone(),
            }),
        };
        let context = registration.context.as_mut() as *mut Context as *mut c_void;
        if configuration.required() {
            self.permission(false).into_result()?;
            let pointer = unsafe { CGEventCreate(ptr::null()) };
            if pointer.is_null() {
                return Err(fault("corner_pointer_unavailable"));
            }
            let point = unsafe { CGEventGetLocation(pointer) };
            unsafe { CFRelease(pointer) };
            if configuration.deflect(point).is_some() {
                return Err(Fault::new(
                    "corner_pointer_inside",
                    "Move the pointer away from the screen saver corner, then turn No Sleep on again.",
                    true,
                ));
            }
            // Only movement and drag events, no keyboard, buttons or scroll.
            registration.port = unsafe {
                CGEventTapCreate(
                    1,
                    0,
                    0,
                    (1 << 5) | (1 << 6) | (1 << 7) | (1 << 27),
                    tap,
                    context,
                )
            };
            if registration.port.is_null() {
                return Err(fault("corner_tap_unavailable"));
            }
            registration.source =
                unsafe { CFMachPortCreateRunLoopSource(ptr::null(), registration.port, 0) };
            if registration.source.is_null() {
                return Err(fault("corner_runloop_source_failed"));
            }
            unsafe {
                CFRunLoopAddSource(
                    CFRunLoopGetCurrent(),
                    registration.source,
                    kCFRunLoopDefaultMode,
                )
            };
            if !unsafe { CGEventTapIsEnabled(registration.port) } {
                return Err(fault("corner_tap_disabled"));
            }
        }
        if !registration.context.fence.fresh() {
            return Err(fault("corner_configuration_changed"));
        }
        Ok(Box::new(registration))
    }
}

/// Read-only, bounded fencing of an exact preferences file, with injectable path.
///
/// The callback only polls already registered kernel events. Revalidation and
/// property-list reads occur on the native source worker, never in the callback.
pub struct PreferenceFence {
    watch: Mutex<Watch>,
    pending: AtomicBool,
}

struct Watch {
    queue: OwnedFd,
    files: Vec<std::fs::File>,
    path: PathBuf,
    snapshot: Snapshot,
}

#[derive(PartialEq)]
struct Stamp(u64, u64, u64, i64, i64, i64, i64);

fn stamp(metadata: &std::fs::Metadata) -> Stamp {
    Stamp(
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

struct Snapshot {
    identity: Stamp,
    bytes: Vec<u8>,
}

fn read_snapshot(path: &Path) -> Result<(std::fs::File, Snapshot), Fault> {
    const LIMIT: u64 = 1024 * 1024;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| fault("corner_preferences_unavailable"))?;
    let before = file
        .metadata()
        .map_err(|_| fault("corner_preferences_unavailable"))?;
    if !before.is_file() || before.uid() != unsafe { libc::geteuid() } || before.len() > LIMIT {
        return Err(fault("corner_preferences_invalid"));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fault("corner_preferences_unavailable"))?;
    let after = file
        .metadata()
        .map_err(|_| fault("corner_preferences_unavailable"))?;
    let selected =
        std::fs::symlink_metadata(path).map_err(|_| fault("corner_preferences_unavailable"))?;
    if bytes.len() as u64 > LIMIT
        || stamp(&before) != stamp(&after)
        || stamp(&after) != stamp(&selected)
    {
        return Err(fault("corner_configuration_unstable"));
    }
    Ok((
        file,
        Snapshot {
            identity: stamp(&after),
            bytes,
        },
    ))
}

fn file_assignment(bytes: &[u8], configuration: &Configuration) -> Result<(), Fault> {
    let (value, _) = propertylist::create_with_data(CFData::from_buffer(bytes), 0)
        .map_err(|_| fault("corner_preference_invalid"))?;
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    let dictionary = value
        .downcast::<CFDictionary>()
        .ok_or_else(|| fault("corner_preference_invalid"))?;
    for (index, corner) in ["tl", "tr", "bl", "br"].iter().enumerate() {
        for (suffix, absent, expected) in [
            ("corner", 1, configuration.actions[index]),
            ("modifier", 0, configuration.modifiers[index] as i64),
        ] {
            let key = CFString::new(&format!("wvous-{corner}-{suffix}"));
            let raw = unsafe {
                CFDictionaryGetValue(
                    dictionary.as_concrete_TypeRef(),
                    key.as_concrete_TypeRef().cast(),
                )
            };
            let actual = if raw.is_null() {
                absent
            } else {
                let value = unsafe { CFType::wrap_under_get_rule(raw) };
                value
                    .downcast::<CFNumber>()
                    .and_then(|number| number.to_i64())
                    .ok_or_else(|| fault("corner_preference_invalid"))?
            };
            if actual != expected {
                return Err(fault("corner_configuration_changed"));
            }
        }
    }
    Ok(())
}

fn watch_file(queue: &OwnedFd, file: &std::fs::File) -> Result<(), Fault> {
    let change = libc::kevent {
        ident: file.as_raw_fd() as usize,
        filter: libc::EVFILT_VNODE,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: libc::NOTE_WRITE
            | libc::NOTE_DELETE
            | libc::NOTE_RENAME
            | libc::NOTE_REVOKE
            | libc::NOTE_ATTRIB
            | libc::NOTE_EXTEND,
        data: 0,
        udata: ptr::null_mut(),
    };
    if unsafe {
        libc::kevent(
            queue.as_raw_fd(),
            &change,
            1,
            ptr::null_mut(),
            0,
            ptr::null(),
        )
    } < 0
    {
        return Err(fault("corner_config_watch_failed"));
    }
    Ok(())
}

impl PreferenceFence {
    /// Watch first and validate exact bytes, identity and selected assignment.
    pub fn open(path: PathBuf, configuration: &Configuration) -> Result<Self, Fault> {
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return Err(fault("corner_config_watch_failed"));
        }
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        let directory = path
            .parent()
            .ok_or_else(|| fault("corner_preferences_invalid"))?;
        let mut files = Vec::new();
        for path in [directory, &path] {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_EVTONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .map_err(|_| fault("corner_config_watch_failed"))?;
            watch_file(&queue, &file)?;
            files.push(file);
        }
        let (_, snapshot) = read_snapshot(&path)?;
        file_assignment(&snapshot.bytes, configuration)?;
        let fence = Self {
            watch: Mutex::new(Watch {
                queue,
                files,
                path,
                snapshot,
            }),
            pending: AtomicBool::new(false),
        };
        if !fence.fresh() {
            return Err(fault("corner_configuration_unstable"));
        }
        Ok(fence)
    }

    /// Nonblocking kernel-only freshness test; ambiguous input passes unchanged.
    pub fn fresh(&self) -> bool {
        let Ok(watch) = self.watch.try_lock() else {
            return false;
        };
        if watch.dirty() {
            self.pending.store(true, Ordering::Release);
        }
        !self.pending.load(Ordering::Acquire)
    }

    /// Resolve a parent event using exact owned-file bytes and assignment.
    ///
    /// Unrelated files and unrelated Dock keys do not permanently disable the
    /// guard. Relevant changes, malformed/replaced unsafe files and an unstable
    /// read fail explicitly instead of retaining the old saver mapping.
    pub fn revalidate(&self, configuration: &Configuration) -> Result<(), Fault> {
        let mut watch = self
            .watch
            .lock()
            .map_err(|_| fault("corner_config_watch_failed"))?;
        if watch.dirty() {
            self.pending.store(true, Ordering::Release);
        }
        if !self.pending.load(Ordering::Acquire) {
            return Ok(());
        }
        let started = Instant::now();
        let (file, snapshot) = read_snapshot(&watch.path)?;
        file_assignment(&snapshot.bytes, configuration)?;
        if snapshot.identity != watch.snapshot.identity {
            watch_file(&watch.queue, &file)?;
            watch.files[1] = file;
        }
        watch.snapshot = snapshot;
        if watch.dirty() || started.elapsed() > Duration::from_millis(50) {
            return Err(fault("corner_configuration_unstable"));
        }
        self.pending.store(false, Ordering::Release);
        Ok(())
    }
}

impl Watch {
    fn dirty(&self) -> bool {
        let mut events = std::mem::MaybeUninit::<[libc::kevent; 8]>::uninit();
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let count = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                ptr::null(),
                0,
                events.as_mut_ptr().cast(),
                8,
                &timeout,
            )
        };
        count != 0
    }
}

struct Context {
    callback: CornerCallback,
    fence: Arc<PreferenceFence>,
    configuration: Configuration,
}

unsafe extern "C" fn tap(_: *mut c_void, kind: u32, event: Event, context: *mut c_void) -> Event {
    let Some(context) = (unsafe { (context as *const Context).as_ref() }) else {
        return event;
    };
    if kind == u32::MAX || kind == u32::MAX - 1 {
        context.callback.lost(fault("corner_tap_disabled"));
    } else if context.fence.fresh() && matches!(kind, 5 | 6 | 7 | 27) && !event.is_null() {
        let point = unsafe { CGEventGetLocation(event) };
        if context.configuration.deflect(point).is_some() {
            match displays() {
                Ok(displays) if displays == context.configuration.displays => {
                    unsafe { filter_original(&context.callback, kind, event) };
                }
                _ => context.callback.lost(fault("corner_display_changed")),
            }
        }
    }
    event
}

struct Registration {
    port: Port,
    source: CFRunLoopSourceRef,
    context: Box<Context>,
}

impl CornerRegistration for Registration {
    fn validate_activation(&mut self) -> Result<(), Fault> {
        self.context.fence.revalidate(&self.context.configuration)?;
        if displays()? != self.context.configuration.displays {
            return Err(fault("corner_display_changed"));
        }
        if !self.context.configuration.required() {
            return Ok(());
        }
        SystemCorners.permission(false).into_result()?;
        if !unsafe { CGEventTapIsEnabled(self.port) } {
            return Err(fault("corner_tap_disabled"));
        }
        let event = unsafe { CGEventCreate(ptr::null()) };
        if event.is_null() {
            return Err(fault("corner_pointer_unavailable"));
        }
        let point = unsafe { CGEventGetLocation(event) };
        unsafe { CFRelease(event) };
        if self.context.configuration.deflect(point).is_some() {
            return Err(Fault::new(
                "corner_pointer_inside",
                "Move the pointer away from the screen saver corner, then turn No Sleep on again.",
                true,
            ));
        }
        Ok(())
    }

    fn poll(&mut self) -> Result<(), Fault> {
        self.context.fence.revalidate(&self.context.configuration)?;
        if displays()? != self.context.configuration.displays {
            return Err(fault("corner_display_changed"));
        }
        if self.context.configuration.required() {
            SystemCorners.permission(false).into_result()?;
            if !unsafe { CGEventTapIsEnabled(self.port) } {
                return Err(fault("corner_tap_disabled"));
            }
        }
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.005, 1) };
        Ok(())
    }

    fn stop(&mut self) {
        if !self.port.is_null() {
            unsafe { CGEventTapEnable(self.port, false) };
            if !self.source.is_null() {
                unsafe {
                    CFRunLoopRemoveSource(
                        CFRunLoopGetCurrent(),
                        self.source,
                        kCFRunLoopDefaultMode,
                    );
                    CFRelease(self.source.cast());
                }
                self.source = ptr::null_mut();
            }
            unsafe {
                CFMachPortInvalidate(self.port);
                CFRelease(self.port)
            };
            self.port = ptr::null_mut();
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe fn filter_original(callback: &CornerCallback, kind: u32, event: Event) {
    if matches!(kind, 5 | 6 | 7 | 27) && !event.is_null() {
        let point = unsafe { CGEventGetLocation(event) };
        callback.deflect(point, |point| unsafe { CGEventSetLocation(event, point) });
    }
}

/// Exercise the production original-event boundary on a never-posted CGEvent.
///
/// This constructs only a local event. It does not install a tap, request
/// permission, post input or move the real pointer.
pub fn unposted_event(callback: &CornerCallback, kind: u32, point: Point) -> Result<Point, Fault> {
    let event = unsafe { CGEventCreate(ptr::null()) };
    if event.is_null() {
        return Err(fault("corner_event_allocation_failed"));
    }
    unsafe {
        CGEventSetType(event, kind);
        CGEventSetLocation(event, point);
        filter_original(callback, kind, event);
    }
    let result = unsafe { CGEventGetLocation(event) };
    unsafe { CFRelease(event) };
    Ok(result)
}

/// Bounded calling-process installation probe with an always-pass-through tap.
///
/// Never prompts, authorizes filtering, inspects input or establishes the signed
/// target application's permission. Unavailable permission remains an error.
pub fn pass_through_probe() -> Result<(), Fault> {
    SystemCorners.permission(false).into_result()?;
    unsafe extern "C" fn pass(_: *mut c_void, _: u32, event: Event, _: *mut c_void) -> Event {
        event
    }
    struct Probe {
        port: Port,
        source: CFRunLoopSourceRef,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            unsafe {
                CGEventTapEnable(self.port, false);
                if !self.source.is_null() {
                    CFRunLoopRemoveSource(
                        CFRunLoopGetCurrent(),
                        self.source,
                        kCFRunLoopDefaultMode,
                    );
                    CFRelease(self.source.cast());
                }
                CFMachPortInvalidate(self.port);
                CFRelease(self.port);
            }
        }
    }
    let port = unsafe { CGEventTapCreate(1, 0, 0, 1 << 5, pass, ptr::null_mut()) };
    if port.is_null() {
        return Err(fault("corner_tap_unavailable"));
    }
    let mut probe = Probe {
        port,
        source: ptr::null_mut(),
    };
    probe.source = unsafe { CFMachPortCreateRunLoopSource(ptr::null(), port, 0) };
    if probe.source.is_null() {
        return Err(fault("corner_runloop_source_failed"));
    }
    unsafe { CFRunLoopAddSource(CFRunLoopGetCurrent(), probe.source, kCFRunLoopDefaultMode) };
    if !unsafe { CGEventTapIsEnabled(port) } {
        return Err(fault("corner_tap_disabled"));
    }
    unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.005, 1) };
    if !unsafe { CGEventTapIsEnabled(port) } {
        return Err(fault("corner_tap_disabled"));
    }
    Ok(())
}
// end system.rs
