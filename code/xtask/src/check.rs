// check.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Metadata-discovered, feature-isolated regression checks with bounded execution.

use crate::{
    assets::{Result, head},
    build::{self, Artifact, artifact, safe_path, verify_artifacts},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Check evidence is separate from build, foundation, and asset attempt records.
pub(crate) const EVIDENCE: &str = "target/check-evidence";
const BUDGET: Duration = Duration::from_secs(600);
const CLIENT: &str = "ai2screenwake_client";
const NATIVE: &str = "ai2screenwake_native";
const BACKEND: &str = "ai2screenwake_backend";
static TIMING: Mutex<()> = Mutex::new(());

/// Only workspace package/test metadata is needed; dependency graph checks run separately.
#[derive(Debug, Deserialize)]
pub(crate) struct Metadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    features: BTreeMap<String, Vec<String>>,
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
}

/// Every maintained integration target gets one explicit feature profile.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Family {
    pub package: String,
    pub target: String,
    pub features: Vec<String>,
    directory: PathBuf,
}

impl Family {
    fn name(&self) -> String {
        format!("{}-{}", self.package, self.target)
    }
}

/// Discover new targets rather than freezing a list which can silently miss future history tests.
pub(crate) fn plan(root: &Path, metadata: Metadata) -> Result<Vec<Family>> {
    let runtime: BTreeSet<_> = metadata
        .packages
        .iter()
        .filter(|package| package.manifest_path.starts_with(root.join("code/crates")))
        .map(|package| package.name.as_str())
        .collect();
    if runtime != BTreeSet::from([BACKEND, CLIENT, NATIVE])
        || metadata.packages.len() != 4
        || !metadata
            .packages
            .iter()
            .any(|package| package.name == "xtask")
    {
        return Err("check requires exactly three runtime packages plus tooling-only xtask".into());
    }
    let mut families = Vec::new();
    for package in metadata.packages {
        if matches!(package.name.as_str(), BACKEND | CLIENT)
            && package
                .features
                .get("default")
                .is_none_or(|features| !features.is_empty())
        {
            return Err(format!("{} must have empty default features", package.name).into());
        }
        let relative = package
            .manifest_path
            .strip_prefix(root)?
            .to_str()
            .ok_or("invalid manifest path")?;
        safe_path(root, relative)?;
        let directory = package
            .manifest_path
            .parent()
            .ok_or("missing package directory")?;
        for target in package
            .targets
            .into_iter()
            .filter(|target| target.kind == ["test"])
        {
            let mut features = target.required_features;
            features.sort();
            features.dedup();
            if features.contains(&"ffi".into()) && features.contains(&"mcp".into()) {
                return Err("runtime test profiles must not unify ffi and MCP features".into());
            }
            if package.name == CLIENT
                && ((target.name == "ui" && features != ["ffi"])
                    || (target.name == "mcp_delivery" && features != ["mcp"]))
            {
                return Err(
                    format!("{} lost its required isolated feature profile", target.name).into(),
                );
            }
            families.push(Family {
                package: package.name.clone(),
                target: target.name,
                features,
                directory: directory.into(),
            });
        }
    }
    let inventory: BTreeSet<_> = families.iter().map(Family::name).collect();
    for (package, targets) in [
        (
            CLIENT,
            &["foundation", "runtime", "ipc", "mcp_delivery", "ui"][..],
        ),
        (NATIVE, &["native"][..]),
        ("xtask", &["assets", "build", "check"][..]),
    ] {
        for target in targets {
            if !inventory.contains(&format!("{package}-{target}")) {
                return Err(
                    format!("required regression family is missing: {package}/{target}").into(),
                );
            }
        }
    }
    if inventory.len() != families.len() {
        return Err("duplicate regression family identity".into());
    }
    Ok(families)
}

