// telemetry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owner-only, explicitly authorized forwarding; no ambient credentials or default destination.

use crate::contract::{Action, Error, Fault, Power, Registry};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Authorization belongs to the host; constructing Authorized requires actual prior consent.
/// No configuration is loaded from environment variables, files or native preferences.
#[derive(Default)]
pub enum Configuration {
    /// No forwarding authorization/destination has been established.
    #[default]
    Unconfigured,
    /// Forwarding is explicitly disabled, not successfully delivered.
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

pub(crate) struct Worker;

pub(crate) fn install(actions: &Registry, configuration: Configuration) -> Worker {
    let code = match configuration {
        Configuration::Unconfigured => "telemetry_unconfigured",
        Configuration::Disabled => "telemetry_disabled",
        Configuration::Authorized(_) => "telemetry_transport_unavailable",
    };
    let reported = Arc::new(AtomicBool::new(false));
    macro_rules! watch {
        ($($action:ty),+ $(,)?) => { $(unavailable::<$action>(actions, reported.clone(), code);)+ };
    }
    watch!(
        Power::Session::Query,
        Power::Session::TurnOn,
        Power::Session::TurnOff,
        Power::Session::Toggle,
        Power::Session::Quit,
        Error::Native::Assertions::Occurred,
        Error::Native::Activity::Occurred,
        Error::Dispatch::Validation::Occurred,
        Error::Ipc::Protocol::Occurred,
        Error::Log::Storage::Occurred,
    );
    Worker
}

fn unavailable<A: Action>(actions: &Registry, reported: Arc<AtomicBool>, code: &'static str) {
    actions.observe_action::<A>(move |_, events| {
        if !reported.swap(true, Ordering::AcqRel) {
            events.error::<Error::Telemetry::Configuration::Occurred>(Fault::new(
                code,
                "Actions forwarding is not configured and active; no event was delivered.",
                true,
            ));
        }
    });
}
// end telemetry.rs
