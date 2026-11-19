// system.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Public Quartz session tap, CFPreferences reads and fail-closed change fencing.

use super::{Configuration, CornerApi, CornerCallback, CornerRegistration, Display, Point, fault};
use ai2screenwake_backend::contract::Fault;
use core_foundation::{
    base::{CFRelease, CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    dictionary::{CFDictionary, CFDictionaryRef},
    number::CFNumber,
    runloop::{
        CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRemoveSource, CFRunLoopRunInMode,
        CFRunLoopSourceRef, kCFRunLoopDefaultMode,
    },
    string::{CFString, CFStringRef},
};
use std::{
    ffi::c_void,
    fs::OpenOptions,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::OpenOptionsExt,
    },
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    fn CGDisplayRegisterReconfigurationCallback(
        callback: unsafe extern "C" fn(u32, u32, *mut c_void),
        context: *mut c_void,
    ) -> i32;
    fn CGDisplayRemoveReconfigurationCallback(
        callback: unsafe extern "C" fn(u32, u32, *mut c_void),
        context: *mut c_void,
    ) -> i32;
    fn CGEventCreate(source: *const c_void) -> Event;
    fn CGEventGetLocation(event: Event) -> Point;
    fn CGEventSetLocation(event: Event, location: Point);
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
    fn CFPreferencesAppSynchronize(app: CFStringRef) -> u8;
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

impl CornerApi for SystemCorners {
    fn configuration(&self) -> Result<Configuration, Fault> {
        let app = CFString::new("com.apple.dock");
        if unsafe { CFPreferencesAppSynchronize(app.as_concrete_TypeRef()) } == 0 {
            return Err(fault("corner_preferences_unavailable"));
        }
        let mut actions = [1; 4];
        let mut modifiers = [0; 4];
        for (index, corner) in ["tl", "tr", "bl", "br"].iter().enumerate() {
            actions[index] = number(&format!("wvous-{corner}-corner"), 1)?;
            modifiers[index] = number(&format!("wvous-{corner}-modifier"), 0)?
                .try_into()
                .map_err(|_| fault("corner_modifier_invalid"))?;
        }
        let mut ids = [0; 32];
        let mut count = 0;
        if unsafe { CGGetActiveDisplayList(32, ids.as_mut_ptr(), &mut count) } != 0
            || count == 0
            || count >= 32
        {
            return Err(fault("corner_displays_unavailable"));
        }
        let displays = ids[..count as usize]
            .iter()
            .map(|id| {
                let rect = unsafe { CGDisplayBounds(*id) };
                Display {
                    origin: rect.origin,
                    width: rect.size.width,
                    height: rect.size.height,
                }
            })
            .collect();
        Configuration::from_preferences(actions, modifiers, displays)
    }

    fn permission(&self, request: bool) -> Result<(), Fault> {
        if unsafe { AXIsProcessTrusted() } != 0 {
            return Ok(());
        }
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
                }
            }
        }
        Err(Fault::new(
            "corner_accessibility_required",
            "Allow AI2ScreenWake in System Settings > Privacy & Security > Accessibility, then turn No Sleep on again.",
            true,
        ))
    }

    fn install(
        &self,
        configuration: &Configuration,
        callback: CornerCallback,
    ) -> Result<Box<dyn CornerRegistration>, Fault> {
        let fence = Arc::new(Fence::open()?);
        // Watch first, then reread: configuration changes during startup cannot
        // become a ready source with the old action mapping.
        if &self.configuration()? != configuration || !fence.fresh() {
            return Err(fault("corner_configuration_changed"));
        }
        let mut registration = Registration {
            port: ptr::null_mut(),
            source: ptr::null_mut(),
            context: Box::new(Context { callback, fence }),
            display_registered: false,
            configuration: configuration.clone(),
        };
        let context = registration.context.as_mut() as *mut Context as *mut c_void;
        if unsafe { CGDisplayRegisterReconfigurationCallback(display_changed, context) } != 0 {
            return Err(fault("corner_display_watch_failed"));
        }
        registration.display_registered = true;
        if configuration.required() {
            self.permission(false)?;
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

struct Fence {
    queue: OwnedFd,
    _files: Vec<std::fs::File>,
    changed: AtomicBool,
}

impl Fence {
    fn open() -> Result<Self, Fault> {
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return Err(fault("corner_config_watch_failed"));
        }
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        let directory = crate::identity::readable_log_path()?
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .ok_or_else(|| fault("corner_home_unavailable"))?
            .join("Preferences");
        let mut files = Vec::new();
        // Parent watches catch atomic replacement; any ambiguous change retires
        // the capability, never briefly continuing with a cached saver action.
        for path in [&directory, &directory.join("com.apple.dock.plist")] {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_EVTONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .map_err(|_| fault("corner_config_watch_failed"))?;
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
            files.push(file);
        }
        Ok(Self {
            queue,
            _files: files,
            changed: AtomicBool::new(false),
        })
    }

    fn fresh(&self) -> bool {
        if self.changed.load(Ordering::Acquire) {
            return false;
        }
        let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let count = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                ptr::null(),
                0,
                event.as_mut_ptr(),
                1,
                &timeout,
            )
        };
        if count != 0 {
            self.changed.store(true, Ordering::Release);
        }
        !self.changed.load(Ordering::Acquire)
    }
}

