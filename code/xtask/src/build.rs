// build.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! One native release recipe, with source identity and failure-safe publication.

use crate::assets::{self, Result, checksum, head};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "build/owned_process.rs"]
mod owned_process;
pub(crate) use owned_process::{OwnedChild, supervisor_entry};
#[path = "build/version.rs"]
pub(crate) mod version;
use version::BuildVersion;

/// The only consumable application target; intermediate products are not apps.
pub(crate) const TARGET: &str = "target/ai2sw/ai2sw.app";
/// Immutable attempt logs, stage timings, and signed-output receipts live outside the bundle.
pub(crate) const EVIDENCE: &str = "target/ai2sw/build-evidence";
const PLATFORM: &str = "aarch64-apple-darwin";
const MACOS: &str = "code/macos/ScreenWake";
const GENERATED: &str = "target/ai2sw/generated";
const LIBRARY: &str =
    "target/ai2sw/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client";
const MCP: &str = "target/ai2sw/mcp-target/aarch64-apple-darwin/release/ai2screenwake-mcp";
const GENERATOR: &str = "target/ai2sw/bindgen-tool/debug/uniffi-bindgen";
const RESOURCES: [&str; 13] = [
    "app.icns",
    "menu-off.png",
    "menu-off@2x.png",
    "menu-on.png",
    "menu-on@2x.png",
    "menu-error.png",
    "menu-error@2x.png",
    "preview-off.png",
    "preview-off@2x.png",
    "preview-on.png",
    "preview-on@2x.png",
    "preview-error.png",
    "preview-error@2x.png",
];
const BINDINGS: [&str; 3] = [
    "screenwake.swift",
    "screenwakeFFI.h",
    "screenwakeFFI.modulemap",
];
const REQUIRED: [&str; 9] = [
    "code/macos/ScreenWake/main.swift",
    "code/macos/ScreenWake/MenuBarController.swift",
    "code/macos/ScreenWake/Info.plist",
    "code/macos/ScreenWake/MenuBarContractTests.swift",
    "code/crates/ai2screenwake_client/src/ffi.rs",
    "code/crates/ai2screenwake_client/src/bin/uniffi-bindgen.rs",
    "code/crates/ai2screenwake_client/src/bin/ai2screenwake-mcp.rs",
    "Cargo.lock",
    "LICENSE.txt",
];

/// Retain the actual tool status in evidence while the command boundary stays nonzero.
#[derive(Debug)]
struct CommandFailure {
    status: ExitStatus,
    message: String,
}

impl std::fmt::Display for CommandFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandFailure {}

/// Resolve only ordinary worktree-relative paths, rejecting symlinked ancestors.
pub(crate) fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("unsafe build path: {relative}").into());
    }
    let path = root.join(relative);
    for ancestor in path.ancestors().take_while(|path| *path != root) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(
                    format!("build symlink is not admissible: {}", ancestor.display()).into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

/// Missing producer sources fail before compilation or any application replacement.
pub(crate) fn require_inputs(root: &Path) -> Result<()> {
    for relative in REQUIRED {
        let path = safe_path(root, relative)?;
        if !path.is_file() || path.metadata()?.len() == 0 {
            return Err(format!("build preflight: missing production input: {relative}").into());
        }
    }
    safe_path(root, TARGET)?;
    safe_path(root, GENERATED)?;
    Ok(())
}

/// Byte and executable-mode identity, independent of file dates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub path: String,
    sha256: String,
    bytes: u64,
    mode: u32,
}

/// Record only regular nonempty files, never references into another checkout.
pub(crate) fn artifact(root: &Path, relative: &str) -> Result<Artifact> {
    let path = safe_path(root, relative)?;
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!("missing or empty build artifact: {relative}").into());
    }
    Ok(Artifact {
        path: relative.into(),
        sha256: checksum(&path)?,
        bytes: metadata.len(),
        mode: metadata.permissions().mode() & 0o777,
    })
}

/// Recheck all identities immediately before a dependent consumption/publication.
pub(crate) fn verify_artifacts(root: &Path, expected: &[Artifact]) -> Result<()> {
    for record in expected {
        if artifact(root, &record.path)? != *record {
            return Err(format!("stale or mixed build artifact: {}", record.path).into());
        }
    }
    Ok(())
}

/// Append the same machine-readable timing shape used by existing xtask commands.
pub(crate) fn record_timing(root: &Path, directory: &str, row: &Value) -> Result<()> {
    let directory = safe_path(root, directory)?;
    fs::create_dir_all(&directory)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(safe_path(
            root,
            &format!("{}/timing.jsonl", directory.strip_prefix(root)?.display()),
        )?)?;
    writeln!(file, "{row}")?;
    file.flush()?;
    println!("{row}");
    Ok(())
}

/// One attempt keeps failed commands/logs rather than overwriting evidence.
pub(crate) struct Build<'a> {
    root: &'a Path,
    sha: &'a str,
    attempt: &'a str,
    commands: Vec<Value>,
    cancelled: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    startup: Option<Box<dyn FnOnce() -> Result<()> + Send + 'a>>,
}

