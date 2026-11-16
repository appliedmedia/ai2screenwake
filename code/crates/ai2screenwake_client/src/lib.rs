// lib.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Single-owner Actions runtime; native resources remain an injected boundary.

pub use ai2screenwake_backend::contract;
#[cfg(unix)]
pub mod ipc;
mod projection;
pub mod runtime;
// end lib.rs
