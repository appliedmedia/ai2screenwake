// mod.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Optional stdio MCP transport over the existing same-user owner Actions port.
//!
//! The transport waits for observed settlements, never a returned core state.
//! Initialization and discovery do not connect, launch, or enable protection.

pub mod owner_connection;
mod protocol;
mod tools;

use owner_connection::OwnerAdapter;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, Implementation, InitializeRequestParams,
        InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams, ProtocolVersion,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use std::{borrow::Cow, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

/// One MCP peer's bounded transport and lazily connected owner collaborator.
pub struct McpService {
    owner: OwnerAdapter,
    output_schema: Arc<JsonObject>,
}

impl McpService {
    /// Inject the owner connector and contract-owned output schema.
    ///
    /// The schema belongs to the backend contract, not a mirrored adapter type.
    pub fn new(owner: OwnerAdapter, output_schema: JsonObject) -> Self {
        Self {
            owner,
            output_schema: Arc::new(output_schema),
        }
    }

    /// Serve newline-delimited JSON-RPC with bounded frames and in-flight work.
    ///
    /// `read` and `write` are stdio in the binary; tests inject byte streams,
    /// while owner integration still uses actual authenticated Unix sockets.
    pub async fn serve_io<R, W>(self, read: R, write: W) -> std::io::Result<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        protocol::serve(self, read, write).await
    }
}

impl ServerHandler for McpService {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "ai2screenwake",
                crate::build_info::VERSION,
            ))
            .with_protocol_version(ProtocolVersion::LATEST_WITH_INITIALIZE)
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(ProtocolVersion::known_up_to(
            &ProtocolVersion::LATEST_WITH_INITIALIZE,
        ))
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        let supported = self.supported_protocol_versions();
        if !supported.contains(&request.protocol_version) {
            return Err(ErrorData::unsupported_protocol_version(
                request.protocol_version,
                &supported,
            ));
        }
        context.peer.set_peer_info(request.clone());
        self.negotiate_initialize(&request)
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|request| request.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "AI2ScreenWake has no tool cursor.",
                None,
            ));
        }
        Ok(ListToolsResult::with_all_items(tools::definitions(
            self.output_schema.clone(),
        )))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools::definitions(self.output_schema.clone())
            .into_iter()
            .find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let control = tools::Control::parse(&request.name)?;
        if request
            .arguments
            .is_some_and(|arguments| !arguments.is_empty())
            || request.input_responses.is_some()
            || request.request_state.is_some()
        {
            return Err(ErrorData::invalid_params(
                "AI2ScreenWake tools take no arguments.",
                None,
            ));
        }
        let result = tokio::select! {
            result = self.owner.call(control) => result?,
            () = context.ct.cancelled() => {
                return Err(ErrorData::new(
                    rmcp::model::ErrorCode(-32800),
                    "The MCP request was cancelled; admitted owner intent is unchanged.",
                    None,
                ));
            }
        };
        Ok(tools::result(result)?.into())
    }
}
// end mod.rs
