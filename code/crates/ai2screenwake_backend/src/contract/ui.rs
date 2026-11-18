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