impl<'a> Build<'a> {
    /// The caller has already resolved the exact worktree and current HEAD.
    pub(crate) fn new(root: &'a Path, sha: &'a str, attempt: &'a str) -> Self {
        Self {
            root,
            sha,
            attempt,
            commands: Vec::new(),
            cancelled: None,
            #[cfg(test)]
            startup: None,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)] // Only the integration harness injects startup; the binary test target does not.
    pub(crate) fn after_startup(mut self, ready: impl FnOnce() -> Result<()> + Send + 'a) -> Self {
        self.startup = Some(Box::new(ready));
        self
    }

    /// One owned compiler may overlap an independent slice; errors cancel and join its exact tree.
    pub(crate) fn alongside<T>(
        &mut self,
        stage: &str,
        command: Command,
        operation: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<(String, T)> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut compiler = Build::new(self.root, self.sha, self.attempt);
        compiler.cancelled = Some(cancelled.clone());
        thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let result = compiler
                    .command(stage, command)
                    .map_err(|error| error.to_string());
                (result, compiler.commands)
            });
            let outcome = operation(self);
            if outcome.is_err() {
                cancelled.store(true, Ordering::Release);
            }
            let (compiled, commands) = worker
                .join()
                .map_err(|_| "owned parallel compiler panicked")?;
            self.commands.extend(commands);
            match (compiled, outcome) {
                (Ok(output), Ok(value)) => Ok((output, value)),
                (Err(compile), Err(error)) => {
                    Err(format!("{error}; sibling compiler: {compile}").into())
                }
                (Err(error), _) => Err(error.into()),
                (_, Err(error)) => Err(error),
            }
        })
    }

    /// Failed in-process stages receive the same timing and error evidence as compilers.
    fn stage<T>(
        &mut self,
        name: &str,
        command: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let started = Instant::now();
        let result = operation();
        let exit_status = result.as_ref().err().map_or(0, |error| {
            error
                .downcast_ref::<CommandFailure>()
                .and_then(|error| error.status.code())
                .unwrap_or(1)
        });
        record_timing(
            self.root,
            EVIDENCE,
            &json!({
                "attempt": self.attempt, "stage": name, "started_at": started_at,
                "finished_at": SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
                "elapsed_seconds": started.elapsed().as_secs_f64(), "git_sha": self.sha,
                "environment": "macos-arm64:release:macos13.0", "worker_count": 1,
                "execution_reason": "one stage driver; only feature-stable Swift and Rust MCP may overlap with reserved compiler capacity",
                "exit_status": exit_status, "command": command,
                "error": result.as_ref().err().map(ToString::to_string),
            }),
        )?;
        result
    }

    /// Stream child output to observable logs; a silent five-minute stage fails.
    pub(crate) fn command(&mut self, stage: &str, command: Command) -> Result<String> {
        self.command_with_limits(
            stage,
            command,
            Duration::from_secs(300),
            Duration::from_secs(1200),
        )
    }

    /// Inject shorter bounds for executor requirements without a separate execution path.
    pub(crate) fn command_with_limits(
        &mut self,
        stage: &str,
        mut command: Command,
        progress_bound: Duration,
        total_bound: Duration,
    ) -> Result<String> {
        if command.get_current_dir().is_none() {
            command.current_dir(self.root);
        }
        let description = format!("{command:?}");
        let stdout = format!("{EVIDENCE}/{}-{stage}.stdout", self.attempt);
        let stderr = format!("{EVIDENCE}/{}-{stage}.stderr", self.attempt);
        let out_path = safe_path(self.root, &stdout)?;
        let err_path = safe_path(self.root, &stderr)?;
        fs::create_dir_all(out_path.parent().ok_or("missing build log parent")?)?;
        command
            .stdout(Stdio::from(File::create(&out_path)?))
            .stderr(Stdio::from(File::create(&err_path)?));
        self.commands.push(json!({
            "stage": stage, "command": description, "stdout": stdout, "stderr": stderr,
        }));
        let cancelled = self.cancelled.clone();
        #[cfg(test)]
        let startup = self.startup.take();
        self.stage(stage, &description, || {
            println!("BUILD_STAGE_START {stage} stdout={stdout} stderr={stderr}");
            let mut child = OwnedChild::spawn(command, &out_path, &err_path)?;
            let result = (|| -> Result<ExitStatus> {
                #[cfg(test)]
                if let Some(ready) = startup { ready()?; }
                let started = Instant::now();
                let mut progress = Instant::now();
                let mut sizes = (0, 0);
                loop {
                    if cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Acquire)) {
                        return Err(format!("{stage}: cancelled after independent build failure").into());
                    }
                    if let Some(status) = child.try_wait()? {
                        return Ok(status);
                    }
                    let observed = (out_path.metadata()?.len(), err_path.metadata()?.len());
                    if observed != sizes {
                        sizes = observed;
                        progress = Instant::now();
                    }
                    if progress.elapsed() >= progress_bound
                        || started.elapsed() >= total_bound
                    {
                        return Err(format!(
                            "{stage}: build stage exceeded its progress/deadline bound; logs retained"
                        )
                        .into());
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            })();
            let result = result.and_then(|status| {
                if !status.success() {
                    return Err(CommandFailure {
                        status,
                        message: format!(
                            "{stage} exited {status}; {}",
                            fs::read_to_string(&err_path)?
                        ),
                    }.into());
                }
                Ok(fs::read_to_string(&out_path)?)
            });
            child.finish(result.is_err()).map_err(|error| {
                    format!("{stage}: cleanup failed after {result:?}: {error}")
                })?;
            result
        })
    }
}

