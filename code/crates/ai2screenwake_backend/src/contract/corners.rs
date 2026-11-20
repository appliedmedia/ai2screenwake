// corners.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Permission-aware native input capability, separate from idle-cancellation evidence.

use super::{Apply, EntityId, Fault, Ui};
use serde::{Deserialize, Serialize};

/// Current basic owner admission, also required before optional corner activation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerAuthorization {
    /// Ordered owner projection; older deliveries cannot restore authorization.
    pub revision: u64,
    /// Exact current On acquisition, including its internal activation-pending phase.
    ///
    /// Absent for other pending work, Off, source faults and shutdown.
    pub request: Option<Apply>,
}

/// Optional source denial, activation failure or loss, independently of basic assertions.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerLoss {
    /// Physical optional-filter installation; absent if no source could be installed.
    pub installation_id: Option<EntityId>,
    /// Acquisition whose corner protection is no longer available.
    pub request: Apply,
    /// Actual failed permission, configuration or native-source condition.
    pub fault: Fault,
}

/// Owner-validated optional capability projection, separate from basic protection truth.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerStatus {
    /// Current optional limitation; absent after cleanup or replacement acquisition.
    pub fault: Option<Fault>,
}

/// An actual original pointer event was deflected, not a cancelled sleep token.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerFiltered {
    /// Current authorized acquisition.
    pub request: Apply,
    /// Position in the validated corner configuration, without input coordinates.
    pub corner: u8,
}

/// Optional-only reconciliation for the exact still-admitted acquisition.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerRefresh {
    /// Explicit setup-window lifetime authorizing this recheck.
    pub session: u64,
    /// Current basic owner admission; never an instruction to acquire assertions.
    pub authorization: CornerAuthorization,
}

/// Native activation outcome, distinct from Accessibility trust and basic On.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerRefreshed {
    /// Actual retained/replacement installation, independently of the owner acquisition.
    pub installation_id: Option<EntityId>,
    /// Original setup and exact acquisition.
    pub refresh: CornerRefresh,
    /// Actual configured filter activation, not merely a granted permission.
    pub ready: bool,
    /// Optional failure, already dispatched through the named error genus.
    pub fault: Option<Fault>,
}

/// Native nonprompting Accessibility check inputs and outcomes.
pub type AccessibilityCheck = Ui::SetupSession;
// end corners.rs
