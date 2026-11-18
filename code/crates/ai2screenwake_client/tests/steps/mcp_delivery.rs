// mcp_delivery.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! MCP byte-stream/subprocess and actual Unix-owner requirements, never real power.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, CorrelationId, Emitter, Empty, Error, Fault,
        IdleObserved, IdleRequest, Outcome, Power,
    },
    ipc::{MAX_FRAME_BYTES, OwnerServer, for_current_user},
    mcp::{McpService, owner_connection::OwnerAdapter},
    runtime::RuntimeOptions,
};
use ai2screenwake_native::{NativePower, launch::OwnerLauncher};
use cucumber::{World, given, then, when, writer::Stats};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{Notify, oneshot},
};

struct Session {
    reader: BufReader<Box<dyn AsyncRead + Unpin + Send>>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
    server: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    child: Option<tokio::process::Child>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("subprocess", &self.child.is_some())
            .finish()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            server.abort();
        }
    }
}

impl Session {
    fn memory(owner: OwnerAdapter) -> Self {
        let (client, server) = tokio::io::duplex(128 * 1024);
        let (reader, writer) = tokio::io::split(client);
        let server = tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(server);
            McpService::new(owner, ai2screenwake_backend::mcp::tool_output_schema())
                .serve_io(reader, writer)
                .await
        });
        Self {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            server: Some(server),
            child: None,
        }
    }

    fn subprocess() -> Self {
        Self::spawn(tokio::process::Command::new(env!(
            "CARGO_BIN_EXE_ai2screenwake-mcp"
        )))
    }

    fn owner_subprocess(directory: &std::path::Path) -> Self {
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command.arg("--mcp-test-stdio").arg(directory);
        Self::spawn(command)
    }

    fn spawn(mut command: tokio::process::Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let reader = child.stdout.take().unwrap();
        let writer = child.stdin.take().unwrap();
        Self {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            server: None,
            child: Some(child),
        }
    }

    async fn raw(&mut self, bytes: &[u8]) {
        tokio::time::timeout(Duration::from_secs(2), self.writer.write_all(bytes))
            .await
            .expect("MCP input deadline")
            .unwrap();
    }

    async fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.raw(&bytes).await;
    }

    async fn receive(&mut self) -> Value {
        let mut line = String::new();
        let size = tokio::time::timeout(Duration::from_secs(2), self.reader.read_line(&mut line))
            .await
            .expect("MCP response deadline")
            .unwrap();
        assert!(
            size > 0 && size <= MAX_FRAME_BYTES,
            "invalid response frame: {line}"
        );
        serde_json::from_str(&line).expect("exactly one JSON-RPC response per line")
    }

    async fn initialize(&mut self) {
        self.send(initialize("2025-11-25")).await;
        let result = self.receive().await;
        assert_eq!(
            result["result"]["protocolVersion"], "2025-11-25",
            "{result}"
        );
        assert!(
            result["result"]["capabilities"]["tools"].is_object(),
            "{result}"
        );
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
    }

    async fn call(&mut self, id: u64, name: &str) -> Value {
        self.send(tool(id, name)).await;
        let result = self.receive().await;
        assert_eq!(result["id"], id, "{result}");
        result
    }

    async fn close(mut self) {
        self.writer.shutdown().await.unwrap();
        drop(std::mem::replace(
            &mut self.writer,
            Box::new(tokio::io::sink()),
        ));
        if let Some(server) = self.server.take() {
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .expect("MCP task shutdown deadline")
                .unwrap()
                .unwrap();
        }
        if let Some(mut child) = self.child.take() {
            assert!(
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .expect("MCP subprocess shutdown deadline")
                    .unwrap()
                    .success()
            );
        }
    }
}

fn initialize(version: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": version, "capabilities": {},
        "clientInfo": {"name": "screenwake-requirements", "version": "0.1.0"}
    }})
}

fn tool(id: u64, name: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": {}}})
}

#[derive(Default)]
struct Native {
    count: AtomicUsize,
    paused: AtomicBool,
    fail_apply: AtomicBool,
    fail_sample: AtomicBool,
    pending: Mutex<Vec<(Apply, Emitter)>>,
    changed: Notify,
}

