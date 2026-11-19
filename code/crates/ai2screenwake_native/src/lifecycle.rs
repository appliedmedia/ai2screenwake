// lifecycle.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owned OS notification pumping, separate from potentially reentrant bus observers.

use crate::power::MacPower;
use ai2screenwake_backend::contract::{
    Error, Fault, IdleSleepCancelled, IdleVetoFailure, InterruptionObserved, Power, SleepPhase,
    WeakEmitter,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const START_BUDGET: Duration = Duration::from_secs(1);
const STOP_BUDGET: Duration = Duration::from_millis(500);
const PUMP_INTERVAL: Duration = Duration::from_millis(25);
const QUEUE_LIMIT: usize = 32;

fn fault(code: &str) -> Fault {
    Fault::new(code, "Native sleep/wake observation is unavailable.", true)
}

/// The public OS messages this observer understands; none is a suppression record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SleepNotification {
    /// Ask whether idle sleep is allowed; only current admitted protection may reject it.
    CanSleep,
    /// Non-abortable sleep is beginning: acknowledge before doing any other work.
    WillSleep,
    /// Devices have completed waking: this message must not be acknowledged.
    DidWake,
}

/// The reply to this exact OS acknowledgement token; wake has no reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SleepDecision {
    /// Permit idle or non-abortable sleep.
    Allow,
    /// Reject only an actual CanSystemSleep request.
    Cancel,
}

/// Native-specific injectable registration, constructed and used on its own OS thread.
pub trait NotificationSource: Send + Sync + 'static {
    /// Register the callback or fail explicitly; must not invoke application observers.
    fn register(
        &self,
        callback: NotificationCallback,
    ) -> Result<Box<dyn NotificationRegistration>, Fault>;
}

/// Thread-confined OS registration; one poll must return within the pump interval.
pub trait NotificationRegistration {
    /// Pump at most 25ms of notifications; never wait for bus delivery.
    fn poll(&mut self) -> Result<(), Fault>;
    /// Remove callbacks and release native registration resources on this same thread.
    fn stop(&mut self) -> Result<(), Fault>;
}

struct Pending {
    request: ai2screenwake_backend::contract::Apply,
    events: WeakEmitter,
    observation: Option<InterruptionObserved>,
    faults: Vec<Fault>,
    cancelled: Option<IdleSleepCancelled>,
    veto_failure: Option<Fault>,
}

#[derive(Default)]
struct Queue {
    pending: Mutex<VecDeque<Pending>>,
    wake: Condvar,
    stopped: AtomicBool,
}

impl Queue {
    fn push(&self, mut item: Pending) {
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        let mut pending = match self.pending.lock() {
            Ok(pending) => pending,
            Err(poisoned) => {
                item.faults.push(fault("lifecycle_queue_poisoned"));
                poisoned.into_inner()
            }
        };
        if pending.len() == QUEUE_LIMIT {
            pending.clear();
            item.faults.push(fault("lifecycle_queue_overflow"));
        }
        pending.push_back(item);
        self.wake.notify_one();
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.wake.notify_all();
    }
}

/// Cloneable OS callback handle; contains only weak owner/context references.
#[derive(Clone)]
pub struct NotificationCallback {
    power: Weak<MacPower>,
    queue: Arc<Queue>,
    generation: u64,
}

impl NotificationCallback {
    /// Reply before clocks, owner locks or bus work; forced sleep is always allowed.
    ///
    /// The OS source supplies its exact acknowledgement token through this closure.
    pub fn receive(
        &self,
        notification: SleepNotification,
        reply: impl FnOnce(SleepDecision) -> Result<(), Fault>,
    ) {
        if notification == SleepNotification::CanSleep
            && !self.queue.stopped.load(Ordering::Acquire)
            && let Some(power) = self.power.upgrade()
        {
            let (admission, result) = power.idle_veto.reply(self.generation, reply);
            if let Some(admission) = admission {
                let result = result.and_then(|()| power.reply_clock());
                let (cancelled, failure) = match result {
                    Ok(clock) => (
                        Some(IdleSleepCancelled {
                            request: admission.request.clone(),
                            clock,
                        }),
                        None,
                    ),
                    Err(failure) => {
                        power.idle_veto.retire_request(&admission.request);
                        (None, Some(failure))
                    }
                };
                self.queue.push(Pending {
                    request: admission.request.clone(),
                    events: admission.events.clone(),
                    observation: None,
                    faults: failure.iter().cloned().collect(),
                    cancelled,
                    veto_failure: failure,
                });
            } else if let Err(failure) = result {
                self.capture(None, Some(failure));
            }
            return;
        }
        let acknowledgement = match notification {
            SleepNotification::CanSleep | SleepNotification::WillSleep => {
                reply(SleepDecision::Allow)
            }
            SleepNotification::DidWake => Ok(()),
        };
        if self.queue.stopped.load(Ordering::Acquire) {
            return;
        }
        let phase = match notification {
            SleepNotification::CanSleep => None,
            SleepNotification::WillSleep => Some(SleepPhase::WillSleep),
            SleepNotification::DidWake => Some(SleepPhase::DidWake),
        };
        if phase.is_none() && acknowledgement.is_ok() {
            return;
        }
        self.capture(phase, acknowledgement.err());
    }

