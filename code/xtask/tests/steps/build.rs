// build.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Focused executable requirements for canonical native packaging.

use cucumber::{World, given, then, when};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
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

#[when(expr = "a {word} build stage leaves a controlled descendant writer")]
fn descendant_stage(world: &mut Bundle, mode: String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--owned-writer-parent")
        .arg(world.root())
        .arg(listener.local_addr().unwrap().to_string())
        .arg(&mode);
    let result = if mode == "nested" || mode == "outertimeout" {
        let mut nested = Command::new(std::env::current_exe().unwrap());
        nested
            .arg("--nested-build")
            .arg(world.root())
            .arg(listener.local_addr().unwrap().to_string())
            .arg(&mode);
        check::Attempt::with_budget(
            world.root(),
            "fixture-source-sha",
            "nested-writer",
            if mode == "outertimeout" {
                Duration::from_millis(250)
            } else {
                Duration::from_secs(2)
            },
        )
        .execute("build", nested, 1, |_| Ok(1))
        .map(|_| String::new())
    } else {
        build::Build::new(world.root(), "fixture-source-sha", "owned-writer").command_with_limits(
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
    for name in ["parent-pid", "child-pid"] {
        world.owned_pids.push(
            fs::read_to_string(world.root().join(name))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    listener.set_nonblocking(true).unwrap();
    let (mut writer, _) = listener
        .accept()
        .expect("the controlled writer must be ready before failure");
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
        command.args([
            "--owned-writer-parent",
            &arguments[1],
            &arguments[2],
            "timeout",
        ]);
        let result = if arguments[3] == "outertimeout" {
            build::Build::new(root, "fixture-source-sha", "inner-writer")
                .command("compiler", command)
        } else {
            build::Build::new(root, "fixture-source-sha", "inner-writer").command_with_limits(
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
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--owned-writer-child", &arguments[1], &arguments[2]])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready, "ready\n");
        println!("OWNED_WRITER_READY");
        if arguments[3] == "failed" {
            std::process::exit(7);
        }
        std::process::exit(child.wait().unwrap().code().unwrap_or(1));
    }
    fs::write(root.join("child-pid"), std::process::id().to_string()).unwrap();
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

fn main() {
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
