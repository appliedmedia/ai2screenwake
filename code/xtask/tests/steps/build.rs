// build.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Focused executable requirements for canonical native packaging.

use cucumber::{World, given, then, when};
use std::{fs, path::Path, process::Command, time::Instant};

#[path = "../../src/assets.rs"]
mod assets;
#[path = "../../src/build.rs"]
mod build;

#[derive(Debug, Default, World)]
struct Bundle {
    output: Option<std::process::Output>,
    fixture: Option<tempfile::TempDir>,
    error: Option<String>,
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
    let result = build::Build::new(world.root(), "fixture-source-sha", "failed-stage")
        .command("compiler", Command::new("/usr/bin/false"));
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
    assert_eq!(row["exit_status"], 1);
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

fn main() {
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