    fn capture(&self, phase: Option<SleepPhase>, failure: Option<Fault>) {
        let Some(power) = self.power.upgrade() else {
            return;
        };
        if !power.lifecycle_generation_is_current(self.generation) {
            return;
        }
        let Some(context) = power.lifecycle_context() else {
            return;
        };
        if let Some(phase) = phase {
            if !power.lifecycle_phase(
                self.generation,
                &context.request,
                phase == SleepPhase::WillSleep,
            ) {
                return;
            }
            if phase == SleepPhase::DidWake && context.held_count != 2 {
                power.idle_veto.retire_request(&context.request);
            }
        }
        let mut faults: Vec<_> = failure.into_iter().collect();
        let mut veto_failure = None;
        let observation = match context.clock {
            Ok(clock) => phase.map(|phase| InterruptionObserved {
                request: context.request.clone(),
                clock,
                phase,
                held_count: context.held_count,
            }),
            Err(error) => {
                power.idle_veto.retire_request(&context.request);
                veto_failure = Some(error.clone());
                faults.push(error);
                None
            }
        };
        self.queue.push(Pending {
            request: context.request,
            events: context.events,
            observation,
            faults,
            cancelled: None,
            veto_failure,
        });
    }
}

struct Worker {
    handle: Option<JoinHandle<()>>,
    done: Receiver<Result<(), Fault>>,
}

impl Worker {
    fn spawn(
        name: &str,
        run: impl FnOnce() -> Result<(), Fault> + Send + 'static,
    ) -> Result<Self, Fault> {
        let (sender, done) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let result = run();
                let _ = sender.send(result);
            })
            .map_err(|_| fault("lifecycle_thread_failed"))?;
        Ok(Self {
            handle: Some(handle),
            done,
        })
    }

    fn finish(&mut self, deadline: Instant) -> Result<(), Fault> {
        let Some(handle) = self.handle.as_ref() else {
            return Ok(());
        };
        if handle.thread().id() == thread::current().id() {
            // The currently executing observer must return before its worker can exit.
            self.handle.take();
            return Ok(());
        }
        let result = self
            .done
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| fault("lifecycle_stop_timeout"))?;
        let handle = self.handle.take().expect("checked worker handle");
        // Completion is sent after the registration/delivery function has torn down.
        // Never join an unfinished thread, even during exceptional unwinding.
        if handle.is_finished() && handle.join().is_err() {
            return Err(fault("lifecycle_thread_failed"));
        }
        result
    }
}

/// Explicit host-owned monitor. Construction of MacPower alone starts no thread.
pub struct LifecycleMonitor {
    queue: Arc<Queue>,
    source: Worker,
    delivery: Worker,
    power: Weak<MacPower>,
    generation: u64,
}

impl LifecycleMonitor {
    /// Register public IOKit notifications on a dedicated, bounded-pump OS thread.
    #[cfg(target_os = "macos")]
    pub fn start(power: Arc<MacPower>) -> Result<Self, Fault> {
        Self::with_source(power, Arc::new(macos::SystemNotifications))
    }

