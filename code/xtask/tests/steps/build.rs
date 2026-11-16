// build.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Focused executable requirements for canonical native packaging.

use cucumber::{World, given, then, when};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[path = "../../src/assets.rs"]
mod assets;
#[path = "../../src/build.rs"]
mod build;
#[path = "../../src/check.rs"]
#[allow(dead_code)]
mod check;

#[derive(Debug, Default, World)]
struct Bundle {
    output: Option<std::process::Output>,
    fixture: Option<tempfile::TempDir>,
    error: Option<String>,
    writer: Option<TcpStream>,
    late_write: bool,
    owned_pids: Vec<u32>,
    producer_output: Option<String>,
    producer_snapshot: Option<build::ProducerReceipt>,
}

impl Drop for Bundle {
    fn drop(&mut self) {
        if let Some(writer) = &mut self.writer {
            if self.late_write {
                writer
                    .write_all(b"q")
                    .expect("stop the controlled surviving red-test writer");
                let mut ended = [0];
                assert_eq!(
                    writer.read(&mut ended).unwrap(),
                    0,
                    "red-test writer must exit"
                );
            }
        }
    }
}

impl Bundle {
    fn root(&self) -> &Path {
        self.fixture.as_ref().unwrap().path()
    }

    fn assert_preserved(&self) {
        assert_eq!(
            fs::read(self.root().join(build::TARGET).join("previous-artifact")).unwrap(),
            b"previous complete output"
        );
        assert_eq!(
            fs::read(self.root().join("unrelated-output")).unwrap(),
            b"unique user output"
        );
    }
}

#[given("an isolated build directory with a previous application")]
fn fixture(world: &mut Bundle) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/build-scenarios");
    fs::create_dir_all(&parent).unwrap();
    let fixture = tempfile::tempdir_in(parent.canonicalize().unwrap()).unwrap();
    let destination = fixture.path().join(build::TARGET);
    fs::create_dir_all(&destination).unwrap();
    fs::write(
        destination.join("previous-artifact"),
        b"previous complete output",
    )
    .unwrap();
    fs::write(
        fixture.path().join("unrelated-output"),
        b"unique user output",
    )
    .unwrap();
    world.fixture = Some(fixture);
}

#[when("the build command requests the debug profile")]
fn unsupported_profile(world: &mut Bundle) {
    world.output = Some(
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["build", "--profile", "debug"])
            .output()
            .expect("execute the actual xtask command"),
    );
}

#[then("it reports that the release profile is required")]
fn rejects_profile(world: &mut Bundle) {
    let output = world.output.as_ref().unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("build requires --profile release"),
        "missing configuration gate: {error}"
    );
}

#[when("the build runs without native producer sources")]
fn absent_sources(world: &mut Bundle) {
    world.error = Some(
        build::run(world.root(), "fixture-source-sha", "missing-source")
            .unwrap_err()
            .to_string(),
    );
}

#[then("the missing production source is named and existing outputs are unchanged")]
fn missing_source(world: &mut Bundle) {
    assert!(
        world.error.as_ref().unwrap().contains(
            "build preflight: missing production input: code/macos/ScreenWake/main.swift"
        )
    );
    let timing =
        fs::read_to_string(world.root().join(build::EVIDENCE).join("timing.jsonl")).unwrap();
    let row: serde_json::Value = serde_json::from_str(timing.trim()).unwrap();
    assert_eq!(row["stage"], "preflight");
    assert_eq!(row["git_sha"], "fixture-source-sha");
    assert_eq!(row["exit_status"], 1);
    world.assert_preserved();
}

#[when("a recorded build input changes")]
fn changed_input(world: &mut Bundle) {
    let path = world.root().join("input");
    fs::write(&path, "original input").unwrap();
    let record = build::artifact(world.root(), "input").unwrap();
    fs::write(path, "changed input").unwrap();
    world.error = Some(
        build::verify_artifacts(world.root(), &[record])
            .unwrap_err()
            .to_string(),
    );
}

#[then("verification rejects its stale byte identity")]
fn stale_input(world: &mut Bundle) {
    assert!(
        world
            .error
            .as_ref()
            .unwrap()
            .contains("stale or mixed build artifact")
    );
}

