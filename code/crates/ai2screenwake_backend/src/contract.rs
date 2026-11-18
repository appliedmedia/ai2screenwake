// contract.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Canonical serializable projections shared by native and MCP consumers.

use serde::{Deserialize, Serialize, ser::SerializeMap};

#[allow(non_snake_case)]
#[path = "contract/ui.rs"]
pub mod Ui;
mod identity;
mod journal;
mod registry;
mod session;

pub use identity::{EntityId, IdGeneration};
pub use journal::{ActionJournal, JournalEntry, JournalHistory, LOG_MEMORY_LIMIT};
pub use registry::{
    Action, CorrelationId, Emitter, KvEnvelope, ObservationSink, PreparedAction, Registry,
    RegistryBuilder, StartupError, Subscription, WeakEmitter,
};
pub use session::{
    Abandoned, ControlResult, FailedApply, InterruptionObserved, OwnerClosed, OwnerControl,
    OwnerLocation, RejectedApply, Settlement, SleepPhase, SuppressionKind, SuppressionRequest,
};

/// Observed protection, never a speculative acknowledgement of requested intent.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[cfg_attr(feature = "mcp-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Status {
    /// True/false only after native resource convergence; otherwise unknown.
    pub is_on: Option<bool>,
    /// Latest admitted request.
    pub desired_on: bool,
    /// Human-readable observed phase.
    pub state: State,
    /// Monotonic projection revision within one owner session.
    pub revision: u64,
    /// Entity identity of the outstanding native apply, if any.
    pub operation_id: Option<String>,
    /// Genuine observed protected-idle age, unavailable until sampled.
    #[serde(default)]
    pub protected_idle_duration_seconds: Option<u64>,
    /// Only actual matching native cancellation evidence belongs here.
    #[serde(default)]
    pub protected_idle_suppressed_events: Vec<SuppressedEvent>,
    /// True only when older current-episode entries have been omitted.
    #[serde(default)]
    pub suppressed_events_truncated: bool,
}

impl Serialize for Status {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map =
            serializer.serialize_map(Some(if self.is_on == Some(true) { 8 } else { 5 }))?;
        map.serialize_entry("is_on", &self.is_on)?;
        map.serialize_entry("desired_on", &self.desired_on)?;
        map.serialize_entry("state", &self.state)?;
        map.serialize_entry("revision", &self.revision)?;
        map.serialize_entry("operation_id", &self.operation_id)?;
        if self.is_on == Some(true) {
            map.serialize_entry(
                "protected_idle_duration_seconds",
                &self.protected_idle_duration_seconds,
            )?;
            map.serialize_entry(
                "protected_idle_suppressed_events",
                &self.protected_idle_suppressed_events,
            )?;
            map.serialize_entry(
                "suppressed_events_truncated",
                &self.suppressed_events_truncated,
            )?;
        }
        map.end()
    }
}
impl Status {
    /// A newly created owner has no native resources and always starts off.
    pub fn off() -> Self {
        Self {
            is_on: Some(false),
            desired_on: false,
            state: State::Off,
            revision: 0,
            operation_id: None,
            protected_idle_duration_seconds: None,
            protected_idle_suppressed_events: Vec::new(),
            suppressed_events_truncated: false,
        }
    }
}

/// The UI renders these phases without deriving resource state itself.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "mcp-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Confirmed zero resources.
    Off,
    /// Both resources confirmed for the current intent.
    On,
    /// Native work has not converged.
    Pending,
    /// An explicit fault prevents claiming convergence.
    Error,
}

/// One verified native cancellation, captured at occurrence rather than query time.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "mcp-schema", derive(schemars::JsonSchema))]
pub struct SuppressedEvent {
    /// Verified eligible cancellation category, never an inferred deadline.
    pub kind: String,
    /// Actual UTC Unix milliseconds captured with the native receipt.
    pub epoch_ms: u64,
    /// Event-time monotonic offset from its protected-idle anchor.
    pub minutes_into_protected_idle: Minutes,
}

/// Nonnegative finite event-time minutes; invalid floating-point values never reach JSON.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Minutes(f64);

impl TryFrom<f64> for Minutes {
    type Error = &'static str;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err("event minutes must be finite and nonnegative")
        }
    }
}

impl From<Minutes> for f64 {
    fn from(value: Minutes) -> Self {
        value.0
    }
}

/// Empty object used by parameterless typed controls, never a positional verb argument.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// One desired state change after admission by the owner.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    /// Requested enabled state.
    pub enabled: bool,
}

/// Native acquisition/release request; only the owner creates operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Apply {
    /// Process lifetime; never restored from replay.
    pub session_id: EntityId,
    /// Monotonic desired-intent version.
    pub generation: u64,
    /// Stable identity of this physical apply.
    pub operation_id: EntityId,
    /// True acquires both resources, false releases all retained resources.
    pub enabled: bool,
}

/// Paired clocks captured together at the native boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClockSample {
    /// Real Unix UTC milliseconds, not a file timestamp.
    pub epoch_ms: u64,
    /// Milliseconds on the native continuous monotonic clock.
    pub monotonic_ms: u64,
}