/// Hash every tracked source/configuration file; no inferred or dirty commit identity.
fn source_identity(root: &Path, sha: &str) -> Result<Vec<Artifact>> {
    if head(root)? != sha {
        return Err("build source HEAD changed".into());
    }
    let status = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("build requires a clean committed source tree".into());
    }
    let files = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z"])
        .output()?;
    if !files.status.success() {
        return Err("build cannot enumerate tracked source inputs".into());
    }
    files
        .stdout
        .split(|byte| *byte == 0)
        .filter(|file| !file.is_empty())
        .map(|file| artifact(root, std::str::from_utf8(file)?))
        .collect()
}

/// Feature-isolated invocations share compiler units, never mutable consumer outputs.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Producer {
    Ffi,
    Bindgen,
    Mcp,
}

impl Producer {
    fn feature(self) -> &'static str {
        match self {
            Self::Ffi => "ffi",
            Self::Bindgen => "bindgen",
            Self::Mcp => "mcp",
        }
    }

    fn target(self) -> &'static str {
        match self {
            Self::Ffi => "ai2screenwake_client",
            Self::Bindgen => "uniffi-bindgen",
            Self::Mcp => "ai2screenwake-mcp",
        }
    }

    fn release(self) -> bool {
        !matches!(self, Self::Bindgen)
    }

    /// One actual Cargo command; no feature unification or profile/configuration shortcut.
    pub(crate) fn command(self, root: &Path) -> Result<Command> {
        let mut command = Command::new(std::env::var_os("CARGO").ok_or("CARGO is unavailable")?);
        command
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("AI2SW_BUILD_VERSION")
            .env_remove("AI2SW_BUILD_STAMP")
            .args(["build", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .args([
                "--locked",
                "--no-default-features",
                "-p",
                "ai2screenwake_client",
            ]);
        if self.release() {
            command
                .env("CARGO_PROFILE_RELEASE_STRIP", "none")
                .env("MACOSX_DEPLOYMENT_TARGET", "13.0")
                .env(
                    "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
                    "-C link-arg=-mmacosx-version-min=13.0",
                )
                .args(["--release", "--target", PLATFORM]);
        }
        command.args(["--features", self.feature()]);
        if matches!(self, Self::Ffi) {
            command.arg("--lib");
        } else {
            command.args(["--bin", self.target()]);
        }
        command
            .args(["--message-format=json", "--target-dir"])
            .arg(safe_path(root, "target")?);
        Ok(command)
    }

    /// Explicit invocation identity, never an ambient date or per-compiler counter.
    pub(crate) fn command_for(
        self,
        root: &Path,
        version: &BuildVersion,
        sha: &str,
    ) -> Result<Command> {
        let mut command = self.command(root)?;
        command
            .env("AI2SW_BUILD_VERSION", version.display())
            .env("AI2SW_BUILD_STAMP", version.stamp(sha));
        Ok(command)
    }

    /// Capture only the exact current successful compiler result before another feature runs.
    #[cfg(test)]
    #[allow(dead_code)] // Integration fixtures also exercise deliberately unversioned compiler receipts.
    pub(crate) fn snapshot(self, root: &Path, sha: &str, output: &str) -> Result<ProducerReceipt> {
        self.snapshot_config(root, sha, output, None)
    }

    /// Canonical capture refuses a stale version even at the same source SHA and profile.
    pub(crate) fn snapshot_for(
        self,
        root: &Path,
        sha: &str,
        output: &str,
        version: &BuildVersion,
    ) -> Result<ProducerReceipt> {
        self.snapshot_config(root, sha, output, Some(version))
    }

    fn snapshot_config(
        self,
        root: &Path,
        sha: &str,
        output: &str,
        version: Option<&BuildVersion>,
    ) -> Result<ProducerReceipt> {
        if head(root)? != sha {
            return Err("compiler producer source HEAD changed".into());
        }
        let expected_features: BTreeSet<_> = if matches!(self, Self::Bindgen) {
            BTreeSet::from(["bindgen", "ffi"])
        } else {
            BTreeSet::from([self.feature()])
        };
        let mut candidate = None;
        let mut finished = false;
        for line in output.lines() {
            let message: Value = serde_json::from_str(line)?;
            if message["reason"] == "build-finished" {
                if message["success"] != true {
                    return Err("compiler producer did not finish successfully".into());
                }
                finished = true;
            }
            if message["reason"] != "compiler-artifact"
                || message["target"]["name"] != self.target()
            {
                continue;
            }
            let features: BTreeSet<_> = message["features"]
                .as_array()
                .ok_or("missing compiler features")?
                .iter()
                .map(|value| value.as_str().ok_or("invalid compiler feature"))
                .collect::<std::result::Result<_, _>>()?;
            let source = if matches!(self, Self::Ffi) {
                "code/crates/ai2screenwake_client/src/lib.rs".to_owned()
            } else {
                format!(
                    "code/crates/ai2screenwake_client/src/bin/{}.rs",
                    self.target()
                )
            };
            if features != expected_features
                || message["manifest_path"]
                    != json!(root.join("code/crates/ai2screenwake_client/Cargo.toml"))
                || message["target"]["src_path"] != json!(root.join(source))
                || message["profile"]["test"] != false
                || message["profile"]["opt_level"] != if self.release() { "3" } else { "0" }
                || (self.release() && message["profile"]["debug_assertions"] != false)
                || (matches!(self, Self::Ffi)
                    && message["target"]["crate_types"] != json!(["rlib", "staticlib", "cdylib"]))
                || (!matches!(self, Self::Ffi) && message["target"]["kind"] != json!(["bin"]))
            {
                return Err("compiler producer feature/profile/source identity mismatch".into());
            }
            if candidate.replace(message).is_some() {
                return Err("duplicate compiler producer artifact".into());
            }
        }
        if !finished {
            return Err("missing successful compiler completion".into());
        }
        let message = candidate.ok_or("missing compiler producer artifact")?;
        let directory = if self.release() {
            format!("target/{PLATFORM}/release")
        } else {
            "target/debug".into()
        };
        let products = if matches!(self, Self::Ffi) {
            vec![
                (
                    "libai2screenwake_client.a".to_owned(),
                    format!("{LIBRARY}.a"),
                ),
                (
                    "libai2screenwake_client.dylib".to_owned(),
                    format!("{LIBRARY}.dylib"),
                ),
            ]
        } else {
            vec![(
                self.target().to_owned(),
                if matches!(self, Self::Mcp) {
                    MCP
                } else {
                    GENERATOR
                }
                .to_owned(),
            )]
        };
        let filenames = message["filenames"]
            .as_array()
            .ok_or("missing compiler output paths")?;
        if let Some(version) = version {
            for (name, _) in &products {
                let source = safe_path(root, &format!("{directory}/{name}"))?;
                version.verify_bytes(&fs::read(source)?, sha)?;
            }
        }
        let mut origins = Vec::new();
        let mut snapshots = Vec::new();
        for (name, destination) in products {
            let source = safe_path(root, &format!("{directory}/{name}"))?;
            if filenames
                .iter()
                .filter(|value| **value == json!(source))
                .count()
                != 1
                || (!matches!(self, Self::Ffi) && message["executable"] != json!(source))
            {
                return Err("compiler producer output is not the canonical worktree target".into());
            }
            origins.push(artifact(
                root,
                source
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("invalid compiler path")?,
            )?);
            let destination = safe_path(root, &destination)?;
            let parent = destination.parent().ok_or("missing snapshot parent")?;
            fs::create_dir_all(parent)?;
            let temporary = tempfile::NamedTempFile::new_in(parent)?;
            fs::copy(&source, temporary.path())?;
            let mut copied = artifact(
                root,
                temporary
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("invalid temporary snapshot path")?,
            )?;
            let origin = origins.last().expect("recorded origin");
            if copied.sha256 != origin.sha256
                || copied.bytes != origin.bytes
                || copied.mode != origin.mode
            {
                return Err("compiler output changed while taking feature snapshot".into());
            }
            temporary.persist(&destination)?;
            copied.path = destination
                .strip_prefix(root)?
                .to_str()
                .ok_or("invalid snapshot path")?
                .to_owned();
            snapshots.push(copied);
        }
        if head(root)? != sha {
            return Err("compiler producer source HEAD changed".into());
        }
        Ok(ProducerReceipt {
            git_sha: sha.to_owned(),
            feature: self.feature(),
            profile: if self.release() { "release" } else { "dev" },
            compiler_artifacts: origins,
            snapshots,
            build_version: version.cloned(),
        })
    }
}

/// Same-checkout producer provenance remains distinct from feature-stable consumer identities.
#[derive(Debug, Serialize)]
pub(crate) struct ProducerReceipt {
    git_sha: String,
    feature: &'static str,
    profile: &'static str,
    compiler_artifacts: Vec<Artifact>,
    pub(crate) snapshots: Vec<Artifact>,
    build_version: Option<BuildVersion>,
}

/// Production has no test define; the separate non-UI contract harness never enters the app.
fn swift(root: &Path, executable: &Path, contract_test: bool) -> Result<Command> {
    let mut command = Command::new("xcrun");
    command
        .args([
            "swiftc",
            "-warnings-as-errors",
            "-O",
            "-target",
            "arm64-apple-macosx13.0",
            "-I",
        ])
        .arg(safe_path(root, GENERATED)?)
        .arg("-Xcc")
        .arg(format!(
            "-fmodule-map-file={}",
            safe_path(root, &format!("{GENERATED}/screenwakeFFI.modulemap"))?.display()
        ))
        .arg(safe_path(root, &format!("{GENERATED}/screenwake.swift"))?)
        .arg(safe_path(
            root,
            &format!("{MACOS}/MenuBarController.swift"),
        )?)
        .arg(safe_path(root, &format!("{MACOS}/main.swift"))?);
    if contract_test {
        command
            .args(["-D", "SCREENWAKE_CONTRACT_TESTS"])
            .arg(safe_path(
                root,
                &format!("{MACOS}/MenuBarContractTests.swift"),
            )?);
    }
    command.arg(safe_path(root, &format!("{LIBRARY}.a"))?);
    for framework in [
        "AppKit",
        "CoreGraphics",
        "Security",
        "CoreServices",
        "IOKit",
        "CoreFoundation",
    ] {
        command.args(["-framework", framework]);
    }
    command.arg("-liconv").arg("-o").arg(executable);
    Ok(command)
}

/// Enumerate a generated tree without following links or accepting special files.
fn inventory(directory: &Path) -> Result<BTreeSet<String>> {
    fn walk(base: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(base, &entry.path(), files)?;
            } else if kind.is_file() {
                files.insert(entry.path().strip_prefix(base)?.to_string_lossy().into());
            } else {
                return Err(format!("non-regular build output: {}", entry.path().display()).into());
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    walk(directory, directory, &mut files)?;
    Ok(files)
}

/// Native generation must not leave an old binding behind after a failed generator.
fn prepare_bindings(root: &Path) -> Result<()> {
    let directory = safe_path(root, GENERATED)?;
    if directory.exists() {
        let expected = BINDINGS.into_iter().map(String::from).collect();
        if !inventory(&directory)?.is_subset(&expected) {
            return Err(
                "unrecognized files in generated bindings; refusing to discard them".into(),
            );
        }
        for name in BINDINGS {
            let path = safe_path(root, &format!("{GENERATED}/{name}"))?;
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    } else {
        fs::create_dir_all(directory)?;
    }
    Ok(())
}

/// The fixed signed bundle inventory rejects accidental test binaries or user files.
pub(crate) fn bundle_paths() -> BTreeSet<String> {
    [
        "Contents/Info.plist",
        "Contents/MacOS/ai2sw",
        "Contents/MacOS/ai2screenwake-mcp",
        "Contents/Resources/build-provenance.json",
        "Contents/Resources/LICENSE.txt",
        "Contents/_CodeSignature/CodeResources",
    ]
    .into_iter()
    .map(String::from)
    .chain(
        RESOURCES
            .into_iter()
            .map(|name| format!("Contents/Resources/{name}")),
    )
    .collect()
}

fn verify_previous(root: &Path, destination: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let paths = inventory(destination)?;
    let current = bundle_paths();
    let predecessor = current
        .iter()
        .filter(|path| !path.contains("-error"))
        .cloned()
        .collect();
    if paths != current && paths != predecessor {
        return Err("refusing to replace unrecognized application files".into());
    }
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    let uid = unsafe { geteuid() };
    let owner = fs::symlink_metadata(destination)?;
    if !owner.is_dir() || owner.uid() != uid || owner.mode() & 0o022 != 0 {
        return Err("previous application directory is not safely owned output".into());
    }
    for path in &paths {
        let metadata = safe_path(destination, path)?.metadata()?;
        if metadata.uid() != uid || metadata.nlink() != 1 {
            return Err("previous application is not owned regular output".into());
        }
    }
    let previous: Value = serde_json::from_slice(&fs::read(
        destination.join("Contents/Resources/build-provenance.json"),
    )?)?;
    if previous["schema_version"] != 1
        || previous["source_root"] != json!(root)
        || previous["target"] != TARGET
        || previous["configuration"] != "release"
    {
        return Err("refusing to replace an application without this worktree's provenance".into());
    }
    let payload: Vec<Artifact> = serde_json::from_value(previous["unsigned_payload"].clone())?;
    let expected: BTreeSet<_> = paths
        .iter()
        .filter(|path| {
            !path.ends_with("build-provenance.json") && !path.contains("_CodeSignature/")
        })
        .cloned()
        .collect();
    if payload
        .iter()
        .map(|record| record.path.clone())
        .collect::<BTreeSet<_>>()
        != expected
        || payload.len() != expected.len()
    {
        return Err("previous application has incomplete payload provenance".into());
    }
    // Code signing changes executable bytes; strict signature verification owns those identities.
    for record in payload
        .iter()
        .filter(|record| !record.path.starts_with("Contents/MacOS/"))
    {
        verify_artifacts(destination, std::slice::from_ref(record))?;
    }
    let signature = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(destination)
        .output()?;
    if !signature.status.success() {
        return Err(format!(
            "previous signature verification failed: {}",
            String::from_utf8_lossy(&signature.stderr)
        )
        .into());
    }
    Ok(())
}

/// Atomically exchange complete directories on macOS, never exposing a partial app.
pub(crate) fn publish(root: &Path, staged: &Path) -> Result<()> {
    safe_path(
        root,
        staged
            .strip_prefix(root)?
            .to_str()
            .ok_or("non-UTF8 staging path")?,
    )?;
    let destination = safe_path(root, TARGET)?;
    if inventory(staged)? != bundle_paths() {
        return Err("staged application inventory is incomplete or unexpected".into());
    }
    if !destination.exists() {
        fs::rename(staged, destination)?;
        return Ok(());
    }
    verify_previous(root, &destination)?;
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        unsafe extern "C" {
            fn renamex_np(
                from: *const std::ffi::c_char,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> i32;
        }
        let from = CString::new(staged.as_os_str().as_bytes())?;
        let to = CString::new(destination.as_os_str().as_bytes())?;
        // Darwin RENAME_SWAP exchanges existing directories on the same filesystem.
        const RENAME_SWAP: u32 = 0x0000_0002;
        if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), RENAME_SWAP) } != 0 {
            return Err(format!(
                "atomic application swap failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    Err("atomic application publication requires macOS".into())
}

/// Assert real Mach-O architecture, deployment version, and system-only linkage.
fn verify_binary(build: &mut Build<'_>, name: &str, path: &Path) -> Result<()> {
    if path.metadata()?.permissions().mode() & 0o111 != 0o111 {
        return Err(format!(
            "bundle executable has invalid permissions: {}",
            path.display()
        )
        .into());
    }
    let mut architecture = Command::new("xcrun");
    architecture.args(["lipo", "-archs"]).arg(path);
    if build
        .command(&format!("{name}-architecture"), architecture)?
        .trim()
        != "arm64"
    {
        return Err(format!("{name}: expected exactly arm64").into());
    }
    let mut version = Command::new("xcrun");
    version.args(["vtool", "-show-build"]).arg(path);
    let version = build.command(&format!("{name}-deployment"), version)?;
    if !version.lines().any(|line| line.trim() == "platform MACOS")
        || !version.lines().any(|line| line.trim() == "minos 13.0")
    {
        return Err(format!("{name}: missing macOS 13.0 deployment contract: {version}").into());
    }
    let mut linkage = Command::new("xcrun");
    linkage.args(["otool", "-L"]).arg(path);
    let linkage = build.command(&format!("{name}-linkage"), linkage)?;
    for line in linkage.lines().skip(1) {
        let dependency = line
            .split_whitespace()
            .next()
            .ok_or("empty dependency line")?;
        if !(dependency.starts_with("/System/Library/")
            || dependency.starts_with("/usr/lib/")
            || (dependency.starts_with("@rpath/libswift") && dependency.ends_with(".dylib")))
        {
            return Err(format!("{name}: non-system runtime dependency: {dependency}").into());
        }
    }
    Ok(())
}

/// Discovery-only smoke never calls a control tool, launches the owner, or enables protection.
fn smoke(root: &Path, executable: &Path, attempt: &str, version: &BuildVersion) -> Result<()> {
    let stderr = safe_path(root, &format!("{EVIDENCE}/{attempt}-mcp-smoke.stderr"))?;
    let stdout = safe_path(root, &format!("{EVIDENCE}/{attempt}-mcp-smoke.stdout"))?;
    let mut child = Command::new(executable)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(File::create(stderr)?))
        .spawn()?;
    let input = child.stdin.take().ok_or("MCP stdin missing")?;
    let output = child.stdout.take().ok_or("MCP stdout missing")?;
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| -> Result<()> {
        let mut input = input;
        let mut log = File::create(stdout)?;
        let mut receive = || -> Result<Value> {
            let line = receiver.recv_timeout(Duration::from_secs(5))??;
            writeln!(log, "{line}")?;
            Ok(serde_json::from_str(&line)?)
        };
        writeln!(
            input,
            "{}",
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "screenwake-bundle-check", "version": "0.1.0"}}
            })
        )?;
        input.flush()?;
        let initialized = receive()?;
        if initialized["id"] != 1
            || initialized["result"]["protocolVersion"] != "2025-11-25"
            || !initialized["result"]["capabilities"]["tools"].is_object()
            || initialized["result"]["serverInfo"]["version"] != version.display()
        {
            return Err(format!("packaged MCP initialize failed: {initialized}").into());
        }
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )?;
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})
        )?;
        input.flush()?;
        let response = receive()?;
        let tools = response["result"]["tools"]
            .as_array()
            .ok_or("MCP tool list missing")?;
        let names: BTreeSet<_> = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        if response["id"] != 2
            || tools.len() != 3
            || names != BTreeSet::from(["is_on", "turn_on", "turn_off"])
        {
            return Err(format!("packaged MCP tool inventory mismatch: {response}").into());
        }
        drop(input);
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err(format!("packaged MCP EOF exited {status}").into());
                }
                return Ok(());
            }
            if started.elapsed() >= Duration::from_secs(5) {
                return Err("packaged MCP did not exit within five seconds of EOF".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    if child.try_wait()?.is_none() {
        child.kill()?;
        child.wait()?;
    }
    reader.join().map_err(|_| "MCP output reader failed")?;
    result
}

/// Build the actual producer sources, seal and inspect the app, then publish once.
pub(crate) fn run(root: &Path, sha: &str, attempt: &str) -> Result<usize> {
    let mut build = Build::new(root, sha, attempt);
    let sources = build.stage(
        "preflight",
        "verify production inputs and clean source identity",
        || {
            require_inputs(root)?;
            if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                return Err("canonical release build requires an arm64 macOS host".into());
            }
            let license = Command::new("git")
                .current_dir(root)
                .args(["hash-object", "LICENSE.txt"])
                .output()?;
            if !license.status.success()
                || String::from_utf8(license.stdout)?.trim()
                    != "90b2643a579327564ed214dee8ec849fbc1732f1"
            {
                return Err("pinned LICENSE.txt changed".into());
            }
            source_identity(root, sha)
        },
    )?;
    let output_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(safe_path(root, &format!("{EVIDENCE}/build.lock"))?)?;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    use std::os::fd::AsRawFd;
    if unsafe { flock(output_lock.as_raw_fd(), 2 | 4) } != 0 {
        return Err("another native build owns this worktree's producer outputs".into());
    }
    let local_date = version::local_date(SystemTime::now())?;
    let build_version = version::allocate(root, &local_date)?;
    let mut versions = serde_json::Map::new();
    for (name, program, args) in [
        ("rustc", "rustc", vec!["--version"]),
        ("cargo", "cargo", vec!["--version"]),
        ("swift", "xcrun", vec!["swiftc", "--version"]),
        ("sdk", "xcrun", vec!["--show-sdk-version"]),
    ] {
        let mut command = Command::new(program);
        command.args(args);
        versions.insert(
            name.into(),
            build
                .command(&format!("{name}-version"), command)?
                .trim()
                .into(),
        );
    }
    let output = build.command(
        "rust-native",
        Producer::Ffi.command_for(root, &build_version, sha)?,
    )?;
    let library = build.stage(
        "snapshot-native",
        "validate and capture actual FFI compiler bytes",
        || Producer::Ffi.snapshot_for(root, sha, &output, &build_version),
    )?;
    let output = build.command(
        "bindgen-tool",
        Producer::Bindgen.command_for(root, &build_version, sha)?,
    )?;
    let generator = build.stage(
        "snapshot-bindgen",
        "validate and capture actual bindgen compiler bytes",
        || Producer::Bindgen.snapshot_for(root, sha, &output, &build_version),
    )?;
    verify_artifacts(root, &library.snapshots[1..])?;
    verify_artifacts(root, &generator.snapshots)?;
    let mut generator_identity = Command::new(safe_path(root, GENERATOR)?);
    generator_identity.arg("--build-info");
    version::verify_report(
        &build.command("bindgen-build-identity", generator_identity)?,
        &build_version,
        sha,
    )?;
    prepare_bindings(root)?;
    let mut generator_command = Command::new(safe_path(root, GENERATOR)?);
    generator_command
        .arg("generate")
        .arg(safe_path(root, &format!("{LIBRARY}.dylib"))?)
        .args(["--language", "swift", "--out-dir"])
        .arg(safe_path(root, GENERATED)?)
        .arg("--no-format");
    build.command("swift-bindings", generator_command)?;
    let mut inputs = library.snapshots.clone();
    for name in BINDINGS {
        inputs.push(artifact(root, &format!("{GENERATED}/{name}"))?);
    }
    build.stage(
        "assets",
        "pure-Rust native asset export and strict verification",
        || {
            assets::export(root, sha)?;
            assets::verify(root, sha)?;
            Ok(())
        },
    )?;
    for name in RESOURCES {
        inputs.push(artifact(root, &format!("{}/{name}", assets::TARGET))?);
    }
    let staging = tempfile::Builder::new()
        .prefix(".bundle-stage-")
        .tempdir_in(safe_path(root, "target/ai2sw")?)?;
    let bundle = staging.path().join("ai2sw.app");
    fs::create_dir_all(bundle.join("Contents/MacOS"))?;
    fs::create_dir_all(bundle.join("Contents/Resources"))?;
    let executable = bundle.join("Contents/MacOS/ai2sw");
    let contracts = safe_path(root, "target/ai2sw/contracts/ScreenWakeContracts")?;
    fs::create_dir_all(contracts.parent().ok_or("missing contract target parent")?)?;
    let swift_stages = |build: &mut Build<'_>| -> Result<()> {
        verify_artifacts(root, &library.snapshots[..1])?;
        build_version.verify_bytes(&fs::read(safe_path(root, &format!("{LIBRARY}.a"))?)?, sha)?;
        build.command("swift-native", swift(root, &executable, false)?)?;
        let mut native_identity = Command::new(&executable);
        native_identity.arg("--build-info");
        version::verify_report(
            &build.command("swift-build-identity", native_identity)?,
            &build_version,
            sha,
        )?;
        build.command("swift-contract-prepare", swift(root, &contracts, true)?)?;
        let mut contract_test = Command::new(&contracts);
        contract_test.arg(safe_path(root, assets::TARGET)?);
        build.command("swift-contract-test", contract_test)?;
        Ok(())
    };
    let mut mcp_command = Producer::Mcp.command_for(root, &build_version, sha)?;
    let capacity = thread::available_parallelism()?.get();
    let output = if capacity >= 3 {
        mcp_command.arg("--jobs").arg((capacity - 1).to_string());
        build.alongside("rust-mcp", mcp_command, swift_stages)?.0
    } else {
        let output = build.command("rust-mcp", mcp_command)?;
        swift_stages(&mut build)?;
        output
    };
    let mcp = build.stage(
        "snapshot-mcp",
        "validate and capture actual MCP compiler bytes",
        || Producer::Mcp.snapshot_for(root, sha, &output, &build_version),
    )?;
    inputs.extend(mcp.snapshots.clone());
    inputs.push(artifact(
        root,
        "target/ai2sw/contracts/ScreenWakeContracts",
    )?);
    build.stage(
        "assemble",
        "copy canonical Info.plist, MCP, license and thirteen native resources",
        || {
            verify_artifacts(root, &inputs)?;
            assets::verify(root, sha)?;
            fs::write(
                bundle.join("Contents/Info.plist"),
                build_version.plist(&fs::read_to_string(safe_path(
                    root,
                    &format!("{MACOS}/Info.plist"),
                )?)?)?,
            )?;
            fs::copy(
                safe_path(root, MCP)?,
                bundle.join("Contents/MacOS/ai2screenwake-mcp"),
            )?;
            fs::copy(
                root.join("LICENSE.txt"),
                bundle.join("Contents/Resources/LICENSE.txt"),
            )?;
            for name in RESOURCES {
                fs::copy(
                    safe_path(root, &format!("{}/{name}", assets::TARGET))?,
                    bundle.join(format!("Contents/Resources/{name}")),
                )?;
            }
            for name in ["ai2sw", "ai2screenwake-mcp"] {
                fs::set_permissions(
                    bundle.join(format!("Contents/MacOS/{name}")),
                    fs::Permissions::from_mode(0o755),
                )?;
            }
            Ok(())
        },
    )?;
    let mut plist = Command::new("plutil");
    plist
        .args(["-lint"])
        .arg(bundle.join("Contents/Info.plist"));
    build.command("plist-lint", plist)?;
    let mut plist = Command::new("plutil");
    plist
        .args(["-convert", "json", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"));
    let plist: Value = serde_json::from_str(&build.command("plist-metadata", plist)?)?;
    let revision = build_version.revision.to_string();
    if plist["CFBundleIdentifier"] != "com.appliedmedia.ScreenWake"
        || plist["CFBundleExecutable"] != "ai2sw"
        || plist["CFBundleName"] != "AI2ScreenWake"
        || plist["CFBundleDisplayName"] != "AI2ScreenWake"
        || plist["LSUIElement"] != true
        || plist["LSMinimumSystemVersion"] != "13.0"
        || plist["CFBundleShortVersionString"] != build_version.date
        || plist["CFBundleVersion"].as_str() != Some(revision.as_str())
    {
        return Err(
            format!("actual Info.plist violates the native bundle contract: {plist}").into(),
        );
    }
    verify_binary(&mut build, "app", &executable)?;
    verify_binary(
        &mut build,
        "mcp",
        &bundle.join("Contents/MacOS/ai2screenwake-mcp"),
    )?;
    let payload: Vec<_> = inventory(&bundle)?
        .iter()
        .map(|path| artifact(&bundle, path))
        .collect::<Result<_>>()?;
    let provenance = json!({
        "schema_version": 1, "git_sha": sha, "source_root": root, "target": TARGET,
        "configuration": "release", "architecture": "arm64", "minimum_macos": "13.0",
        "signing": "local ad-hoc only; not notarized or distributed",
        "sources": sources, "build_inputs": inputs, "unsigned_payload": payload,
        "toolchain": versions, "commands": build.commands,
        "compiler_producers": [&library, &generator, &mcp],
        "build_version": build_version, "build_stamp": build_version.stamp(sha),
    });
    fs::write(
        bundle.join("Contents/Resources/build-provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    for (name, path) in [
        ("sign-mcp", bundle.join("Contents/MacOS/ai2screenwake-mcp")),
        ("sign-app", bundle.clone()),
    ] {
        let mut sign = Command::new("codesign");
        sign.args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(path);
        build.command(name, sign)?;
    }
    let mut signature = Command::new("codesign");
    signature
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(&bundle);
    build.command("signature-check", signature)?;
    build.stage(
        "mcp-smoke",
        "packaged MCP initialize + tools/list + EOF (no controls)",
        || {
            smoke(
                root,
                &bundle.join("Contents/MacOS/ai2screenwake-mcp"),
                attempt,
                &build_version,
            )
        },
    )?;
    let artifacts: Vec<_> = inventory(&bundle)?
        .iter()
        .map(|path| artifact(&bundle, path))
        .collect::<Result<_>>()?;
    let destination = safe_path(root, TARGET)?;
    if destination.exists() {
        let previous: Value = serde_json::from_slice(&fs::read(
            destination.join("Contents/Resources/build-provenance.json"),
        )?)?;
        if previous["schema_version"] != 1
            || previous["source_root"] != json!(root)
            || previous["target"] != TARGET
        {
            return Err(
                "refusing to replace an application without this worktree's provenance".into(),
            );
        }
        let mut signature = Command::new("codesign");
        signature
            .args(["--verify", "--deep", "--strict"])
            .arg(&destination);
        build.command("previous-signature-check", signature)?;
    }
    let receipt = json!({
        "schema_version": 1, "attempt": attempt, "git_sha": sha, "source_root": root,
        "target": TARGET, "configuration": "release", "artifacts": artifacts,
        "provenance_sha256": checksum(&bundle.join("Contents/Resources/build-provenance.json"))?,
        "commands": build.commands, "compiler_producers": [&library, &generator, &mcp],
        "mcp_tools": ["is_on", "turn_on", "turn_off"],
        "app_launched": false, "release_posted": false,
        "build_version": build_version, "build_stamp": build_version.stamp(sha),
    });
    let receipt_path = safe_path(root, &format!("{EVIDENCE}/{attempt}-bundle.json"))?;
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    build.stage(
        "publish",
        "verify current source/inputs and atomically publish complete signed app",
        || {
            if source_identity(root, sha)? != sources {
                return Err("source changed during native build".into());
            }
            verify_artifacts(root, &inputs)?;
            verify_artifacts(&bundle, &artifacts)?;
            if version::allocate(root, &local_date)? != build_version {
                return Err("published predecessor changed during versioned build".into());
            }
            publish(root, &bundle)
        },
    )?;
    verify_artifacts(&destination, &artifacts)?;
    println!(
        "BUILD_RECEIPT path={} sha={sha} target={TARGET}",
        receipt_path.display()
    );
    Ok(artifacts.len())
}
// end build.rs
