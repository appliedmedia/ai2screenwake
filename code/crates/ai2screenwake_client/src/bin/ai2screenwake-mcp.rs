// ai2screenwake-mcp.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Local stdio entry point; no owner override, daemon, or power resources.

use ai2screenwake_client::mcp::{McpService, owner_connection::OwnerAdapter};
use std::{process::ExitCode, time::Duration};

fn main() -> ExitCode {
    if std::env::args().skip(1).eq(["--build-info"]) {
        println!(
            "{}",
            serde_json::json!({
                "version": ai2screenwake_client::build_info::VERSION,
                "stamp": ai2screenwake_client::build_info::STAMP,
            })
        );
        return ExitCode::SUCCESS;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("AI2ScreenWake MCP runtime could not start.");
            return ExitCode::FAILURE;
        }
    };
    let outcome = runtime.block_on(async {
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
                eprintln!("AI2ScreenWake MCP transport failed ({:?}).", error.kind());
                ExitCode::FAILURE
            }
        }
    });
    // Darwin's Tokio stdin read cannot be cancelled. Transport/owner cleanup has
    // finished; do not let an idle blocking stdin worker prevent this process exit.
    runtime.shutdown_timeout(Duration::from_secs(1));
    outcome
}
// end ai2screenwake-mcp.rs
