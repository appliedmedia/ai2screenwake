// build_info.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Immutable compiler inputs frozen by the canonical publication invocation.

/// The exact native, MCP and menu version; ordinary Cargo builds are not publications.
pub const VERSION: &str = match option_env!("AI2SW_BUILD_VERSION") {
    Some(value) => value,
    None => "Development",
};

/// Source/configuration/version stamp checked at producer capture and native consumption.
pub const STAMP: &str = match option_env!("AI2SW_BUILD_STAMP") {
    Some(value) => value,
    None => "AI2SW_BUILD:Development",
};
// end build_info.rs
