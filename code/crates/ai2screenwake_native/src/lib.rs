// lib.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native callback contracts and explicitly owned macOS power resources.

pub mod activity;
pub mod contracts;
pub mod corners;
#[cfg(target_os = "macos")]
pub mod identity;
#[cfg(target_os = "macos")]
pub mod launch;
pub mod lifecycle;
pub mod power;

pub use contracts::NativePower;
// end lib.rs