#[when("publication receives an incomplete staged application")]
fn partial_publish(world: &mut Bundle) {
    let staged = world.root().join("staging");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("partial"), b"not a complete application").unwrap();
    world.error = Some(
        build::publish(world.root(), &staged)
            .unwrap_err()
            .to_string(),
    );
}

#[then("publication fails and existing outputs are unchanged")]
fn preserved(world: &mut Bundle) {
    assert!(
        world
            .error
            .as_ref()
            .unwrap()
            .contains("inventory is incomplete")
    );
    world.assert_preserved();
}

#[when("a build stage executes an unsuccessful command")]
fn failed_stage(world: &mut Bundle) {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 7"]);
    let result = build::Build::new(world.root(), "fixture-source-sha", "failed-stage")
        .command("compiler", command);
    world.error = Some(result.unwrap_err().to_string());
}

#[then("its failure and timing are retained without a success result")]
fn failure_evidence(world: &mut Bundle) {
    assert!(world.error.as_ref().unwrap().contains("compiler exited"));
    let timing =
        fs::read_to_string(world.root().join(build::EVIDENCE).join("timing.jsonl")).unwrap();
    let row: serde_json::Value = serde_json::from_str(timing.trim()).unwrap();
    assert_eq!(row["stage"], "compiler");
    assert_eq!(row["attempt"], "failed-stage");
    assert_eq!(row["git_sha"], "fixture-source-sha");
    assert_eq!(row["exit_status"], 7);
    assert!(row["elapsed_seconds"].as_f64().unwrap() >= 0.0);
    assert!(
        world
            .root()
            .join(build::EVIDENCE)
            .join("failed-stage-compiler.stderr")
            .is_file()
    );
    world.assert_preserved();
}

#[when("a build target points through a symlink")]
fn linked_target(world: &mut Bundle) {
    let destination = world.root().join("external-output");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("unique"), "preserve destination").unwrap();
    std::os::unix::fs::symlink(&destination, world.root().join("linked")).unwrap();
    world.error = Some(
        build::safe_path(world.root(), "linked/output")
            .unwrap_err()
            .to_string(),
    );
}

#[then("path validation rejects the target without changing its destination")]
fn rejects_link(world: &mut Bundle) {
    assert!(
        world
            .error
            .as_ref()
            .unwrap()
            .contains("symlink is not admissible")
    );
    assert_eq!(
        fs::read_to_string(world.root().join("external-output/unique")).unwrap(),
        "preserve destination"
    );
}

#[given("a captured current FFI producer in a shared compiler cache")]
fn compiler_fixture(world: &mut Bundle) {
    fixture(world);
    for arguments in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=BuildTests",
            "-c",
            "user.email=build@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--allow-empty",
            "--quiet",
            "-m",
            "Build fixture",
        ],
    ] {
        assert!(
            Command::new("git")
                .current_dir(world.root())
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }
    let directory = world.root().join("target/aarch64-apple-darwin/release");
    fs::create_dir_all(&directory).unwrap();
    for extension in ["a", "dylib"] {
        fs::write(
            directory.join(format!("libai2screenwake_client.{extension}")),
            format!("ffi {extension}"),
        )
        .unwrap();
    }
    let message = serde_json::json!({
        "reason":"compiler-artifact",
        "manifest_path":world.root().join("code/crates/ai2screenwake_client/Cargo.toml"),
        "target":{"name":"ai2screenwake_client","src_path":world.root().join("code/crates/ai2screenwake_client/src/lib.rs"),"crate_types":["rlib","staticlib","cdylib"]},
        "features":["ffi"], "profile":{"opt_level":"3","debug_assertions":false,"test":false},
        "filenames":[directory.join("libai2screenwake_client.a"),directory.join("libai2screenwake_client.dylib")],
        "executable":null
    });
    let output = format!(
        "{message}\n{}\n",
        serde_json::json!({"reason":"build-finished","success":true})
    );
    world.producer_snapshot = Some(
        build::Producer::Ffi
            .snapshot(world.root(), &assets::head(world.root()).unwrap(), &output)
            .unwrap(),
    );
    world.producer_output = Some(output);
}