impl Native {
    fn finish(&self, request: Apply, events: Emitter) {
        let fail = self.fail_apply.load(Ordering::SeqCst);
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
            held_count: if request.enabled && !fail { 2 } else { 0 },
            request,
            outcome: if fail {
                Outcome::Failed
            } else {
                Outcome::Applied
            },
            fault_codes: if fail {
                vec!["test_acquire_failed".into()]
            } else {
                vec![]
            },
            clock: ClockSample {
                epoch_ms: 100_000,
                monotonic_ms: 1000,
            },
        });
    }

    fn release(&self) {
        self.paused.store(false, Ordering::SeqCst);
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for (request, events) in pending {
            self.finish(request, events);
        }
    }

    async fn applied(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.count.load(Ordering::SeqCst) == 0 {
                self.changed.notified().await;
            }
        })
        .await
        .expect("actual owner must reach the native boundary");
    }
}

impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
        if self.paused.load(Ordering::SeqCst) {
            self.pending.lock().unwrap().push((request, events));
        } else {
            self.finish(request, events);
        }
        self.count.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_one();
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        let failed = self.fail_sample.load(Ordering::SeqCst);
        events.emit::<Power::Activity::System::Observed>(IdleObserved {
            request,
            clock: ClockSample {
                epoch_ms: 110_000,
                monotonic_ms: 11_000,
            },
            idle_age_ms: if failed { None } else { Some(5000) },
            fault: failed
                .then(|| Fault::new("test_sampling_failed", "Native idle sampling failed.", true)),
        });
    }
}

struct NoLaunch;
impl OwnerLauncher for NoLaunch {
    fn launch(&self) -> Result<(), Fault> {
        Err(Fault::new(
            "test_owner_unavailable",
            "No test owner is available.",
            true,
        ))
    }
}

#[derive(Default)]
struct Admissions {
    pending: Mutex<HashSet<CorrelationId>>,
    count: AtomicUsize,
    changed: Notify,
}

impl Admissions {
    async fn wait(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = self.changed.notified();
                if self.count.load(Ordering::SeqCst) >= count {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the control must pass owner admission before native release: expected {count}, observed {}", self.count.load(Ordering::SeqCst)));
    }
}

#[derive(Default, World)]
struct Mcp {
    sessions: Vec<Session>,
    response: Option<Value>,
    results: Vec<Value>,
    root: Option<tempfile::TempDir>,
    owner: Option<OwnerServer>,
    native: Arc<Native>,
    process_failed: bool,
    admissions: Arc<Admissions>,
}

impl std::fmt::Debug for Mcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mcp")
            .field("sessions", &self.sessions.len())
            .finish()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        self.native.release();
        self.sessions.clear();
        self.owner.take();
    }
}

impl Mcp {
    fn add_owner_session(&mut self) {
        self.sessions.push(Session::owner_subprocess(
            &self.root.as_ref().unwrap().path().join("owner"),
        ));
    }

    async fn owner(&mut self, deadline: Duration) {
        self.root = Some(tempfile::Builder::new().prefix("swm-").tempdir().unwrap());
        let location = for_current_user(self.root.as_ref().unwrap().path().join("owner"));
        self.owner = Some(
            OwnerServer::bind_with_options(
                location,
                self.native.clone(),
                RuntimeOptions {
                    operation_timeout: deadline,
                },
            )
            .unwrap(),
        );
        let actions = self.owner.as_ref().unwrap().actions();
        let on = self.admissions.clone();
        actions.observe_action::<Power::Session::Protection::TurnOn>(move |_, events| {
            on.pending.lock().unwrap().insert(events.correlation());
        });
        let errors = self.admissions.clone();
        actions.observe_action::<Error::Dispatch::Validation::Occurred>(move |_, events| {
            errors.pending.lock().unwrap().remove(&events.correlation());
        });
        let admitted = self.admissions.clone();
        actions.observe_action::<Power::Session::Protection::Snapshot>(move |status, events| {
            if status.state == ai2screenwake_client::contract::State::Pending
                && admitted
                    .pending
                    .lock()
                    .unwrap()
                    .remove(&events.correlation())
            {
                admitted.count.fetch_add(1, Ordering::SeqCst);
                admitted.changed.notify_one();
            }
        });
        for _ in 0..2 {
            self.add_owner_session();
            self.sessions.last_mut().unwrap().initialize().await;
        }
    }
}