    /// Start an injected native source; production uses the actual macOS source.
    pub fn with_source(
        power: Arc<MacPower>,
        source: Arc<dyn NotificationSource>,
    ) -> Result<Self, Fault> {
        let generation = power.begin_lifecycle()?;
        let queue = Arc::new(Queue::default());
        let callback = NotificationCallback {
            power: Arc::downgrade(&power),
            queue: queue.clone(),
            generation,
        };
        let delivery_queue = queue.clone();
        let weak_power = Arc::downgrade(&power);
        let delivery = Worker::spawn("screenwake-lifecycle-events", move || {
            loop {
                let mut pending = delivery_queue
                    .pending
                    .lock()
                    .map_err(|_| fault("lifecycle_queue_poisoned"))?;
                while pending.is_empty() && !delivery_queue.stopped.load(Ordering::Acquire) {
                    pending = delivery_queue
                        .wake
                        .wait_timeout(pending, PUMP_INTERVAL)
                        .map_err(|_| fault("lifecycle_queue_poisoned"))?
                        .0;
                }
                if delivery_queue.stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                let item = pending.pop_front().expect("nonempty pending queue");
                drop(pending);
                let Some(power) = weak_power.upgrade() else {
                    continue;
                };
                let Some(events) = item.events.upgrade() else {
                    continue;
                };
                for failure in item.faults {
                    events.error::<Error::Native::Activity::Occurred>(failure);
                }
                if let Some(fault) = item.veto_failure {
                    events.emit::<Power::Activity::IdleSleep::Failed>(IdleVetoFailure {
                        request: item.request.clone(),
                        fault,
                    });
                }
                if let Some(cancelled) = item.cancelled {
                    events.emit::<Power::Activity::IdleSleep::Cancelled>(cancelled);
                }
                if let Some(observation) = item.observation.filter(|_| {
                    power.lifecycle_generation_is_current(generation)
                        && power.lifecycle_is_current(&item.request)
                        && !delivery_queue.stopped.load(Ordering::Acquire)
                }) {
                    events.emit::<Power::Activity::System::Interrupted>(observation);
                }
            }
        })?;
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let source_queue = queue.clone();
        let mut delivery = delivery;
        let source_power = Arc::downgrade(&power);
        let source_worker = Worker::spawn("screenwake-lifecycle-os", move || {
            let mut registration = match source.register(callback.clone()) {
                Ok(registration) => registration,
                Err(error) => {
                    let _ = ready_sender.send(Err(error.clone()));
                    return Err(error);
                }
            };
            if let Some(power) = source_power.upgrade() {
                power.lifecycle_availability(generation, true, None);
            }
            let _ = ready_sender.send(Ok(()));
            let result = loop {
                if source_queue.stopped.load(Ordering::Acquire) {
                    break Ok(());
                }
                if let Err(error) = registration.poll() {
                    if let Some(power) = source_power.upgrade() {
                        power.lifecycle_availability(generation, false, Some(error.clone()));
                    }
                    break Err(error);
                }
            };
            let cleanup = registration.stop();
            if let Err(error) = &cleanup {
                callback.capture(None, Some(error.clone()));
            }
            result.and(cleanup)
        });
        let source = match source_worker {
            Ok(source) => source,
            Err(error) => {
                queue.stop();
                let _ = delivery.finish(Instant::now() + STOP_BUDGET);
                return Err(error);
            }
        };
        let mut monitor = Self {
            queue,
            source,
            delivery,
            power: Arc::downgrade(&power),
            generation,
        };
        match ready.recv_timeout(START_BUDGET) {
            Ok(Ok(())) => Ok(monitor),
            outcome => {
                let error = match outcome {
                    Ok(Err(error)) => error,
                    _ => fault("lifecycle_start_timeout"),
                };
                let _ = monitor.stop();
                Err(error)
            }
        }
    }

    /// Stop notification production and delivery within 500ms, or return an explicit fault.
    ///
    /// Calling from a bus observer is safe: its own delivery thread exits when that
    /// observer returns rather than attempting to join itself. Retry a timed-out
    /// stop after a long-running external observer finishes.
    pub fn stop(&mut self) -> Result<(), Fault> {
        if let Some(power) = self.power.upgrade() {
            power.lifecycle_availability(self.generation, false, None);
        }
        self.queue.stop();
        let deadline = Instant::now() + STOP_BUDGET;
        let source = self.source.finish(deadline);
        let delivery = self.delivery.finish(deadline);
        source.and(delivery)
    }
}