#[when("MCP overwrites the shared library and cached FFI is selected again")]
fn overwritten_cache(world: &mut Bundle) {
    for extension in ["a", "dylib"] {
        fs::write(
            world.root().join(format!(
                "target/aarch64-apple-darwin/release/libai2screenwake_client.{extension}"
            )),
            b"mcp feature",
        )
        .unwrap();
    }
    build::verify_artifacts(
        world.root(),
        &world.producer_snapshot.as_ref().unwrap().snapshots,
    )
    .unwrap();
    for extension in ["a", "dylib"] {
        fs::write(
            world.root().join(format!(
                "target/aarch64-apple-darwin/release/libai2screenwake_client.{extension}"
            )),
            format!("ffi {extension}"),
        )
        .unwrap();
    }
    let result = build::Producer::Ffi
        .snapshot(
            world.root(),
            &assets::head(world.root()).unwrap(),
            world.producer_output.as_ref().unwrap(),
        )
        .unwrap();
    world.producer_snapshot = Some(result);
}

#[then("Swift inputs retain only the verified FFI bytes and prior app remains untouched")]
fn stable_ffi(world: &mut Bundle) {
    let snapshot = world.producer_snapshot.as_ref().unwrap();
    build::verify_artifacts(world.root(), &snapshot.snapshots).unwrap();
    for entry in &snapshot.snapshots {
        let extension = Path::new(&entry.path)
            .extension()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            fs::read_to_string(world.root().join(&entry.path)).unwrap(),
            format!("ffi {extension}")
        );
    }
    world.assert_preserved();
}

#[when(expr = "a {word} compiler receipt is offered as current FFI")]
fn invalid_producer(world: &mut Bundle, defect: String) {
    let mut messages: Vec<serde_json::Value> = world
        .producer_output
        .as_ref()
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    match defect.as_str() {
        "MCP" => messages[0]["features"] = serde_json::json!(["mcp"]),
        "debug" => messages[0]["profile"]["opt_level"] = serde_json::json!("0"),
        "foreign" => messages[0]["manifest_path"] = serde_json::json!("/outside/Cargo.toml"),
        "unfinished" => {
            messages.pop();
        }
        "failed" => messages[1]["success"] = serde_json::json!(false),
        _ => unreachable!(),
    }
    let output = messages
        .into_iter()
        .map(|message| message.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    world.error = Some(
        build::Producer::Ffi
            .snapshot(world.root(), &assets::head(world.root()).unwrap(), &output)
            .unwrap_err()
            .to_string(),
    );
}

#[then("producer validation fails without relabeling the previous snapshots")]
fn invalid_producer_rejected(world: &mut Bundle) {
    assert!(world.error.is_some());
    stable_ffi(world);
}

#[when("the three production Cargo producers are planned")]
fn production_commands(world: &mut Bundle) {
    let mut caches = Vec::new();
    for (producer, feature) in [
        (build::Producer::Ffi, "ffi"),
        (build::Producer::Bindgen, "bindgen"),
        (build::Producer::Mcp, "mcp"),
    ] {
        let command = producer.command(world.root()).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["--features", feature]));
        assert!(!args.contains(&"--all-features"));
        let cache = args
            .windows(2)
            .find(|pair| pair[0] == "--target-dir")
            .unwrap()[1];
        caches.push(cache.to_owned());
        if feature != "bindgen" {
            assert!(args.contains(&"--release"));
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--target", "aarch64-apple-darwin"])
            );
        } else {
            assert!(!args.contains(&"--release"));
        }
    }
    assert!(
        caches
            .iter()
            .all(|path| Path::new(path) == world.root().join("target"))
    );
}

#[then("feature profiles remain separate while compiler units share only this worktree target")]
fn production_profiles(world: &mut Bundle) {
    world.assert_preserved();
}

