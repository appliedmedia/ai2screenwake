// shutdown.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Private foreign-delivery lifetime gate, shared with the executable UI requirements.

use super::{NativeIntent, NativeObserver};
use ai2screenwake_backend::contract::{Empty, Error, Fault, Log, Registry, Ui};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    time::Duration,
};

#[derive(Default)]
pub(super) struct Signals {
    pub disposed: AtomicBool,
    pub overflow: AtomicBool,
    pub cleaning: AtomicBool,
    pub cleanup_failed: AtomicBool,
}

pub(super) fn observe_cleanup(bus: &Registry, signals: Arc<Signals>) {
    bus.observe_action::<Error::Ipc::Protocol::Occurred>(move |failure, _| {
        if signals.cleaning.load(Ordering::Acquire)
            && matches!(
                failure.fault_code.as_str(),
                "socket_cleanup" | "socket_changed"
            )
        {
            signals.cleanup_failed.store(true, Ordering::Release);
        }
    });
}

pub(super) fn deliver_after_disposal(
    disposal: Receiver<()>,
    commands: Receiver<NativeIntent>,
    diagnostics: Registry,
    observer: Arc<dyn NativeObserver>,
    signals: Arc<Signals>,
    packet: Ui::Projection,
) {
    let mut complete = false;
    loop {
        if !complete {
            complete = disposal.recv_timeout(Duration::from_secs(2)).is_ok();
        }
        if complete {
            signals.cleaning.store(false, Ordering::Release);
        }
        if complete && !signals.cleanup_failed.load(Ordering::Acquire) {
            observer.observed(packet.text, packet.flags);
            return;
        }
        diagnostics.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
            "ui_shutdown_failed",
            "The closed private owner has not finished disposal. Quit can retry.",
            true,
        ));
        if !shutdown_retry(&commands, &diagnostics) {
            return;
        }
    }
}

pub(super) fn shutdown_retry(commands: &Receiver<NativeIntent>, diagnostics: &Registry) -> bool {
    loop {
        match commands.recv() {
            Ok(NativeIntent::Quit) => return true,
            Ok(NativeIntent::ShowLog) => diagnostics.emit::<Log::Session::History::Query>(Empty {}),
            Ok(NativeIntent::CloseLog) => diagnostics.emit::<Ui::Log::Window::Dismissed>(Empty {}),
            Ok(_) => diagnostics.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "owner_closing",
                "Only diagnostics and shutdown retry are available.",
                true,
            )),
            Err(_) => return false,
        }
    }
}
// end shutdown.rs
