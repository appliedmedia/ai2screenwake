// corners.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Permission-aware native input capability, separate from idle-cancellation evidence.

use super::{Apply, Fault};
use serde::{Deserialize, Serialize};

/// Only current, fully confirmed owner state may authorize the installed source.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerAuthorization {
    /// Ordered owner projection; older deliveries cannot restore authorization.
    pub revision: u64,
    /// Exact native operation, absent during pending, off, faults and shutdown.
    pub request: Option<Apply>,
}

/// Loss of an installed source, even when both idle assertions remain owned.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerLoss {
    /// Acquisition whose corner protection is no longer available.
    pub request: Apply,
    /// Actual failed permission, configuration or native-source condition.
    pub fault: Fault,
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
// end corners.rs