fn structured(response: &Value) -> &Value {
    let result = &response["result"];
    assert!(result.is_object(), "{response}");
    let structured = &result["structuredContent"];
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        *structured
    );
    assert_eq!(result["isError"], !structured["error"].is_null());
    if let Some(error) = structured["error"].as_object() {
        assert_eq!(error.len(), 3);
        assert!(
            error["fault_code"].is_string()
                && error["message"].is_string()
                && error["recoverable"].is_boolean()
        );
    }
    if let Some(status) = structured["status"].as_object() {
        let on = status["is_on"] == true;
        for field in [
            "protected_idle_duration_seconds",
            "protected_idle_suppressed_events",
            "suppressed_events_truncated",
        ] {
            assert_eq!(status.contains_key(field), on, "{structured}");
        }
    }
    structured
}

#[given("a local MCP stdio session")]
fn local_session(world: &mut Mcp) {
    world
        .sessions
        .push(Session::memory(OwnerAdapter::bundled()));
}

#[given("the built MCP subprocess")]
fn subprocess(world: &mut Mcp) {
    world.sessions.push(Session::subprocess());
}

#[given("an initialized local MCP session")]
async fn initialized(world: &mut Mcp) {
    local_session(world);
    world.sessions[0].initialize().await;
}

#[given("two MCP sessions connected to one real private owner")]
async fn shared_owner(world: &mut Mcp) {
    world.owner(Duration::from_secs(5)).await;
}

#[given("a real private owner with a short test deadline")]
async fn short_deadline(world: &mut Mcp) {
    world.owner(Duration::from_millis(100)).await;
}

#[given("a real private owner location without a listener")]
async fn unavailable(world: &mut Mcp) {
    world.root = Some(tempfile::Builder::new().prefix("swm-").tempdir().unwrap());
    world.add_owner_session();
    world.sessions[0].initialize().await;
}

#[when("the client initializes and lists tools")]
async fn discover(world: &mut Mcp) {
    world.sessions[0].initialize().await;
    world.sessions[0]
        .send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))
        .await;
    world.response = Some(world.sessions[0].receive().await);
}

#[when("the first client lists tools without issuing a control")]
async fn discovery_only(world: &mut Mcp) {
    world.sessions[0]
        .send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))
        .await;
    world.response = Some(world.sessions[0].receive().await);
}

#[then("exactly is_on turn_on and turn_off are advertised with empty inputs")]
fn approved_tools(world: &mut Mcp) {
    let response = world.response.as_ref().unwrap();
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list must return tool definitions");
    let names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["is_on", "turn_on", "turn_off"]);
    for tool in tools {
        assert_eq!(
            tool["inputSchema"],
            json!({"type": "object", "properties": {}, "additionalProperties": false})
        );
        let schema = &tool["outputSchema"];
        assert_eq!(schema["required"], json!(["status", "error"]));
        assert_eq!(
            schema["$defs"]["Status"]["allOf"][0]["if"]["properties"]["is_on"]["const"],
            true
        );
        assert_eq!(
            schema["$defs"]["Status"]["allOf"][0]["then"]["required"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            schema["$defs"]["Status"]["allOf"][0]["else"]["properties"]["protected_idle_duration_seconds"],
            false
        );
        assert_eq!(schema["$defs"]["ToolFault"]["additionalProperties"], false);
        assert!(
            schema["$defs"]["ToolFault"]["properties"]
                .get("failed_genus")
                .is_none()
        );
    }
}

#[when(regex = r"^the client calls turn_on with arguments (.+)$")]
async fn invalid_arguments(world: &mut Mcp, arguments: String) {
    let mut request = tool(3, "turn_on");
    request["params"]["arguments"] = serde_json::from_str(&arguments).unwrap();
    world.sessions[0].send(request).await;
    world.response = Some(world.sessions[0].receive().await);
}

#[when("the client calls unknown tool idle_info")]
async fn unknown_tool(world: &mut Mcp) {
    world.response = Some(world.sessions[0].call(3, "idle_info").await);
}

#[when("the client calls an unknown method")]
async fn unknown_method(world: &mut Mcp) {
    world.sessions[0]
        .send(json!({"jsonrpc": "2.0", "id": 3, "method": "screenwake/unknown", "params": {}}))
        .await;
    world.response = Some(world.sessions[0].receive().await);
}

#[then(regex = r"^JSON-RPC error (-?[0-9]+) is returned$")]
fn protocol_error(world: &mut Mcp, code: i64) {
    let response = world.response.as_ref().unwrap();
    assert_eq!(response["error"]["code"], code, "{response}");
    assert!(response.get("result").is_none());
}

