// lib.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Single-owner Actions runtime; native resources remain an injected boundary.

pub use ai2screenwake_backend::contract;
#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(unix)]
pub mod ipc;
#[cfg(feature = "mcp")]
pub mod mcp;
mod projection;
pub mod runtime;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!("screenwake");
// end lib.rs
