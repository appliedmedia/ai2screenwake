// tools.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! MCP names and wire wrapping; domain state and its schema stay in the contract.

use crate::contract::ControlResult;
use rmcp::{
    ErrorData,
    model::{CallToolResult, ErrorCode, JsonObject, Tool},
};
use serde_json::json;
use std::sync::Arc;

/// Closed transport selection; the owner still receives typed Actions identities.
pub(super) enum Control {
    Query,
    TurnOn,
    TurnOff,
}

impl Control {
    pub(super) fn parse(name: &str) -> Result<Self, ErrorData> {
        match name {
            "is_on" => Ok(Self::Query),
            "turn_on" => Ok(Self::TurnOn),
            "turn_off" => Ok(Self::TurnOff),
            _ => Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "Unknown ScreenWake tool.",
                None,
            )),
        }
    }
}

pub(super) fn definitions(output_schema: Arc<JsonObject>) -> Vec<Tool> {
    [
        ("is_on", "Am I on?"),
        ("turn_on", "Turn on"),
        ("turn_off", "Turn off"),
    ]
    .into_iter()
    .map(|(name, description)| {
        let mut input = JsonObject::new();
        input.insert("type".into(), json!("object"));
        input.insert("properties".into(), json!({}));
        input.insert("additionalProperties".into(), json!(false));
        Tool::new(name, description, input).with_raw_output_schema(output_schema.clone())
    })
    .collect()
}

pub(super) fn result(outcome: ControlResult) -> Result<CallToolResult, ErrorData> {
    let failed = outcome.error.is_some();
    let value = ai2screenwake_backend::mcp::tool_output(&outcome)
        .map_err(|_| ErrorData::internal_error("Cannot encode the observed owner result.", None))?;
    Ok(if failed {
        CallToolResult::structured_error(value)
    } else {
        CallToolResult::structured(value)
    })
}
// end tools.rs
