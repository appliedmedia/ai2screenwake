// check.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Focused runner requirements; this executable never invokes the full check.

use cucumber::{World, given, then, when};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

#[path = "../../src/main.rs"]
#[allow(dead_code)]
mod foundation_runner;
use foundation_runner::{assets, build, check};

#[derive(Debug, Default, World)]
struct Runner {
    output: Option<std::process::Output>,
    fixture: Option<tempfile::TempDir>,
    error: Option<String>,
    families: Vec<check::Family>,
    lanes: Vec<PathBuf>,
}

impl Runner {
    fn root(&self) -> &Path {
        self.fixture.as_ref().unwrap().path()
    }

    fn timing(&self) -> serde_json::Value {
        serde_json::from_str(
            fs::read_to_string(self.root().join(check::EVIDENCE).join("timing.jsonl"))
                .unwrap()
                .trim(),
        )
        .unwrap()
    }
}

#[given("an isolated runner evidence directory")]
fn fixture(world: &mut Runner) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    world.fixture = Some(
        tempfile::Builder::new()
            .prefix("check-requirements-")
            .tempdir_in(parent.canonicalize().unwrap())
            .unwrap(),
    );
}

#[when("the check command receives an unsupported argument")]
fn unsupported(world: &mut Runner) {
    world.output = Some(
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["check", "--unsupported"])
            .output()
            .unwrap(),
    );
}

#[then("it rejects the argument before running any regression family")]
fn rejected(world: &mut Runner) {
    let output = world.output.as_ref().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("check accepts no arguments"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("CHECK_STAGE"));
}

fn workspace_metadata() -> (PathBuf, serde_json::Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let output = Command::new(std::env::var_os("CARGO").unwrap())
        .current_dir(&root)
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    (root, serde_json::from_slice(&output.stdout).unwrap())
}

#[when("Cargo metadata gains a history test target")]
fn new_target(world: &mut Runner) {
    let (root, mut metadata) = workspace_metadata();
    let client = metadata["packages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|package| package["name"] == "ai2screenwake_client")
        .unwrap();
    client["targets"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "new_history_requirement", "kind": ["test"], "required-features": []
        }));
    world.families = check::plan(&root, serde_json::from_value(metadata).unwrap()).unwrap();
}

#[when("current native UI regression passes are planned")]
fn ui_passes(world: &mut Runner) {
    let (root, metadata) = workspace_metadata();
    world.families = check::plan(&root, serde_json::from_value(metadata).unwrap()).unwrap();
}

#[then("native UI resource cases have a separate serial tag partition")]
fn ui_resource_partition(world: &mut Runner) {
    let family = world
        .families
        .iter()
        .find(|family| family.package == "ai2screenwake_client" && family.target == "ui")
        .unwrap();
    let passes = check::test_passes(family);
    let actual = passes
        .iter()
        .find(|pass| pass.real_host)
        .expect("UI real-host partition is missing");
    let deterministic = passes
        .iter()
        .find(|pass| !pass.real_host)
        .expect("UI deterministic partition is missing");
    assert!(
        actual.serial,
        "actual NativeHost/IOKit/IPC must run serially"
    );
    assert!(!deterministic.serial);
    assert!(
        actual
            .arguments
            .windows(2)
            .any(|pair| pair == ["--tags", "@real_host"])
    );
    assert!(
        deterministic
            .arguments
            .windows(2)
            .any(|pair| pair == ["--tags", "not @real_host"])
    );
}

#[then("the plan includes history and keeps native and MCP profiles separate")]
fn planned(world: &mut Runner) {
    assert!(
        world
            .families
            .iter()
            .any(|family| family.target == "new_history_requirement")
    );
    let ui = world
        .families
        .iter()
        .find(|family| family.target == "ui")
        .unwrap();
    let mcp = world
        .families
        .iter()
        .find(|family| family.target == "mcp_delivery")
        .unwrap();
    assert_eq!(ui.features, ["ffi"]);
    assert_eq!(mcp.features, ["mcp"]);
    assert!(
        world
            .families
            .iter()
            .all(|family| !(family.features.contains(&"ffi".into())
                && family.features.contains(&"mcp".into())))
    );
}

#[when("a required stage exits with status seven")]
fn failed_stage(world: &mut Runner) {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 7"]);
    world.error = Some(
        check::Attempt::new(world.root(), "fixture-sha", "failure")
            .execute("required-compiler", command, 1, |_| Ok(0))
            .unwrap_err()
            .to_string(),
    );
}

#[then("the failed stage records status seven and its source identity")]
fn failed_evidence(world: &mut Runner) {
    assert!(
        world
            .error
            .as_ref()
            .unwrap()
            .contains("required-compiler exited")
    );
    let timing = world.timing();
    assert_eq!(timing["stage"], "required-compiler");
    assert_eq!(timing["exit_status"], 7);
    assert_eq!(timing["git_sha"], "fixture-sha");
    assert!(timing["elapsed_seconds"].as_f64().unwrap() >= 0.0);
}

#[when("a required runner reports zero scenarios")]
fn zero(world: &mut Runner) {
    let mut command = Command::new("/usr/bin/printf");
    command.args(["%s\\n", "0 scenarios"]);
    world.error = Some(
        check::Attempt::new(world.root(), "fixture-sha", "zero")
            .execute("required-tests", command, 1, check::scenario_count)
            .unwrap_err()
            .to_string(),
    );
}

