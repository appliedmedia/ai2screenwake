// protocol.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Bounded JSON-RPC framing around the official SDK's protocol/handler lifecycle.

use super::McpService;
use crate::ipc::{MAX_FRAME_BYTES, MAX_OUTSTANDING};
use rmcp::{
    RoleServer, ServiceExt,
    model::{
        ClientJsonRpcMessage, ClientNotification, ErrorCode, ErrorData, RequestId,
        ServerJsonRpcMessage,
    },
    transport::Transport,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

type Active = Arc<Mutex<HashSet<RequestId>>>;

struct Outgoing {
    message: ServerJsonRpcMessage,
    completed: Option<RequestId>,
}

struct Wire<R> {
    reader: BufReader<R>,
    line: Vec<u8>,
    discarding: bool,
    pending_error: Option<ServerJsonRpcMessage>,
    output: mpsc::Sender<Outgoing>,
    active: Active,
    cancelled: CancellationToken,
    read_error: Arc<Mutex<Option<io::Error>>>,
}

enum Read {
    Line,
    Oversized,
    Closed,
}

impl<R: AsyncRead + Unpin> Wire<R> {
    async fn read_line(&mut self) -> io::Result<Read> {
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                return Ok(Read::Closed);
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(available.len(), |position| position + 1);
            let oversized = !self.discarding && self.line.len() + count > MAX_FRAME_BYTES;
            if oversized {
                self.discarding = true;
                self.line.clear();
            } else if !self.discarding {
                self.line.extend_from_slice(&available[..count]);
            }
            self.reader.consume(count);
            if newline.is_some() {
                if self.discarding {
                    self.discarding = false;
                } else {
                    return Ok(Read::Line);
                }
            }
            if oversized {
                return Ok(Read::Oversized);
            }
        }
    }

    fn parse(&self) -> Result<ClientJsonRpcMessage, (Option<RequestId>, ErrorData)> {
        let value: Value = serde_json::from_slice(&self.line)
            .map_err(|_| (None, ErrorData::parse_error("Invalid JSON.", None)))?;
        let id = value
            .get("id")
            .and_then(|id| serde_json::from_value(id.clone()).ok());
        if !value.is_object()
            || value["jsonrpc"] != "2.0"
            || !value["method"].is_string()
            || (value.get("id").is_some() && id.is_none())
        {
            return Err((
                id,
                ErrorData::invalid_request("Invalid JSON-RPC request.", None),
            ));
        }
        if value["method"] == "tools/call" {
            let params = &value["params"];
            if !params.is_object()
                || !params["name"].is_string()
                || params.as_object().is_some_and(|params| {
                    params
                        .keys()
                        .any(|key| !matches!(key.as_str(), "name" | "arguments" | "_meta"))
                })
                || params
                    .get("arguments")
                    .is_some_and(|arguments| !arguments.is_object())
            {
                return Err((
                    id,
                    ErrorData::invalid_params("Invalid tools/call parameters.", None),
                ));
            }
        }
        serde_json::from_value(value).map_err(|_| {
            (
                id,
                ErrorData::invalid_params("Invalid method parameters.", None),
            )
        })
    }
}

impl<R: AsyncRead + Unpin + Send + 'static> Transport<RoleServer> for Wire<R> {
    type Error = io::Error;

    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let output = self.output.clone();
        let completed = match &message {
            ServerJsonRpcMessage::Response(response) => Some(response.id.clone()),
            ServerJsonRpcMessage::Error(error) => error.id.clone(),
            _ => None,
        };
        async move {
            output
                .send(Outgoing { message, completed })
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MCP output closed"))
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if let Some(message) = &self.pending_error {
                if self
                    .output
                    .send(Outgoing {
                        message: message.clone(),
                        completed: None,
                    })
                    .await
                    .is_err()
                {
                    return None;
                }
                self.pending_error = None;
            }
            let output = self.output.clone();
            let read = tokio::select! {
                result = self.read_line() => result,
                () = output.closed() => {
                    self.cancelled.cancel();
                    return None;
                },
            };
            match read {
                Ok(Read::Closed) => {
                    self.cancelled.cancel();
                    return None;
                }
                Err(error) => {
                    *self.read_error.lock().expect("MCP read error poisoned") = Some(error);
                    self.cancelled.cancel();
                    return None;
                }
                Ok(Read::Oversized) => {
                    self.pending_error = Some(ServerJsonRpcMessage::error(
                        ErrorData::invalid_request("The MCP frame exceeds 64 KiB.", None),
                        None,
                    ));
                    continue;
                }
                Ok(Read::Line) => {}
            }
            let parsed = self.parse();
            self.line.clear();
            match parsed {
                Err((id, error)) => {
                    self.pending_error = Some(ServerJsonRpcMessage::error(error, id))
                }
                Ok(message) => {
                    if let ClientJsonRpcMessage::Notification(notification) = &message
                        && let ClientNotification::CancelledNotification(cancelled) =
                            &notification.notification
                        && let Some(id) = &cancelled.params.request_id
                    {
                        self.active
                            .lock()
                            .expect("MCP request ledger poisoned")
                            .remove(id);
                    }
                    if let ClientJsonRpcMessage::Request(request) = &message {
                        let mut active = self.active.lock().expect("MCP request ledger poisoned");
                        let error = if active.contains(&request.id) {
                            Some(ErrorData::invalid_request(
                                "Duplicate in-flight request id.",
                                None,
                            ))
                        } else if active.len() >= MAX_OUTSTANDING {
                            Some(ErrorData::new(
                                ErrorCode(-32000),
                                "Too many outstanding MCP requests.",
                                None,
                            ))
                        } else {
                            active.insert(request.id.clone());
                            None
                        };
                        if let Some(error) = error {
                            self.pending_error =
                                Some(ServerJsonRpcMessage::error(error, Some(request.id.clone())));
                            continue;
                        }
                    }
                    return Some(message);
                }
            }
        }
    }

    async fn close(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) async fn serve<R, W>(handler: McpService, read: R, mut write: W) -> io::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (output, mut outgoing) = mpsc::channel::<Outgoing>(MAX_OUTSTANDING);
    let active = Active::default();
    let cancelled = CancellationToken::new();
    let read_error = Arc::new(Mutex::new(None));
    let writer_active = active.clone();
    let writer = tokio::spawn(async move {
        while let Some(outgoing) = outgoing.recv().await {
            let mut bytes = serde_json::to_vec(&outgoing.message).map_err(io::Error::other)?;
            bytes.push(b'\n');
            if bytes.len() > MAX_FRAME_BYTES {
                return Err(io::Error::other("MCP output exceeds 64 KiB"));
            }
            tokio::time::timeout(Duration::from_secs(5), async {
                write.write_all(&bytes).await?;
                write.flush().await
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP output deadline"))??;
            if let Some(id) = outgoing.completed {
                writer_active
                    .lock()
                    .expect("MCP request ledger poisoned")
                    .remove(&id);
            }
        }
        write.shutdown().await
    });
    let wire = Wire {
        reader: BufReader::new(read),
        line: Vec::new(),
        discarding: false,
        pending_error: None,
        output,
        active,
        cancelled: cancelled.clone(),
        read_error: read_error.clone(),
    };
    let result = async {
        let service = handler
            .serve_with_ct(wire, cancelled)
            .await
            .map_err(io::Error::other)?;
        service.waiting().await.map_err(io::Error::other)?;
        Ok(())
    }
    .await;
    let written = writer.await.map_err(io::Error::other)?;
    if let Some(error) = read_error.lock().expect("MCP read error poisoned").take() {
        return Err(error);
    }
    result.and(written)
}
// end protocol.rs
