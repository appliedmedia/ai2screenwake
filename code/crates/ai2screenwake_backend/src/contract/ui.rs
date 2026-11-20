// ui.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Rendering-only native menu observations; never a mirrored domain-state schema.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical render attributes passed unchanged through the generated native seam.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    /// Rust-owned strings/resource names; the shell only lays them out.
    pub text: HashMap<String, String>,
    /// Rendering instructions, never optimistic protection state.
    pub flags: HashMap<String, bool>,
}

/// Native completion of the already logged website-opening effect.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebsiteCompletion {
    /// True only when the native workspace accepted the requested URL.
    pub opened: bool,
}

/// One explicit setup-window lifetime, never a stored entity identity.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupSession {
    /// Monotonic window revision; stale callbacks cannot affect a reopened window.
    pub session: u64,
}

/// Actual workspace acceptance, not an Accessibility grant.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupCompletion {
    /// The window whose native effect completed.
    pub session: u64,
    /// Whether System Settings accepted its URL.
    pub opened: bool,
}

/// Nonprompting native trust observation for an explicit setup lifetime.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccessibilityObservation {
    /// The originating explicit window revision.
    pub session: u64,
    /// Fresh native trust, never inferred from a drag or workspace result.
    pub trusted: bool,
    /// Actual denial or unavailable observation, logged before this outcome.
    pub fault: Option<super::Fault>,
}

/// Fresh nonprompting menu permission observation, correlated by its Actions chain.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerTrust {
    /// Actual target-process Accessibility authorization.
    pub trusted: bool,
    /// A named native denial or unavailable observation, already logged.
    pub fault: Option<super::Fault>,
}

/// Owner-scoped corner preference, distinct from effective basic protection and filter readiness.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CornerControlPresentation {
    /// Orders trust/preference changes without changing basic resource ownership.
    pub revision: u64,
    /// Fresh nonprompting OS truth; unknown and denied both offer Install.
    pub trusted: Option<bool>,
    /// In-memory user preference, default On; never restored from diagnostic replay.
    pub enabled: bool,
}

impl Default for CornerControlPresentation {
    fn default() -> Self {
        Self {
            revision: 0,
            trusted: None,
            enabled: true,
        }
    }
}

/// Rust-owned setup copy and one-shot native effects.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupPresentation {
    /// Explicit window revision, also carried by every native callback.
    pub session: u64,
    /// Whether observation and the instruction window remain requested.
    pub visible: bool,
    /// One-shot System Settings opening, never repeated by ordinary rendering.
    pub open_settings: bool,
    /// Fresh OS trust, independently of native filter readiness.
    pub trusted: Option<bool>,
    /// True only after matching native filter activation.
    pub guard_ready: bool,
    /// Localized, truthful setup-only status.
    pub message: String,
}

/// Explicit opt-in setup; no ordinary control requests a permission challenge.
#[allow(non_snake_case)]
pub mod Accessibility {
    /// Corner-icon queries and user preference changes remain Rust-owned.
    pub mod Control {
        macro_rules! action {
            ($name:ident, $payload:ty, $command:expr) => {
                #[doc = "A typed corner control input or owner observation."]
                pub struct $name;
                impl super::super::super::registry::sealed::Sealed for $name {}
                impl super::super::super::Action for $name {
                    type Payload = $payload;
                    const COMMAND: bool = $command;
                }
            };
        }
        action!(Query, super::super::super::Empty, true);
        action!(Activated, super::super::super::Empty, true);
        action!(Changed, super::super::CornerControlPresentation, false);
    }

    /// The user-owned Accessibility setup window and its observations.
    pub mod Setup {
        macro_rules! action {
            ($name:ident, $payload:ty, $command:expr) => {
                #[doc = "A typed explicit Accessibility setup transition."]
                pub struct $name;
                impl super::super::super::registry::sealed::Sealed for $name {}
                impl super::super::super::Action for $name {
                    type Payload = $payload;
                    const COMMAND: bool = $command;
                }
            };
        }
        action!(Activated, super::super::super::Empty, true);
        action!(Completed, super::super::SetupCompletion, true);
        action!(Check, super::super::SetupSession, true);
        action!(Tick, super::super::SetupSession, false);
        action!(Dismissed, super::super::SetupSession, true);
        action!(Changed, super::super::SetupPresentation, false);
    }
}

/// Explicit user-requested information, never background network activity.
#[allow(non_snake_case)]
pub mod Info {
    /// One fixed public product website.
    pub mod Website {
        macro_rules! event {
            ($name:ident, $payload:ty) => {
                #[doc = "A logged website intent or observed native outcome."]
                pub struct $name;
                impl super::super::super::registry::sealed::Sealed for $name {}
                impl super::super::super::Action for $name {
                    type Payload = $payload;
                    const COMMAND: bool = false;
                }
            };
        }
        event!(Activated, super::super::super::Empty);
        event!(Completed, super::super::WebsiteCompletion);
        event!(Opened, super::super::super::Empty);
    }
}

/// Native menu rendering is an ordinary logged Actions observation.
#[allow(non_snake_case)]
pub mod Menu {
    /// Observed menu presentation, separate from the protection domain.
    pub mod Presentation {
        /// Observed attributes, including terminal-only instructions.
        pub struct Render;

        impl super::super::super::registry::sealed::Sealed for Render {}
        impl super::super::super::Action for Render {
            type Payload = super::super::Projection;
            const COMMAND: bool = false;
        }
    }
}

/// Native preview input is logged separately from observed protection.
#[allow(non_snake_case)]
pub mod Preview {
    /// The actionable large TV, not a second protection state.
    pub mod Control {
        /// Rust chooses and confirms the next face after activation.
        pub struct Activated;

        impl super::super::super::registry::sealed::Sealed for Activated {}
        impl super::super::super::Action for Activated {
            type Payload = super::super::super::Empty;
            const COMMAND: bool = false;
        }
    }
}

/// Auxiliary-window lifetime is rendering-only, never an owner control.
#[allow(non_snake_case)]
pub mod Log {
    /// Lifetime of the auxiliary diagnostic window.
    pub mod Window {
        /// The native read-only log window was dismissed.
        pub struct Dismissed;

        impl super::super::super::registry::sealed::Sealed for Dismissed {}
        impl super::super::super::Action for Dismissed {
            type Payload = super::super::super::Empty;
            const COMMAND: bool = false;
        }
    }
}
// end ui.rs