#[when("malformed JSON is followed by a ping")]
async fn malformed(world: &mut Mcp) {
    world.sessions[0].raw(b"{broken\n").await;
    world.results.push(world.sessions[0].receive().await);
    world.sessions[0]
        .send(json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}))
        .await;
    world.results.push(world.sessions[0].receive().await);
}

#[then("a parse error precedes the successful ping")]
fn parse_recovered(world: &mut Mcp) {
    assert_eq!(world.results[0]["error"]["code"], -32700);
    assert!(world.results[1]["result"].is_object());
    assert_eq!(world.results[1]["id"], 4);
}

#[when("a request uses the wrong JSON-RPC version")]
async fn wrong_rpc(world: &mut Mcp) {
    world.sessions[0]
        .send(json!({"jsonrpc": "1.0", "id": 3, "method": "tools/list"}))
        .await;
    world.response = Some(world.sessions[0].receive().await);
}

#[when("the client initializes with an unsupported revision")]
async fn unsupported(world: &mut Mcp) {
    world.sessions[0].send(initialize("1900-01-01")).await;
    world.response = Some(world.sessions[0].receive().await);
}

#[when("an oversized line is followed by a ping")]
async fn oversized(world: &mut Mcp) {
    let mut bytes = vec![b'x'; MAX_FRAME_BYTES + 1];
    bytes.push(b'\n');
    world.sessions[0].raw(&bytes).await;
    world.results.push(world.sessions[0].receive().await);
    world.sessions[0]
        .send(json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}))
        .await;
    world.results.push(world.sessions[0].receive().await);
}

#[then("an invalid-request error precedes the successful ping")]
fn size_recovered(world: &mut Mcp) {
    assert_eq!(world.results[0]["error"]["code"], -32600);
    assert_eq!(world.results[1]["id"], 4);
    assert!(world.results[1]["result"].is_object());
}

#[when("the client calls is_on")]
async fn query(world: &mut Mcp) {
    world.response = Some(world.sessions[0].call(3, "is_on").await);
}

#[then("the unavailable result has null status and an explicit public error")]
fn unavailable_result(world: &mut Mcp) {
    let value = structured(world.response.as_ref().unwrap());
    assert!(value["status"].is_null(), "{value}");
    assert!(value["error"].is_object(), "{value}");
}

#[when("stdout closes while stdin remains open")]
async fn closed_stdout(world: &mut Mcp) {
    let session = &mut world.sessions[0];
    session.reader = BufReader::new(Box::new(tokio::io::empty()));
    session
        .send(json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}))
        .await;
    let status = tokio::time::timeout(
        Duration::from_secs(2),
        session.child.as_mut().unwrap().wait(),
    )
    .await
    .expect("closed stdout must not wait for stdin EOF")
    .unwrap();
    world.process_failed = !status.success();
}

#[then("the executable exits with an explicit transport failure")]
fn transport_failure(world: &mut Mcp) {
    assert!(world.process_failed);
}

#[then("the native collaborator has received no apply")]
fn no_apply(world: &mut Mcp) {
    approved_tools(world);
    assert_eq!(world.native.count.load(Ordering::SeqCst), 0);
}

#[when("the clients interleave on query and native off")]
async fn interleave(world: &mut Mcp) {
    world
        .results
        .push(world.sessions[0].call(10, "turn_on").await);
    world
        .results
        .push(world.sessions[1].call(20, "is_on").await);
    let actions = world.owner.as_ref().unwrap().actions();
    let off = actions
        .prepare::<Power::Session::Protection::TurnOff>(Empty {})
        .unwrap();
    let correlation = off.correlation();
    let (sender, receiver) = oneshot::channel();
    let sender = Mutex::new(Some(sender));
    actions.observe_action::<Power::Session::Protection::Settled>(move |settlement, _| {
        if settlement.correlation == correlation
            && let Some(sender) = sender.lock().unwrap().take()
        {
            let _ = sender.send(settlement.result);
        }
    });
    actions.emit_prepared(off);
    let settled = tokio::time::timeout(Duration::from_secs(2), receiver)
        .await
        .expect("native Off settlement deadline")
        .expect("native Off observation closed");
    assert!(settled.error.is_none(), "{settled:?}");
    assert_eq!(
        settled.status.as_ref().and_then(|status| status.is_on),
        Some(false),
        "{settled:?}"
    );
    world
        .results
        .push(world.sessions[0].call(30, "is_on").await);
}