#[then("the named gate fails and records the zero-selection failure")]
fn zero_evidence(world: &mut Runner) {
    assert!(
        world
            .error
            .as_ref()
            .unwrap()
            .contains("zero, skipped, failed, or malformed")
    );
    let timing = world.timing();
    assert_eq!(timing["stage"], "required-tests");
    assert_eq!(timing["exit_status"], 1);
    assert!(timing["scenario_count"].is_null());
    assert!(timing["error"].as_str().unwrap().contains("0 scenarios"));
}

#[when("three isolated executor lanes run together")]
fn isolated(world: &mut Runner) {
    let mut threads = Vec::new();
    for lane in 0..3 {
        let root = world.root().join(format!("lane-{lane}"));
        fs::create_dir(&root).unwrap();
        world.lanes.push(root.clone());
        threads.push(std::thread::spawn(move || {
            let mut command = Command::new("/usr/bin/printf");
            command.args(["%s\\n", &format!("lane-{lane}"), "1 scenario (1 passed)"]);
            check::Attempt::new(&root, "fixture-sha", "isolated")
                .execute("required-tests", command, 1, check::scenario_count)
                .map(|outcome| outcome.count)
                .map_err(|error| error.to_string())
        }));
    }
    for thread in threads {
        assert_eq!(thread.join().unwrap().unwrap(), 1);
    }
}

#[then("every lane retains only its own output and passing count")]
fn isolated_evidence(world: &mut Runner) {
    for (lane, root) in world.lanes.iter().enumerate() {
        let output = fs::read_to_string(
            root.join(check::EVIDENCE)
                .join("isolated-required-tests.stdout"),
        )
        .unwrap();
        assert_eq!(output, format!("lane-{lane}\n1 scenario (1 passed)\n"));
        let rows = fs::read_to_string(root.join(check::EVIDENCE).join("timing.jsonl")).unwrap();
        let timing: serde_json::Value = serde_json::from_str(rows.trim()).unwrap();
        assert_eq!(timing["scenario_count"], 1);
        assert_eq!(timing["exit_status"], 0);
    }
}

fn main() {
    if build::supervisor_entry() {
        return;
    }
    if controlled_preparation() {
        return;
    }
    let started = Instant::now();
    futures::executor::block_on(
        Runner::cucumber()
            .max_concurrent_scenarios(3)
            .fail_on_skipped()
            .run_and_exit(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features/check.feature"),
            ),
    );
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "CHECK_SCENARIOS_OK workers=3 elapsed_seconds={:.6}",
        started.elapsed().as_secs_f64()
    );
}
#[when(expr = "foundation source {word} changes after its executable compiles")]
fn changed_preparation(world: &mut Runner, identity: String) {
    fs::create_dir_all(world.root().join("code")).unwrap();
    fs::create_dir_all(world.root().join(".cargo")).unwrap();
    fs::create_dir_all(world.root().join("target/foundation-evidence")).unwrap();
    fs::write(world.root().join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::write(world.root().join("Cargo.lock"), "version=4\n").unwrap();
    fs::write(world.root().join(".cargo/config.toml"), "").unwrap();
    fs::write(
        world.root().join("code/main.rs"),
        "fn main() { println!(\"compiled A\"); }\n",
    )
    .unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=RunnerTests",
            "-c",
            "user.email=runner@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        assert!(
            Command::new("git")
                .current_dir(world.root())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    let sha = assets::head(world.root()).unwrap();
    let mut cargo = Command::new(std::env::current_exe().unwrap());
    cargo.arg("--controlled-preparation").arg(identity);
    world.error = foundation_runner::prepare_command(world.root(), &sha, "racing-input", cargo)
        .err()
        .map(|error| error.to_string());
}

#[then("preparation rejects changed identity without publishing a receipt")]
fn preparation_rejected(world: &mut Runner) {
    assert!(
        world
            .error
            .as_ref()
            .is_some_and(|error| error.contains("source changed during preparation")),
        "{:?}",
        world.error
    );
    assert!(
        !world
            .root()
            .join("target/foundation-evidence/prepared.json")
            .exists()
    );
}

fn controlled_preparation() -> bool {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_none_or(|arg| arg != "--controlled-preparation")
    {
        return false;
    }
    assert!(
        Command::new("rustc")
            .args(["code/main.rs", "-o", "target/compiled-runner"])
            .status()
            .unwrap()
            .success()
    );
    if args[1] == "bytes" {
        fs::write(
            "code/main.rs",
            "fn main() { println!(\"new source B\"); }\n",
        )
        .unwrap();
    } else {
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.name=RunnerTests",
                    "-c",
                    "user.email=runner@example.invalid",
                    "-c",
                    "commit.gpgSign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "commit",
                    "--allow-empty",
                    "--quiet",
                    "-m",
                    "changed"
                ])
                .status()
                .unwrap()
                .success()
        );
    }
    println!(
        "{}",
        serde_json::json!({"reason":"compiler-artifact", "target":{"name":"foundation"}, "executable":std::env::current_dir().unwrap().join("target/compiled-runner")})
    );
    true
}
// end check.rs