impl Drop for LifecycleMonitor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::{ffi::c_void, ptr};

    type CfRef = *const c_void;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IORegisterForSystemPower(
            context: *mut c_void,
            port: *mut *mut c_void,
            callback: extern "C" fn(*mut c_void, u32, u32, *mut c_void),
            notifier: *mut u32,
        ) -> u32;
        fn IODeregisterForSystemPower(notifier: *mut u32) -> i32;
        fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> CfRef;
        fn IONotificationPortDestroy(port: *mut c_void);
        fn IOServiceClose(connection: u32) -> i32;
        fn IOAllowPowerChange(connection: u32, token: isize) -> i32;
        fn IOCancelPowerChange(connection: u32, token: isize) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: CfRef;
        fn CFRunLoopGetCurrent() -> CfRef;
        fn CFRunLoopAddSource(run_loop: CfRef, source: CfRef, mode: CfRef);
        fn CFRunLoopRemoveSource(run_loop: CfRef, source: CfRef, mode: CfRef);
        fn CFRunLoopRunInMode(mode: CfRef, seconds: f64, return_after_source: u8) -> i32;
    }

    struct Context {
        callback: NotificationCallback,
        root: u32,
    }

    extern "C" fn notified(
        context: *mut c_void,
        _service: u32,
        message: u32,
        argument: *mut c_void,
    ) {
        // IO callbacks run only on this registration's run loop. The Box outlives
        // deregistration, run-loop-source removal and notification-port destruction.
        let context = unsafe { &*context.cast::<Context>() };
        let notification = match message {
            0xe000_0270 => SleepNotification::CanSleep,
            0xe000_0280 => SleepNotification::WillSleep,
            0xe000_0300 => SleepNotification::DidWake,
            _ => return,
        };
        context.callback.receive(notification, |decision| {
            let result = match decision {
                SleepDecision::Cancel => unsafe {
                    IOCancelPowerChange(context.root, argument as isize)
                },
                SleepDecision::Allow => unsafe {
                    IOAllowPowerChange(context.root, argument as isize)
                },
            };
            if result == 0 {
                Ok(())
            } else {
                Err(match decision {
                    SleepDecision::Cancel => Fault::new(
                        "lifecycle_cancellation_failed",
                        &format!("Idle system-sleep cancellation failed ({result:#x})."),
                        true,
                    ),
                    SleepDecision::Allow => fault("lifecycle_acknowledgement_failed"),
                })
            }
        });
    }

    /// The factory is shareable; its actual CF/IOKit registration never crosses threads.
    pub(super) struct SystemNotifications;

    struct Registration {
        context: Box<Context>,
        port: *mut c_void,
        notifier: u32,
        source: CfRef,
        run_loop: CfRef,
        attached: bool,
    }

    impl NotificationSource for SystemNotifications {
        fn register(
            &self,
            callback: NotificationCallback,
        ) -> Result<Box<dyn NotificationRegistration>, Fault> {
            let mut registration = Registration {
                context: Box::new(Context { callback, root: 0 }),
                port: ptr::null_mut(),
                notifier: 0,
                source: ptr::null(),
                run_loop: ptr::null(),
                attached: false,
            };
            // All output pointers refer to owned writable fields. The callback
            // cannot run until its source is attached and this thread pumps.
            registration.context.root = unsafe {
                IORegisterForSystemPower(
                    (&mut *registration.context as *mut Context).cast(),
                    &mut registration.port,
                    notified,
                    &mut registration.notifier,
                )
            };
            if registration.context.root == 0
                || registration.port.is_null()
                || registration.notifier == 0
            {
                return Err(fault("lifecycle_registration_failed"));
            }
            registration.source = unsafe { IONotificationPortGetRunLoopSource(registration.port) };
            registration.run_loop = unsafe { CFRunLoopGetCurrent() };
            if registration.source.is_null() || registration.run_loop.is_null() {
                return Err(fault("lifecycle_source_failed"));
            }
            unsafe {
                CFRunLoopAddSource(
                    registration.run_loop,
                    registration.source,
                    kCFRunLoopDefaultMode,
                );
            }
            registration.attached = true;
            Ok(Box::new(registration))
        }
    }

    impl NotificationRegistration for Registration {
        fn poll(&mut self) -> Result<(), Fault> {
            // Polling is confined to this dedicated OS thread, never the main/UI
            // thread. Delivery runs elsewhere, so a slow observer cannot delay ACKs.
            let result = unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, PUMP_INTERVAL.as_secs_f64(), 1)
            };
            if result == 1 || result == 2 {
                return Err(fault("lifecycle_runloop_stopped"));
            }
            Ok(())
        }

        fn stop(&mut self) -> Result<(), Fault> {
            let mut failure = None;
            if self.attached {
                unsafe {
                    CFRunLoopRemoveSource(self.run_loop, self.source, kCFRunLoopDefaultMode);
                }
                self.attached = false;
            }
            if self.notifier != 0 {
                if unsafe { IODeregisterForSystemPower(&mut self.notifier) } != 0 {
                    failure = Some(fault("lifecycle_deregistration_failed"));
                }
                self.notifier = 0;
            }
            if self.context.root != 0 {
                if unsafe { IOServiceClose(self.context.root) } != 0 && failure.is_none() {
                    failure = Some(fault("lifecycle_connection_close_failed"));
                }
                self.context.root = 0;
            }
            if !self.port.is_null() {
                unsafe { IONotificationPortDestroy(self.port) };
                self.port = ptr::null_mut();
                self.source = ptr::null();
            }
            match failure {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }

    impl Drop for Registration {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }
}
// end lifecycle.rs