#[then("each result matches its request and the shared native owner")]
fn shared_truth(world: &mut Mcp) {
    for (response, on) in world.results.iter().zip([true, true, false]) {
        let value = structured(response);
        assert!(value["error"].is_null(), "{response}");
        assert_eq!(value["status"]["is_on"], on);
    }
    let query = structured(&world.results[1]);
    assert_eq!(query["status"]["protected_idle_duration_seconds"], 5);
    assert_eq!(
        query["status"]["protected_idle_suppressed_events"],
        json!([])
    );
    assert_eq!(query["status"]["suppressed_events_truncated"], false);
    assert_eq!(world.native.count.load(Ordering::SeqCst), 2);
}

#[when("one client sends two on requests before native convergence")]
async fn repeated(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.native.applied().await;
    world.sessions[0].send(tool(11, "turn_on")).await;
    world.admissions.wait(2).await;
    world.native.release();
    world.results.push(world.sessions[0].receive().await);
    world.results.push(world.sessions[0].receive().await);
}

#[then("both request IDs succeed with just one native apply")]
fn repeated_results(world: &mut Mcp) {
    let mut ids: Vec<_> = world
        .results
        .iter()
        .map(|response| response["id"].as_u64().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, [10, 11]);
    for response in &world.results {
        let value = structured(response);
        assert_eq!(value["status"]["is_on"], true);
        assert!(value["error"].is_null());
    }
    assert_eq!(world.native.count.load(Ordering::SeqCst), 1);
}

#[when("another client requests off before on converges")]
async fn opposite(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.native.applied().await;
    world.sessions[1].send(tool(20, "turn_off")).await;
    world.results.push(world.sessions[0].receive().await);
    world.native.release();
    world.results.push(world.sessions[1].receive().await);
}

#[then("the on request is superseded and only the off request succeeds")]
fn superseded(world: &mut Mcp) {
    assert_eq!(world.results[0]["id"], 10);
    assert_eq!(
        structured(&world.results[0])["error"]["fault_code"],
        "superseded"
    );
    assert_eq!(world.results[1]["id"], 20);
    assert!(structured(&world.results[1])["error"].is_null());
    assert_eq!(structured(&world.results[1])["status"]["is_on"], false);
}

#[when("native acquisition fails during a turn_on call")]
async fn native_failure(world: &mut Mcp) {
    world.native.fail_apply.store(true, Ordering::SeqCst);
    world.response = Some(world.sessions[0].call(10, "turn_on").await);
}

#[then("the tool error preserves unknown protection without exposing failed_genus")]
fn native_fault(world: &mut Mcp) {
    let value = structured(world.response.as_ref().unwrap());
    assert!(value["status"]["is_on"].is_null());
    assert!(value["error"].is_object());
    assert!(value["error"].get("failed_genus").is_none());
}

#[when("an On query encounters a native idle sampling failure")]
async fn sample_failure(world: &mut Mcp) {
    assert!(structured(&world.sessions[0].call(10, "turn_on").await)["error"].is_null());
    world.native.fail_sample.store(true, Ordering::SeqCst);
    world.response = Some(world.sessions[0].call(11, "is_on").await);
}

#[then("On remains true with null duration and an explicit sampling error")]
fn sample_fault(world: &mut Mcp) {
    let value = structured(world.response.as_ref().unwrap());
    assert_eq!(value["status"]["is_on"], true);
    assert!(value["status"]["protected_idle_duration_seconds"].is_null());
    assert_eq!(value["error"]["fault_code"], "test_sampling_failed");
}

#[when("native convergence misses the owner deadline")]
async fn deadline(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.response = Some(world.sessions[0].call(10, "turn_on").await);
}

#[then("the tool result reports the owner's timeout")]
fn deadline_result(world: &mut Mcp) {
    let value = structured(world.response.as_ref().unwrap());
    assert_eq!(
        value["error"]["fault_code"], "operation_timed_out",
        "{value}"
    );
    assert!(value["status"]["is_on"].is_null());
}

#[when("an on caller disconnects before native convergence")]
async fn disconnect(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.native.applied().await;
    world.sessions.remove(0).close().await;
    world.native.release();
    world.response = Some(world.sessions[0].call(20, "is_on").await);
}

