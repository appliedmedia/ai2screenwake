// session.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request settlement and injected local-owner/native lifecycle boundaries.

use super::{Action, Apply, ClockSample, CorrelationId, Empty, EntityId, Fault, Power, Status};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Exactly the three remote controls, selected by action identity rather than tool strings.
pub trait OwnerControl: Action<Payload = Empty> {}
impl OwnerControl for Power::Session::Query {}
impl OwnerControl for Power::Session::TurnOn {}
impl OwnerControl for Power::Session::TurnOff {}

/// The canonical eventual control result; emit never returns this value.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResult {
    /// Last independently observed state, absent only before an owner is available.
    pub status: Option<Status>,
    /// Explicit failure; presence is the MCP isError condition.
    pub error: Option<Fault>,
}

/// One terminal outcome for a request, delivered as an ordinary logged event.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    /// Opaque bus request identity, not a stored entity or route.
    pub correlation: CorrelationId,
    /// Eventual observed result.
    pub result: ControlResult,
}

/// Closing is permanent for this owner; this event proves explicit zero-resource cleanup.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerClosed {
    /// Owner lifetime whose admission fence remains closed.
    pub session_id: EntityId,
    /// Latest converged Off generation.
    pub generation: u64,
}

/// Transport disconnection releases notification state, never admitted physical intent.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Abandoned {
    /// At most the peer's 32 outstanding requests.
    pub correlations: Vec<CorrelationId>,
}

/// Actual native system-sleep transition; ordinary inactivity is not an interruption.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SleepPhase {
    /// Native system notification that real sleep is beginning.
    WillSleep,
    /// Native system notification that the system actually woke.
    DidWake,
}

/// Native sleep/wake evidence tied to the still-owned physical assertion operation.
///
/// Emit Power::Activity::Interrupted through the captured native continuation;
/// NativePower itself is unchanged and no listener must retain a strong emitter.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InterruptionObserved {
    /// Exact active acquisition, including owner/generation/operation identities.
    pub request: Apply,
    /// Paired actual notification clocks.
    pub clock: ClockSample,
    /// Actual native sleep/wake phase.
    pub phase: SleepPhase,
    /// Real retained assertion IDs, not an optimistic desired count.
    pub held_count: u8,
}

/// Native-injected login-session location; deliberately not serializable into logs.
#[derive(Clone, Debug)]
pub struct OwnerLocation {
    /// Absolute canonical directory supplied by native identity discovery.
    pub directory: PathBuf,
    /// Expected effective same-user UID, checked against the actual OS peer.
    pub uid: u32,
}
// end session.rs