/// Native completion status; failed cleanup must retain resource IDs in native ownership.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Requested resource transition completed.
    Applied,
    /// Requested transition did not complete.
    Failed,
}

/// Native resource accounting with its original operation and occurrence clocks.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionsObserved {
    /// Exact operation being completed.
    #[serde(flatten)]
    pub request: Apply,
    /// Number of resources still held, including failed-release IDs.
    pub held_count: u8,
    /// Success must also match enabled and held_count.
    pub outcome: Outcome,
    /// Stable fault codes, never raw native error chains.
    pub fault_codes: Vec<String>,
    /// Time of native confirmation, not observer delivery.
    pub clock: ClockSample,
}

/// A fresh query sample is bound to an owner, generation, interval and request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdleRequest {
    /// Current owner lifetime.
    pub session_id: EntityId,
    /// Current desired version.
    pub generation: u64,
    /// Native acquisition interval; current idle/reset anchors remain core-owned.
    pub episode_id: EntityId,
    /// Unique sample request.
    pub sample_id: EntityId,
}

/// Native lifecycle-source health, ordered independently of protection generations.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleAvailability {
    /// Strictly increasing native source-health revision, including monitor replacements.
    pub revision: u64,
    /// True only after the required sleep/wake source has successfully registered.
    pub available: bool,
}

/// Genuine OS idle-age observation; absence requires an explicit fault.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdleObserved {
    /// Original guarded request.
    pub request: IdleRequest,
    /// Paired clocks captured with the OS idle sample.
    pub clock: ClockSample,
    /// Genuine OS idle age in milliseconds; never synthesized from enable time.
    pub idle_age_ms: Option<u64>,
    /// Native sampling failure, if any.
    pub fault: Option<Fault>,
}

/// Reserved cancellation evidence contract; production has no synthetic event source.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationObserved {
    /// Guarded owner/interval identity.
    pub request: IdleRequest,
    /// Actual matched native request identity.
    pub native_request_id: EntityId,
    /// Actual successful cancellation acknowledgement identity.
    pub cancellation_id: EntityId,
    /// Clocks at occurrence, not at a later query.
    pub clock: ClockSample,
    /// Genuine OS idle age captured atomically with the cancellation clocks.
    pub idle_age_ms: u64,
    /// Actual native cancellation acknowledgement, never inferred from held assertions.
    pub cancelled: bool,
}

/// Canonical error payload; only framework emission fills the triggering genus.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    /// Stable machine-readable fault.
    pub fault_code: String,
    /// Short, non-sensitive message for observers.
    pub message: String,
    /// Whether retry or continued operation is possible.
    pub recoverable: bool,
    /// Original failed action, populated by the dispatch framework when triggered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_genus: Option<String>,
}

/// Read-only log presentation record; display text is never a dispatch address.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    /// Zero-based canonical append position.
    pub sequence: u64,
    /// Framework-formatted action/correlation label, with no protected payloads.
    pub display: String,
}

/// Native Show Log projection. The canonical log remains owned by the registry.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogSnapshot {
    /// Ordered metadata view, including the query that requested it.
    pub entries: Vec<LogRecord>,
    /// The bounded recent window omits earlier retained/rotated history.
    #[serde(default)]
    pub earlier_entries_omitted: bool,
    /// A failed durable append is never presented as a healthy history view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_error: Option<Fault>,
    /// Actual absolute path of the derived readable snapshot, never a replay source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readable_path: Option<String>,
}

impl LogSnapshot {
    /// Bounded metadata-only text shared by the private export and native viewer.
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let mut text = String::new();
        if let Some(path) = &self.readable_path {
            writeln!(text, "{path}").expect("String writes cannot fail");
        }
        let omitted = self.entries.len().saturating_sub(200);
        for entry in self.entries.iter().skip(omitted).rev() {
            writeln!(text, "{}  {}", entry.sequence, entry.display)
                .expect("String writes cannot fail");
        }
        if self.earlier_entries_omitted {
            text.push_str("Earlier action history is omitted from this view.\n");
        }
        if omitted > 0 {
            writeln!(
                text,
                "{omitted} additional recent log entries are not shown."
            )
            .expect("String writes cannot fail");
        }
        if self.storage_error.is_some() {
            text.push_str("Some history could not be saved; this view may be incomplete.\n");
        }
        text
    }
}

impl Fault {
    /// Construct a source-less error; an emitter attaches triggering correlation.
    pub fn new(code: &str, message: &str, recoverable: bool) -> Self {
        Self {
            fault_code: code.into(),
            message: message.into(),
            recoverable,
            failed_genus: None,
        }
    }
}

macro_rules! action {
    ($name:ident, $payload:ty, $command:expr) => {
        #[doc = concat!("Typed identity for the ", stringify!($name), " action in this hierarchy.")]
        #[derive(Clone, Copy, Debug)]
        pub struct $name;
        impl crate::contract::registry::sealed::Sealed for $name {}
        impl crate::contract::Action for $name {
            type Payload = $payload;
            const COMMAND: bool = $command;
        }
    };
}

