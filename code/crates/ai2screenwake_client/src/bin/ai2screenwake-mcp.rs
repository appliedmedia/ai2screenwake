// ai2screenwake-mcp.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Local stdio entry point; no owner override, daemon, or power resources.

use ai2screenwake_client::mcp::{McpService, owner_connection::OwnerAdapter};
use std::process::ExitCode;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let service = McpService::new(
        OwnerAdapter::bundled(),
        ai2screenwake_backend::mcp::tool_output_schema(),
    );
    match service
        .serve_io(tokio::io::stdin(), tokio::io::stdout())
        .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ScreenWake MCP transport failed ({:?}).", error.kind());
            ExitCode::FAILURE
        }
    }
}
// end ai2screenwake-mcp.rs