#[when(expr = "an independent build slice {word} beside its owned compiler")]
fn overlapping_stages(world: &mut Bundle, outcome: String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--owned-writer-parent")
        .arg(world.root())
        .arg(listener.local_addr().unwrap().to_string())
        .arg("timeout");
    let mut connection = None;
    let result = build::Build::new(world.root(), "fixture-source-sha", "alongside").alongside(
        "compiler",
        command,
        |_| -> assets::Result<()> {
            #[repr(C)]
            struct PollFd {
                fd: i32,
                events: i16,
                revents: i16,
            }
            unsafe extern "C" {
                fn poll(fds: *mut PollFd, count: u32, timeout: i32) -> i32;
            }
            let mut fd = PollFd {
                fd: listener.as_raw_fd(),
                events: 1,
                revents: 0,
            };
            assert_eq!(
                unsafe { poll(&mut fd, 1, 2000) },
                1,
                "compiler must start while the independent slice is running"
            );
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            if outcome == "completes" {
                stream.write_all(b"q").unwrap();
            }
            connection = Some(stream);
            if outcome == "fails" {
                Err("injected independent Swift failure".into())
            } else {
                Ok(())
            }
        },
    );
    if outcome == "fails" {
        let error = result.unwrap_err().to_string();
        assert!(error.contains("injected independent Swift failure"));
        assert!(!error.contains("cleanup failed"), "{error}");
    } else {
        result.unwrap();
    }
    let mut end = [0];
    assert_eq!(
        connection.unwrap().read(&mut end).unwrap(),
        0,
        "return must join the exact compiler descendants"
    );
    for name in ["parent-pid", "child-pid"] {
        let pid = fs::read_to_string(world.root().join(name)).unwrap();
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
}

#[then("both owned stages have finished before any application publication")]
fn overlap_joined(world: &mut Bundle) {
    world.assert_preserved();
    assert!(!world.root().join("late-output").exists());
}

#[when(expr = "a {word} build stage leaves a controlled descendant writer")]
fn descendant_stage(world: &mut Bundle, mode: String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let ready_writer = std::sync::Arc::new(std::sync::Mutex::new(None));
    let ready_stream = ready_writer.clone();
    let startup = move || -> assets::Result<()> {
        *ready_stream.lock().unwrap() = Some(accept_ready(&listener)?);
        Ok(())
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--owned-writer-parent")
        .arg(world.root())
        .arg(&address)
        .arg(&mode);
    let result = if mode == "nested" || mode == "outertimeout" || mode == "nestedorphan" {
        let mut nested = Command::new(std::env::current_exe().unwrap());
        nested
            .arg("--nested-build")
            .arg(world.root())
            .arg(&address)
            .arg(&mode);
        check::Attempt::with_budget(
            world.root(),
            "fixture-source-sha",
            "nested-writer",
            Duration::from_secs(2),
        )
        .execute_started(
            "build",
            nested,
            1,
            |_| Ok(1),
            startup,
            Some(if mode == "outertimeout" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(2)
            }),
        )
        .map(|_| String::new())
    } else {
        build::Build::new(world.root(), "fixture-source-sha", "owned-writer")
            .after_startup(startup)
            .command_with_limits(
                "compiler",
                command,
                Duration::from_millis(100),
                Duration::from_secs(2),
            )
    };
    world.error = Some(
        result
            .expect_err("the controlled stage must fail")
            .to_string(),
    );
    let Some(mut writer) = ready_writer.lock().unwrap().take() else {
        panic!(
            "controlled writer startup failed before any failure trigger: {:?}",
            world.error
        );
    };
    for name in ["parent-pid", "child-pid"] {
        world.owned_pids.push(
            fs::read_to_string(world.root().join(name))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    writer.set_nonblocking(false).unwrap();
    writer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let sent = writer.write_all(b"w");
    let mut acknowledgement = [0];
    world.late_write = match writer.read(&mut acknowledgement) {
        Ok(0) => false,
        Ok(1) => {
            assert!(sent.is_ok());
            assert_eq!(acknowledgement, [b'w']);
            true
        }
        Ok(_) => unreachable!(),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
            ) =>
        {
            false
        }
        Err(error) => panic!("controlled writer exit observation failed: {error}"),
    };
    world.writer = Some(writer);
}

fn accept_ready(listener: &TcpListener) -> assets::Result<TcpStream> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    unsafe extern "C" {
        fn poll(fds: *mut PollFd, count: u32, timeout: i32) -> i32;
    }
    let mut descriptor = PollFd {
        fd: listener.as_raw_fd(),
        events: 1,
        revents: 0,
    };
    if unsafe { poll(&mut descriptor, 1, 2000) } != 1 {
        return Err("controlled writer did not reach its descriptor-backed startup barrier within two seconds".into());
    }
    Ok(listener.accept()?.0)
}

#[when("Cargo launches a nested stage before its readiness-bounded outer timeout")]
fn cargo_nested(world: &mut Bundle) {
    let fixture = world.root().join("cargo-fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Cargo.toml"), "[package]\nname=\"owned-exec-fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[[bin]]\nname=\"owned-exec-fixture\"\npath=\"main.rs\"\n").unwrap();
    fs::write(fixture.join("main.rs"), "fn main() { let mut args=std::env::args_os().skip(1); let status=std::process::Command::new(args.next().unwrap()).args(args).status().unwrap(); std::process::exit(status.code().unwrap_or(1)); }\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let ready = std::sync::Arc::new(std::sync::Mutex::new(None));
    let accepted = ready.clone();
    let mut command = Command::new(std::env::var_os("CARGO").unwrap());
    command
        .args(["run", "--quiet", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", fixture.join("target"))
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .arg("--nested-build")
        .arg(world.root())
        .arg(address)
        .arg("callerloss");
    let result = check::Attempt::with_budget(
        world.root(),
        "fixture-source-sha",
        "cargo-chain",
        Duration::from_secs(2),
    )
    .execute_started(
        "cargo",
        command,
        1,
        |_| Ok(1),
        move || {
            *accepted.lock().unwrap() = Some(accept_ready(&listener)?);
            Ok(())
        },
        Some(Duration::from_millis(100)),
    );
    world.error = Some(
        result
            .err()
            .expect("outer Cargo stage must time out")
            .to_string(),
    );
    let mut writer = ready
        .lock()
        .unwrap()
        .take()
        .expect("actual Cargo chain must reach readiness before timeout");
    writer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let write = writer.write_all(b"w");
    let mut acknowledgement = [0];
    world.late_write = match writer.read(&mut acknowledgement) {
        Ok(0) => false,
        Ok(1) => {
            assert!(write.is_ok());
            true
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
            ) =>
        {
            false
        }
        outcome => panic!("controlled Cargo writer did not settle: {outcome:?}"),
    };
    world.writer = Some(writer);
    for name in ["parent-pid", "child-pid", "outer-pid"] {
        world.owned_pids.push(
            fs::read_to_string(world.root().join(name))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
}

#[then("the failed stage has no live parent descendant or late output mutation")]
fn descendants_gone(world: &mut Bundle) {
    assert!(
        !world.error.as_ref().unwrap().contains("cleanup failed"),
        "owned teardown must complete, not merely report a cleanup error"
    );
    assert!(
        !world.late_write,
        "a descendant still mutated output after the production stage returned failure"
    );
    assert!(!world.root().join("late-output").exists());
    for pid in &world.owned_pids {
        let status = Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "owned stage PID {pid} survived cleanup");
    }
    world.assert_preserved();
}

fn controlled_process() -> bool {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let Some(mode) = arguments.first().map(String::as_str) else {
        return false;
    };
    if !matches!(
        mode,
        "--owned-writer-parent" | "--owned-writer-child" | "--nested-build"
    ) {
        return false;
    }
    let root = Path::new(&arguments[1]);
    if mode == "--nested-build" {
        fs::write(root.join("outer-pid"), std::process::id().to_string()).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        let startup = TcpListener::bind("127.0.0.1:0").unwrap();
        command.args([
            "--owned-writer-parent",
            &arguments[1],
            &arguments[2],
            if arguments[3] == "nestedorphan" {
                "orphan"
            } else if arguments[3] == "callerloss" {
                "newgroup"
            } else {
                "timeout"
            },
            &startup.local_addr().unwrap().to_string(),
        ]);
        let mut driver = build::Build::new(root, "fixture-source-sha", "inner-writer")
            .after_startup(move || accept_ready(&startup).map(|_| ()));
        let result = if arguments[3] == "outertimeout" || arguments[3] == "callerloss" {
            driver.command("compiler", command)
        } else {
            driver.command_with_limits(
                "compiler",
                command,
                Duration::from_millis(100),
                Duration::from_secs(2),
            )
        };
        assert!(result.is_err());
        std::process::exit(7);
    }
    if mode == "--owned-writer-parent" {
        fs::write(root.join("parent-pid"), std::process::id().to_string()).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--owned-writer-child", &arguments[1], &arguments[2]])
            .stdout(Stdio::piped());
        if arguments[3] == "orphan" || arguments[3] == "newgroup" {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready, "ready\n");
        if let Some(address) = arguments.get(4) {
            let _ready = TcpStream::connect(address).unwrap();
        }
        println!("OWNED_WRITER_READY");
        if arguments[3] == "failed" || arguments[3] == "orphan" {
            std::process::exit(7);
        }
        std::process::exit(child.wait().unwrap().code().unwrap_or(1));
    }
    fs::write(root.join("child-pid"), std::process::id().to_string()).unwrap();
    unsafe extern "C" {
        fn getsid(pid: i32) -> i32;
    }
    fs::write(root.join("session-pid"), unsafe { getsid(0) }.to_string()).unwrap();
    let mut stream = TcpStream::connect(&arguments[2]).unwrap();
    println!("ready");
    std::io::stdout().flush().unwrap();
    let mut instruction = [0];
    while stream.read(&mut instruction).unwrap() == 1 {
        match instruction[0] {
            b'w' => {
                fs::write(
                    root.join("late-output"),
                    "descendant wrote after stage failure",
                )
                .unwrap();
                stream.write_all(b"w").unwrap();
            }
            b'q' => break,
            _ => panic!("unexpected controlled writer instruction"),
        }
    }
    true
}

#[when("the stage caller dies after its new-group writer is ready")]
fn caller_death(world: &mut Bundle) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--nested-build",
            world.root().to_str().unwrap(),
            &listener.local_addr().unwrap().to_string(),
            "callerloss",
        ])
        .env_remove("SCREENWAKE_BUILD_SESSION")
        .env_remove("SCREENWAKE_BUILD_SCOPE_FD")
        .env_remove("SCREENWAKE_BUILD_LIFETIME_FDS")
        .current_dir(world.root())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut caller = command.spawn().unwrap();
    let ready = accept_ready(&listener);
    // Cleanup always uses the exact Child handle, including failed fixture startup.
    caller.kill().unwrap();
    caller.wait().unwrap();
    let mut writer = ready.expect("writer startup must precede caller death");
    writer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let ended = matches!(writer.read(&mut [0]), Ok(0));
    if !ended {
        writer.write_all(b"q").unwrap();
        assert_eq!(
            writer.read(&mut [0]).unwrap(),
            0,
            "always stop the exact red-fixture writer"
        );
    }
    world.writer = Some(writer);
    assert!(ended, "caller EOF must tear down the new-group writer");
    for name in ["parent-pid", "child-pid", "session-pid"] {
        world.owned_pids.push(
            fs::read_to_string(world.root().join(name))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    for pid in &world.owned_pids {
        while Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
        {
            assert!(
                Instant::now() < deadline,
                "owned caller-loss PID {pid} did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    world.error = Some("caller loss after verified readiness".into());
}

fn main() {
    if build::supervisor_entry() {
        return;
    }
    if controlled_process() {
        return;
    }
    let started = Instant::now();
    futures::executor::block_on(
        Bundle::cucumber()
            .max_concurrent_scenarios(3)
            .fail_on_skipped()
            .run_and_exit(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features/build.feature"),
            ),
    );
    assert!(
        started.elapsed().as_secs_f64() < 10.0,
        "focused build scenarios exceeded ten seconds"
    );
    println!(
        "BUILD_SCENARIOS_OK workers=3 elapsed_seconds={:.6}",
        started.elapsed().as_secs_f64()
    );
}
// end build.rs
