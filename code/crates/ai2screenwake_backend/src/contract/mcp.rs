// mcp.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Canonical public tool projection and schema, independent of any MCP SDK.

use crate::contract::{ControlResult, Fault, Minutes, Status};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::borrow::Cow;

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolFault<'a> {
    fault_code: &'a str,
    message: &'a str,
    recoverable: bool,
}

impl<'a> From<&'a Fault> for ToolFault<'a> {
    fn from(fault: &'a Fault) -> Self {
        Self {
            fault_code: &fault.fault_code,
            message: &fault.message,
            recoverable: fault.recoverable,
        }
    }
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolOutput<'a> {
    status: &'a Option<Status>,
    error: Option<ToolFault<'a>>,
}

impl JsonSchema for Minutes {
    fn schema_name() -> Cow<'static, str> {
        "Minutes".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type": "number", "minimum": 0})
    }
}

/// Serialize actual canonical state while redacting only internal error attribution.
///
/// The owner's `Status::serialize` exclusively decides which On-only fields exist;
/// errors keep exactly their public code, short message and recoverability.
pub fn tool_output(result: &ControlResult) -> Result<Value, serde_json::Error> {
    serde_json::to_value(ToolOutput {
        status: &result.status,
        error: result.error.as_ref().map(ToolFault::from),
    })
}

/// Generate the one tool-output schema from canonical types and serialization rules.
///
/// Schema derivation models deserialization, where optional/default fields may be
/// absent. Public serialization always emits the five base fields, and emits all
/// three idle fields if and only if `is_on` is true.
pub fn tool_output_schema() -> Map<String, Value> {
    let mut schema: Value = schemars::schema_for!(ToolOutput<'static>).into();
    schema["required"] = json!(["status", "error"]);
    let status = &mut schema["$defs"]["Status"];
    status["required"] = json!(["is_on", "desired_on", "state", "revision", "operation_id"]);
    status["allOf"] = json!([{
        "if": {"properties": {"is_on": {"const": true}}, "required": ["is_on"]},
        "then": {"required": [
            "protected_idle_duration_seconds",
            "protected_idle_suppressed_events",
            "suppressed_events_truncated"
        ]},
        "else": {"properties": {
            "protected_idle_duration_seconds": false,
            "protected_idle_suppressed_events": false,
            "suppressed_events_truncated": false
        }}
    }]);
    match schema {
        Value::Object(schema) => schema,
        _ => unreachable!("a derived tool output is an object schema"),
    }
}
// end mcp.rs
