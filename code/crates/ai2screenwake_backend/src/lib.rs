// lib.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Shared wire contracts; this foundation does not host a network service.

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "mcp-schema")]
#[path = "contract/mcp.rs"]
pub mod mcp;
// end lib.rs