/// Semantic hierarchy; the framework derives each static route from the Rust type path.
#[allow(non_snake_case)]
pub mod Power {
    /// User controls and observed projections.
    pub mod Session {
        /// Session protection controls, projections and request lifetime.
        pub mod Protection {
            action!(TurnOn, super::super::super::Empty, true);
            action!(TurnOff, super::super::super::Empty, true);
            action!(Toggle, super::super::super::Empty, true);
            action!(Query, super::super::super::Empty, true);
            action!(Quit, super::super::super::Empty, true);
            action!(Closed, super::super::super::OwnerClosed, false);
            action!(Changed, super::super::super::Status, false);
            action!(Snapshot, super::super::super::Status, false);
            action!(Settled, super::super::super::Settlement, false);
            action!(Expired, super::super::super::CorrelationId, false);
            action!(Abandoned, super::super::super::Abandoned, false);
        }
    }
    /// Native resource operations.
    pub mod Assertions {
        /// Acquisition, release and accounting of native assertion resources.
        pub mod Resources {
            action!(Apply, super::super::super::Apply, true);
            action!(Observed, super::super::super::AssertionsObserved, false);
            action!(Rejected, super::super::super::RejectedApply, false);
            action!(Failed, super::super::super::FailedApply, false);
        }
    }
    /// Genuine idle observations.
    pub mod Activity {
        /// Genuine system input-age and sleep/wake availability.
        pub mod System {
            action!(Watch, super::super::super::Empty, true);
            action!(
                Availability,
                super::super::super::LifecycleAvailability,
                false
            );
            action!(Sample, super::super::super::IdleRequest, true);
            action!(Observed, super::super::super::IdleObserved, false);
            action!(
                Interrupted,
                super::super::super::InterruptionObserved,
                false
            );
        }
    }
    /// Conditional, proof-only cancellation evidence.
    pub mod Suppression {
        /// Actual requests and confirmed cancellation evidence, never inferred timers.
        pub mod Evidence {
            action!(Requested, super::super::super::SuppressionRequest, false);
            action!(Recorded, super::super::super::CancellationObserved, false);
        }
    }
}

/// A local diagnostic acknowledgement, never an analytics payload or power-state result.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ForwardingReceipt {
    /// Original Actions correlation, retained locally only.
    pub source: CorrelationId,
    /// Destination that actually accepted the redacted event.
    pub target: ForwardingTarget,
}

/// Independently configured forwarding destinations.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardingTarget {
    /// Configured PostHog collector.
    Analytics,
    /// Configured unrecoverable-error collector.
    BugReport,
}

/// Local forwarding diagnostics are excluded from the forwarding allowlist.
#[allow(non_snake_case)]
pub mod Telemetry {
    /// Real terminal transport outcomes only, never queued-as-delivered acknowledgements.
    pub mod Delivery {
        /// A terminal receipt for a configured forwarding destination.
        pub mod Forwarding {
            action!(Confirmed, super::super::super::ForwardingReceipt, false);
        }
    }
    /// Owned forwarding worker lifetime.
    pub mod Session {
        /// Disposal of the owned forwarding worker.
        pub mod Worker {
            action!(Stopped, super::super::super::Empty, false);
        }
    }
}

/// Error actions have their own root, never a verb-swapped failed command.
#[allow(non_snake_case)]
pub mod Error {
    /// Native resource and observation faults.
    pub mod Native {
        /// Resource-accounting failures.
        pub mod Assertions {
            action!(Occurred, super::super::super::Fault, false);
        }
        /// Idle-source failures.
        pub mod Activity {
            action!(Occurred, super::super::super::Fault, false);
        }
    }
    /// Invalid envelopes and handler failures.
    pub mod Dispatch {
        /// Validation failures observed on the bus.
        pub mod Validation {
            action!(Occurred, super::super::super::Fault, false);
        }
    }
    /// Local transport failures, never account or network-service authentication.
    pub mod Ipc {
        /// Framing, peer and operation faults.
        pub mod Protocol {
            action!(Occurred, super::super::super::Fault, false);
        }
    }
    /// Durable history failures.
    pub mod Log {
        /// Canonical append/replay failures.
        pub mod Storage {
            action!(Occurred, super::super::super::Fault, false);
        }
    }
    /// Optional forwarding must never change protection or recursively forward its own failures.
    pub mod Telemetry {
        /// Explicit missing, disabled, invalid or unavailable forwarding configuration.
        pub mod Configuration {
            action!(Occurred, super::super::super::Fault, false);
        }
        /// A terminal analytics or bug-report delivery failure.
        pub mod Transport {
            action!(Occurred, super::super::super::Fault, false);
        }
        /// Bounded queue rejection or shutdown abandonment.
        pub mod Buffer {
            action!(Occurred, super::super::super::Fault, false);
        }
    }
}

/// Native-only log observation; this does not add an MCP tool.
#[allow(non_snake_case)]
pub mod Log {
    /// Read-only current-owner diagnostic history.
    pub mod Session {
        /// Read-only canonical history and its bounded diagnostic projection.
        pub mod History {
            action!(Query, super::super::super::Empty, true);
            action!(Snapshot, super::super::super::LogSnapshot, false);
        }
    }
}
// end contract.rs