/// Accept only one positive, wholly passing Cucumber summary, never guessed feature-line counts.
pub(crate) fn scenario_count(output: &str) -> Result<usize> {
    let mut counts = Vec::new();
    for line in output.lines().map(str::trim) {
        let Some((number, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(count) = number.parse::<usize>() else {
            continue;
        };
        if rest.starts_with("scenario ")
            || rest.starts_with("scenarios ")
            || rest == "scenarios"
            || rest == "scenario"
        {
            let expected = format!(
                "{} ({} passed)",
                if count == 1 { "scenario" } else { "scenarios" },
                count
            );
            if count == 0 || rest != expected {
                return Err(
                    format!("zero, skipped, failed, or malformed scenario result: {line}").into(),
                );
            }
            counts.push(count);
        }
    }
    if counts.len() != 1 {
        return Err("required runner did not report exactly one passing scenario summary".into());
    }
    Ok(counts[0])
}

/// An immutable attempt has a shared deadline even across concurrent family workers.
#[derive(Clone)]
pub(crate) struct Attempt {
    root: PathBuf,
    sha: String,
    id: String,
    deadline: Instant,
}

/// Executed stage results retain observed counts and duration for same-run critical-path comparison.
#[derive(Debug, Serialize)]
pub(crate) struct Outcome {
    stage: String,
    pub count: usize,
    elapsed_seconds: f64,
    #[serde(skip)]
    pub output: String,
}

impl Attempt {
    /// Focused executor tests use this same bounded production runner, not a second implementation.
    pub(crate) fn new(root: &Path, sha: &str, id: &str) -> Self {
        Self {
            root: root.into(),
            sha: sha.into(),
            id: id.into(),
            deadline: Instant::now() + BUDGET,
        }
    }

    /// A named stage fails on process, parser, provenance, timeout, or evidence-write failure.
    pub(crate) fn execute(
        &self,
        stage: &str,
        mut command: Command,
        workers: usize,
        validate: impl FnOnce(&str) -> Result<usize>,
    ) -> Result<Outcome> {
        let description = format!("{command:?}");
        let stdout = safe_path(
            &self.root,
            &format!("{EVIDENCE}/{}-{stage}.stdout", self.id),
        )?;
        let stderr = safe_path(
            &self.root,
            &format!("{EVIDENCE}/{}-{stage}.stderr", self.id),
        )?;
        fs::create_dir_all(stdout.parent().ok_or("missing check evidence parent")?)?;
        command
            .stdout(Stdio::from(File::create(&stdout)?))
            .stderr(Stdio::from(File::create(&stderr)?))
            .process_group(0);
        let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let started = Instant::now();
        let mut exit_status = 1;
        println!("CHECK_STAGE_START {stage} command={description}");
        let result = (|| -> Result<(String, usize)> {
            if Instant::now() >= self.deadline {
                return Err("full check exceeded its ten-minute budget".into());
            }
            let mut child = command.spawn()?;
            let waited = (|| -> Result<_> {
                let mut sizes = (0, 0);
                let mut progress = Instant::now();
                loop {
                    if let Some(status) = child.try_wait()? {
                        return Ok(status);
                    }
                    let observed = (stdout.metadata()?.len(), stderr.metadata()?.len());
                    if observed != sizes {
                        sizes = observed;
                        progress = Instant::now();
                    }
                    if Instant::now() >= self.deadline
                        || progress.elapsed() >= Duration::from_secs(300)
                    {
                        return Err("check stage exceeded its total/progress deadline".into());
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            })();
            if child.try_wait()?.is_none() {
                unsafe extern "C" {
                    fn kill(pid: i32, signal: i32) -> i32;
                }
                // This new process group contains only the stage and its compiler/test descendants.
                if unsafe { kill(-i32::try_from(child.id())?, 9) } != 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(3)
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                child.wait()?;
            }
            let status = waited?;
            exit_status = status.code().unwrap_or(1);
            if !status.success() {
                return Err(format!(
                    "{stage} exited {status}; see {}: {}",
                    stderr.display(),
                    fs::read_to_string(&stderr)?
                )
                .into());
            }
            let output = fs::read_to_string(&stdout)?;
            let count = validate(&output)?;
            Ok((output, count))
        })();
        if result.is_err() && exit_status == 0 {
            exit_status = 1;
        }
        let elapsed_seconds = started.elapsed().as_secs_f64();
        {
            let _guard = TIMING.lock().map_err(|_| "check timing lock poisoned")?;
            build::record_timing(
                &self.root,
                EVIDENCE,
                &json!({
                    "attempt": self.id, "stage": stage, "started_at": started_at,
                    "finished_at": SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
                    "elapsed_seconds": elapsed_seconds, "git_sha": self.sha,
                    "environment": "macos-arm64:explicit-features:regression", "worker_count": workers,
                    "command": description, "exit_status": exit_status,
                    "scenario_count": result.as_ref().ok().map(|(_, count)| count),
                    "error": result.as_ref().err().map(ToString::to_string),
                    "stdout": stdout, "stderr": stderr,
                }),
            )?;
        }
        let (output, count) = result?;
        Ok(Outcome {
            stage: stage.into(),
            count,
            elapsed_seconds,
            output,
        })
    }

    fn cargo(&self, arguments: &[&str]) -> Result<Command> {
        let mut command = Command::new(std::env::var_os("CARGO").ok_or("CARGO unavailable")?);
        command
            .current_dir(&self.root)
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .args(arguments);
        Ok(command)
    }

    fn cargo_stage(&self, stage: &str, arguments: &[&str]) -> Result<Outcome> {
        self.execute(stage, self.cargo(arguments)?, 1, |_| Ok(0))
    }
}

/// Each prepared executable remains tied to its source SHA, feature profile, and support binaries.
#[derive(Clone, Debug, Serialize)]
struct Prepared {
    family: Family,
    executable: PathBuf,
    artifacts: Vec<Artifact>,
    git_sha: String,
}

/// Compile each package/feature group once and identify test executables from Cargo's actual output.
fn prepare(attempt: &Attempt, families: Vec<Family>) -> Result<Vec<Prepared>> {
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for family in families {
        groups
            .entry((family.package.clone(), family.features.clone()))
            .or_default()
            .push(family);
    }
    let mut prepared = Vec::new();
    for ((package, features), families) in groups {
        let mut command = attempt.cargo(&[
            "test",
            "--locked",
            "--no-default-features",
            "-p",
            &package,
            "--no-run",
            "--message-format=json",
        ])?;
        if !features.is_empty() {
            command.arg("--features").arg(features.join(","));
        }
        for family in &families {
            command.arg("--test").arg(&family.target);
        }
        let stage = format!(
            "prepare-{package}-{}",
            if features.is_empty() {
                "default".into()
            } else {
                features.join("+")
            }
        );
        let output = attempt.execute(&stage, command, 1, |_| Ok(0))?.output;
        let mut executables = BTreeMap::new();
        let mut artifacts = Vec::new();
        for line in output.lines() {
            let message: Value = serde_json::from_str(line)?;
            if message["reason"] != "compiler-artifact" {
                continue;
            }
            if let Some(path) = message["executable"].as_str() {
                let path = PathBuf::from(path);
                let relative = path
                    .strip_prefix(&attempt.root)?
                    .to_str()
                    .ok_or("invalid compiler executable path")?;
                if !path.starts_with(attempt.root.join("target")) {
                    return Err("cross-worktree test executable".into());
                }
                artifacts.push(artifact(&attempt.root, relative)?);
                if message["target"]["kind"] == json!(["test"]) {
                    let name = message["target"]["name"]
                        .as_str()
                        .ok_or("missing test target name")?;
                    if executables.insert(name.to_owned(), path).is_some() {
                        return Err("duplicate prepared test executable".into());
                    }
                }
            }
        }
        for family in families {
            let executable = executables.remove(&family.target).ok_or_else(|| {
                format!(
                    "no executable for required family {}/{}",
                    package, family.target
                )
            })?;
            prepared.push(Prepared {
                family,
                executable,
                artifacts: artifacts.clone(),
                git_sha: attempt.sha.clone(),
            });
        }
        if !executables.is_empty() {
            return Err("unexpected prepared test family".into());
        }
    }
    fs::write(
        safe_path(
            &attempt.root,
            &format!("{EVIDENCE}/{}-prepared.json", attempt.id),
        )?,
        serde_json::to_vec_pretty(&prepared)?,
    )?;
    Ok(prepared)
}

/// Short per-family private temp roots keep Unix sockets below Darwin's pathname limit.
fn run_family(attempt: &Attempt, prepared: Prepared, real_host: bool) -> Result<Outcome> {
    if prepared.git_sha != attempt.sha || head(&attempt.root)? != attempt.sha {
        return Err("prepared check family has stale source identity".into());
    }
    verify_artifacts(&attempt.root, &prepared.artifacts)?;
    let temporary = tempfile::Builder::new()
        .prefix("c-")
        .tempdir_in(attempt.root.join("target"))?;
    let workers = if prepared.family.target == "assets" {
        3
    } else {
        1
    };
    let name = format!(
        "{}{}",
        prepared.family.name(),
        if real_host { "-real-host" } else { "" }
    );
    let mut command = Command::new(&prepared.executable);
    command
        .current_dir(&prepared.family.directory)
        .env("TMPDIR", temporary.path())
        .env("TMP", temporary.path())
        .env("TEMP", temporary.path())
        .args(["--concurrency", &workers.to_string(), "--color", "never"]);
    if prepared.family.package == NATIVE {
        command.args([
            "--tags",
            if real_host {
                "@real_host"
            } else {
                "not @real_host"
            },
        ]);
    }
    attempt.execute(&name, command, workers, scenario_count)
}

/// Independent roots use a bounded pool; failures retain all already-running results.
fn parallel(attempt: &Attempt, families: Vec<Prepared>, workers: usize) -> Result<Vec<Outcome>> {
    let queue = Arc::new(Mutex::new(VecDeque::from(families)));
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    thread::scope(|scope| {
        let mut threads = Vec::new();
        for _ in 0..workers {
            let queue = queue.clone();
            let outcomes = outcomes.clone();
            threads.push(scope.spawn(move || -> std::result::Result<(), String> {
                loop {
                    let prepared = queue
                        .lock()
                        .map_err(|_| "family queue poisoned")?
                        .pop_front();
                    let Some(prepared) = prepared else { break };
                    let result =
                        run_family(attempt, prepared, false).map_err(|error| error.to_string());
                    let failed = result.is_err();
                    outcomes
                        .lock()
                        .map_err(|_| "family results poisoned")?
                        .push(result);
                    if failed {
                        break;
                    }
                }
                Ok(())
            }));
        }
        for thread in threads {
            thread
                .join()
                .map_err(|_| "check family worker panicked")??;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    let results = Arc::try_unwrap(outcomes)
        .map_err(|_| "family result ownership remained shared")?
        .into_inner()
        .map_err(|_| "family results poisoned")?;
    results
        .into_iter()
        .map(|result| result.map_err(Into::into))
        .collect()
}

/// Normal production graphs never pick up MCP/server SDKs or tooling-only bindgen.
fn graph(attempt: &Attempt, package: &str, features: &str, empty: bool) -> Result<()> {
    let mut command = attempt.cargo(&[
        "tree",
        "--locked",
        "-p",
        package,
        "--no-default-features",
        "--edges",
        "normal",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ])?;
    if !features.is_empty() {
        command.args(["--features", features]);
    }
    let stage = format!(
        "graph-{package}-{}",
        if features.is_empty() {
            "default"
        } else {
            features
        }
    );
    let output = attempt.execute(&stage, command, 1, |_| Ok(0))?.output;
    let packages: BTreeSet<_> = output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    if (empty && packages != BTreeSet::from([package]))
        || packages.iter().any(|package| {
            [
                "rmcp",
                "tokio",
                "tokio-util",
                "axum",
                "sqlx",
                "schemars",
                "uniffi_bindgen",
            ]
            .contains(package)
        })
    {
        return Err(format!("forbidden dependency in isolated graph {stage}: {packages:?}").into());
    }
    Ok(())
}

/// Preserve all tracked source identities, including generated-lockfile and workflow changes.
fn sources(attempt: &Attempt, phase: &str) -> Result<Vec<Artifact>> {
    let mut status = Command::new("git");
    status
        .current_dir(&attempt.root)
        .args(["status", "--porcelain", "--untracked-files=normal"]);
    if !attempt
        .execute(&format!("source-clean-{phase}"), status, 1, |_| Ok(0))?
        .output
        .is_empty()
    {
        return Err("check requires a clean committed source tree".into());
    }
    let mut files = Command::new("git");
    files.current_dir(&attempt.root).args(["ls-files", "-z"]);
    let files = attempt
        .execute(&format!("source-inventory-{phase}"), files, 1, |_| Ok(0))?
        .output;
    files
        .split('\0')
        .filter(|file| !file.is_empty())
        .map(|file| artifact(&attempt.root, file))
        .collect()
}

/// Run one real completed-wave regression command; long-idle/GUI product acceptance remains separate.
pub(crate) fn run(root: &Path, sha: &str, id: &str) -> Result<usize> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(
            "check requires an arm64 macOS host with actual login-session/native resources".into(),
        );
    }
    let lock = safe_path(root, &format!("{EVIDENCE}/check.lock"))?;
    let _lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(lock)?;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    use std::os::fd::AsRawFd;
    if unsafe { flock(_lock.as_raw_fd(), 2 | 4) } != 0 {
        return Err("another canonical check owns this worktree".into());
    }
    let attempt = Attempt::new(root, sha, id);
    let started = Instant::now();
    let source = sources(&attempt, "before")?;
    let metadata = attempt.cargo_stage(
        "metadata",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
    )?;
    let families = plan(root, serde_json::from_str(&metadata.output)?)?;
    fs::write(
        safe_path(root, &format!("{EVIDENCE}/{id}-plan.json"))?,
        serde_json::to_vec_pretty(&families)?,
    )?;
    let mut license = Command::new("git");
    license
        .current_dir(root)
        .args(["hash-object", "LICENSE.txt"]);
    let output = attempt.execute("license", license, 1, |_| Ok(0))?.output;
    if output.trim() != "90b2643a579327564ed214dee8ec849fbc1732f1" {
        return Err("pinned LICENSE.txt changed".into());
    }
    attempt.cargo_stage("format", &["fmt", "--all", "--", "--check"])?;
    graph(&attempt, BACKEND, "", true)?;
    graph(&attempt, BACKEND, "contract", false)?;
    graph(&attempt, CLIENT, "", false)?;
    graph(&attempt, CLIENT, "ffi", false)?;
    attempt.cargo_stage(
        "backend-empty-default",
        &["check", "--locked", "-p", BACKEND, "--no-default-features"],
    )?;
    attempt.cargo_stage(
        "clippy-default",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--no-default-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    for features in ["ffi", "mcp", "bindgen"] {
        let mut arguments = vec![
            "clippy",
            "--locked",
            "-p",
            CLIENT,
            "--no-default-features",
            "--features",
            features,
        ];
        if features == "bindgen" {
            arguments.extend(["--bin", "uniffi-bindgen"]);
        } else {
            arguments.push("--all-targets");
        }
        arguments.extend(["--", "-D", "warnings"]);
        attempt.cargo_stage(&format!("clippy-{features}"), &arguments)?;
    }
    let prepared = prepare(&attempt, families)?;
    let native = prepared
        .iter()
        .find(|prepared| prepared.family.package == NATIVE && prepared.family.target == "native")
        .ok_or("native resource family was not prepared")?
        .clone();
    let (renderers, host): (Vec<_>, Vec<_>) = prepared.into_iter().partition(|prepared| {
        prepared.family.package == "xtask" && prepared.family.target == "assets"
    });
    let workers = thread::available_parallelism()?.get().min(3);
    let host_count = host.len();
    let parallel_started = Instant::now();
    let mut outcomes = parallel(&attempt, host, workers)?;
    let parallel_seconds = parallel_started.elapsed().as_secs_f64();
    if outcomes.len() != host_count {
        return Err("not every required host family completed".into());
    }
    let sum_family_seconds: f64 = outcomes.iter().map(|outcome| outcome.elapsed_seconds).sum();
    // The renderer consumes three scenario workers and its existing ten-second budget.
    for renderer in renderers {
        outcomes.push(run_family(&attempt, renderer, false)?);
    }
    // Actual IOKit resources/login identity are a serial exception, never a skipped CI gate.
    outcomes.push(run_family(&attempt, native, true)?);
    let build = attempt.cargo_stage(
        "canonical-release-bundle",
        &["xtask", "build", "--profile", "release"],
    )?;
    if !build
        .output
        .lines()
        .any(|line| line.starts_with("BUILD_OK ") && line.contains(sha))
    {
        return Err(
            "canonical release build did not identify a successful current-SHA bundle".into(),
        );
    }
    verify_artifacts(root, &source)?;
    if head(root)? != sha || !sources(&attempt, "after")?.eq(&source) {
        return Err("source changed during canonical check".into());
    }
    let count = outcomes.iter().map(|outcome| outcome.count).sum::<usize>();
    let summary = json!({
        "schema_version": 1, "attempt": id, "git_sha": sha, "command": "cargo xtask check",
        "elapsed_seconds": started.elapsed().as_secs_f64(), "scenario_count": count,
        "families": outcomes, "host_family_workers": workers,
        "lower_worker_reason": if workers < 3 { Some("host logical CPU capacity is below three") } else { None },
        "serial_exceptions": ["three-worker CPU-bound asset renderer", "actual login-session IOKit resources", "dependent canonical native/Swift build"],
        "parallel_host_seconds": parallel_seconds, "observed_host_family_seconds_sum": sum_family_seconds,
        "previous_full_suite_baseline_seconds": null,
        "comparison_note": "No prior full-suite baseline exists; same-run family-duration sum is not a historical serial benchmark",
        "canonical_bundle": build::TARGET, "product_acceptance_proved": false,
        "remaining_acceptance": ["controlled screensaver/idle", "real GUI/MCP golden path", "support matrix/audit", "release"],
    });
    fs::write(
        safe_path(root, &format!("{EVIDENCE}/{id}-summary.json"))?,
        serde_json::to_vec_pretty(&summary)?,
    )?;
    if started.elapsed() >= BUDGET {
        return Err("full check exceeded its ten-minute budget".into());
    }
    println!(
        "CHECK_RESULT scenarios={count} workers={workers} elapsed_seconds={:.6} sha={sha}",
        started.elapsed().as_secs_f64()
    );
    Ok(count)
}
// end check.rs
