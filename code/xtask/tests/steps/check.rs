// check.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Focused runner requirements; this executable never invokes the full check.

use cucumber::{World, given, then, when};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

#[path = "../../src/assets.rs"]
#[allow(dead_code)]
mod assets;
#[path = "../../src/build.rs"]
#[allow(dead_code)]
mod build;
#[path = "../../src/check.rs"]
#[allow(dead_code)]
mod check;

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

#[when("Cargo metadata gains a history test target")]
fn new_target(world: &mut Runner) {
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
    let mut metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
// end check.rs