struct Context {
    callback: CornerCallback,
    fence: Arc<Fence>,
}

unsafe extern "C" fn display_changed(_: u32, _: u32, context: *mut c_void) {
    if let Some(context) = unsafe { (context as *const Context).as_ref() } {
        context.fence.changed.store(true, Ordering::Release);
        context.callback.lost(fault("corner_display_changed"));
    }
}

unsafe extern "C" fn tap(_: *mut c_void, kind: u32, event: Event, context: *mut c_void) -> Event {
    let Some(context) = (unsafe { (context as *const Context).as_ref() }) else {
        return event;
    };
    if kind == u32::MAX || kind == u32::MAX - 1 {
        context.callback.lost(fault("corner_tap_disabled"));
    } else if !context.fence.fresh() {
        context.callback.lost(fault("corner_configuration_changed"));
    } else if matches!(kind, 5 | 6 | 7 | 27) && !event.is_null() {
        let point = unsafe { CGEventGetLocation(event) };
        if let Some(point) = context.callback.filter(point) {
            unsafe { CGEventSetLocation(event, point) };
        }
    }
    event
}

struct Registration {
    port: Port,
    source: CFRunLoopSourceRef,
    context: Box<Context>,
    display_registered: bool,
    configuration: Configuration,
}

impl CornerRegistration for Registration {
    fn poll(&mut self) -> Result<(), Fault> {
        if !self.context.fence.fresh() {
            return Err(fault("corner_configuration_changed"));
        }
        if self.configuration.required() {
            SystemCorners.permission(false)?;
            if !unsafe { CGEventTapIsEnabled(self.port) } {
                return Err(fault("corner_tap_disabled"));
            }
        }
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.005, 1) };
        Ok(())
    }

    fn stop(&mut self) {
        if self.display_registered {
            let status = unsafe {
                CGDisplayRemoveReconfigurationCallback(
                    display_changed,
                    self.context.as_mut() as *mut Context as *mut c_void,
                )
            };
            if status != 0 {
                // Keep callback context alive if the OS cannot retire its pointer.
                self.context
                    .callback
                    .lost(fault("corner_display_unregistration_failed"));
            } else {
                self.display_registered = false;
            }
        }
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
        if self.display_registered {
            let replacement = Box::new(Context {
                callback: self.context.callback.clone(),
                fence: self.context.fence.clone(),
            });
            let retained = std::mem::replace(&mut self.context, replacement);
            Box::leak(retained);
        }
    }
}
// end system.rs