#[then("the remaining client observes On after the admitted apply finishes")]
fn disconnected_intent(world: &mut Mcp) {
    let value = structured(world.response.as_ref().unwrap());
    assert_eq!(value["status"]["is_on"], true, "{value}");
    assert!(value["error"].is_null());
    assert_eq!(world.native.count.load(Ordering::SeqCst), 1);
}

#[when("an on caller cancels its request before native convergence")]
async fn cancellation(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.native.applied().await;
    for id in 11..42 {
        world.sessions[0].send(tool(id, "turn_on")).await;
    }
    world.admissions.wait(32).await;
    for id in 10..42 {
        world.sessions[0]
            .send(json!({
                "jsonrpc": "2.0", "method": "notifications/cancelled",
                "params": {"requestId": id, "reason": "test observer relinquished"}
            }))
            .await;
    }
    world.sessions[0].send(tool(42, "turn_on")).await;
    let admissions = world.admissions.clone();
    tokio::select! {
        () = admissions.wait(33) => {},
        result = world.sessions[0].receive() => panic!("replacement control was rejected before owner admission: {result}"),
    }
    world.native.release();
    let response = world.sessions[0].receive().await;
    assert_eq!(response["id"], 42, "{response}");
    assert!(structured(&response)["error"].is_null(), "{response}");
    world.response = Some(world.sessions[0].call(100, "is_on").await);
}

#[then("a fresh query succeeds after the admitted apply finishes")]
fn cancelled_intent(world: &mut Mcp) {
    disconnected_intent(world);
}

#[when("a client duplicates an outstanding on request ID")]
async fn duplicate(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.native.applied().await;
    world.sessions[0].send(tool(10, "turn_on")).await;
    world.results.push(world.sessions[0].receive().await);
    world.native.release();
    world.results.push(world.sessions[0].receive().await);
}

#[then("the duplicate is rejected and the original request still settles")]
fn duplicate_result(world: &mut Mcp) {
    assert_eq!(world.results[0]["id"], 10);
    assert_eq!(world.results[0]["error"]["code"], -32600);
    assert_eq!(world.results[1]["id"], 10);
    assert_eq!(structured(&world.results[1])["status"]["is_on"], true);
}

#[when("a client submits more than thirty-two outstanding controls")]
async fn capacity(world: &mut Mcp) {
    world.native.paused.store(true, Ordering::SeqCst);
    for id in 10..43 {
        world.sessions[0].send(tool(id, "turn_on")).await;
    }
    world.results.push(world.sessions[0].receive().await);
    world.native.applied().await;
    world.native.release();
    for _ in 0..32 {
        world.results.push(world.sessions[0].receive().await);
    }
}

#[then("the extra request is rejected and all admitted requests settle")]
fn capacity_result(world: &mut Mcp) {
    assert_eq!(world.results[0]["id"], 42);
    assert_eq!(world.results[0]["error"]["code"], -32000);
    let mut ids = Vec::new();
    for response in &world.results[1..] {
        assert_eq!(structured(response)["status"]["is_on"], true);
        assert!(structured(response)["error"].is_null(), "{response}");
        ids.push(response["id"].as_u64().unwrap());
    }
    ids.sort();
    assert_eq!(ids, (10..42).collect::<Vec<_>>());
}

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments
        .get(1)
        .is_some_and(|argument| argument == "--mcp-test-stdio")
    {
        assert_eq!(arguments.len(), 3);
        let location = for_current_user(std::path::PathBuf::from(&arguments[2]));
        McpService::new(
            OwnerAdapter::for_owner(location, Arc::new(NoLaunch)),
            ai2screenwake_backend::mcp::tool_output_schema(),
        )
        .serve_io(tokio::io::stdin(), tokio::io::stdout())
        .await
        .unwrap();
        return;
    }
    let started = Instant::now();
    let result = Mcp::cucumber()
        .max_concurrent_scenarios(3)
        .fail_on_skipped()
        .filter_run(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features"),
            |feature, _, _| {
                matches!(
                    feature.name.as_str(),
                    "ScreenWake local MCP protocol" | "MCP controls use the real shared owner IPC"
                )
            },
        )
        .await;
    eprintln!(
        "MCP scenario elapsed_seconds={:.6} worker_count=3",
        started.elapsed().as_secs_f64()
    );
    assert!(result.passed_steps() > 0, "no MCP requirements selected");
    assert!(!result.execution_has_failed(), "MCP requirements failed");
}
// end mcp_delivery.rs
