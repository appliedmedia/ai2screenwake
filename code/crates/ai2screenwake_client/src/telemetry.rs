// telemetry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owner-only, explicitly authorized forwarding; no ambient credentials or default destination.

use crate::contract::{Action, Error, Fault, Power, Registry};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Authorization belongs to the host; constructing Authorized requires actual prior consent.
/// No configuration is loaded from environment variables, files or native preferences.
#[derive(Default)]
pub enum Configuration {
    /// Intentionally inactive: no authorization, sending, delivery claim or attention fault.
    #[default]
    Unconfigured,
    /// Intentionally inactive by explicit choice, never reported as delivered.
    Disabled,
    /// Only these explicitly authorized destinations may receive redacted metadata.
    Authorized(Destinations),
}

/// Explicit destinations, deliberately without Debug/Serialize to avoid exposing a project key.
pub struct Destinations {
    /// Optional authorized analytics destination.
    pub analytics: Option<AnalyticsDestination>,
    /// Optional authorized exact /bug-report URL.
    pub bug_report_url: Option<String>,
}

/// PostHog requires an explicit origin and nonblank project key; neither has a default.
pub struct AnalyticsDestination {
    /// HTTPS origin, or an explicitly injected literal-loopback HTTP test origin.
    pub host: String,
    /// Used only for SDK authentication, never Action metadata or diagnostics.
    pub project_key: String,
}

#[cfg(feature = "telemetry")]
mod transport;
#[cfg(feature = "telemetry")]
pub(crate) use transport::Worker;

#[cfg(not(feature = "telemetry"))]
pub(crate) struct Worker;
#[cfg(not(feature = "telemetry"))]
impl Worker {
    pub(crate) fn start(&mut self, _: &Registry) {}
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionName {
    Query,
    TurnOn,
    TurnOff,
    Toggle,
    Quit,
    NativeAssertionsFailure,
    NativeActivityFailure,
    DispatchFailure,
    IpcFailure,
    StorageFailure,
}

#[derive(Clone, Serialize)]
struct Metadata {
    action: ActionName,
    #[serde(skip_serializing_if = "Option::is_none")]
    recoverable: Option<bool>,
}

enum Mode {
    Inactive,
    Unavailable {
        reported: AtomicBool,
        code: &'static str,
    },
    #[cfg(feature = "telemetry")]
    Active(Arc<transport::Queue>),
}

impl Mode {
    fn unavailable(code: &'static str) -> Self {
        Self::Unavailable {
            reported: AtomicBool::new(false),
            code,
        }
    }

    fn forward(&self, metadata: Metadata, events: &crate::contract::Emitter) {
        match self {
            Self::Inactive => {}
            Self::Unavailable { reported, code } => {
                if !reported.swap(true, Ordering::AcqRel) {
                    events.error::<Error::Telemetry::Configuration::Occurred>(Fault::new(
                        code,
                        "Actions forwarding is not active; no event was delivered.",
                        true,
                    ));
                }
            }
            #[cfg(feature = "telemetry")]
            Self::Active(queue) => queue.submit(metadata, events),
        }
        #[cfg(not(feature = "telemetry"))]
        let _ = metadata;
    }
}

pub(crate) fn install(actions: &Registry, configuration: Configuration) -> Worker {
    #[cfg(feature = "telemetry")]
    let (mode, worker) = match configuration {
        Configuration::Unconfigured | Configuration::Disabled => {
            (Mode::Inactive, Worker::default())
        }
        Configuration::Authorized(destinations) => match transport::configure(destinations) {
            Ok((queue, worker)) => (Mode::Active(queue), worker),
            Err(_) => (
                Mode::unavailable("telemetry_invalid_configuration"),
                Worker::default(),
            ),
        },
    };
    #[cfg(not(feature = "telemetry"))]
    let (mode, worker) = (
        match configuration {
            Configuration::Unconfigured | Configuration::Disabled => Mode::Inactive,
            Configuration::Authorized(_) => Mode::unavailable("telemetry_feature_unavailable"),
        },
        Worker,
    );
    let mode = Arc::new(mode);
    macro_rules! control {
        ($action:ident) => {
            watch::<Power::Session::$action>(actions, mode.clone(), ActionName::$action, |_| None);
        };
    }
    control!(Query);
    control!(TurnOn);
    control!(TurnOff);
    control!(Toggle);
    control!(Quit);
    macro_rules! fault {
        ($action:ty, $name:ident) => {
            watch::<$action>(actions, mode.clone(), ActionName::$name, |fault| {
                Some(fault.recoverable)
            });
        };
    }
    fault!(Error::Native::Assertions::Occurred, NativeAssertionsFailure);
    fault!(Error::Native::Activity::Occurred, NativeActivityFailure);
    fault!(Error::Dispatch::Validation::Occurred, DispatchFailure);
    fault!(Error::Ipc::Protocol::Occurred, IpcFailure);
    fault!(Error::Log::Storage::Occurred, StorageFailure);
    #[cfg(feature = "telemetry")]
    if let Mode::Active(queue) = mode.as_ref() {
        let queue = queue.clone();
        actions.observe_action::<Power::Session::Closed>(move |_, events| {
            queue.close(Some(events.downgrade_metadata()));
        });
    }
    worker
}

fn watch<A: Action>(
    actions: &Registry,
    mode: Arc<Mode>,
    name: ActionName,
    recoverable: fn(A::Payload) -> Option<bool>,
) {
    actions.observe_action::<A>(move |payload, events| {
        mode.forward(
            Metadata {
                action: name,
                recoverable: recoverable(payload),
            },
            events,
        );
    });
}
// end telemetry.rs
