// main.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Measured, worktree-local foundation checks and native artwork export.

mod assets;

use assets::{Result, checksum, head};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const EVIDENCE: &str = "target/foundation-evidence";
const SCENARIOS: &str = "code/crates/ai2screenwake_client/tests/features/foundation.feature";
const WORKERS: u8 = 3;
const SANITY_BUDGET: Duration = Duration::from_secs(10);

/// Artifact identity captured only after a successful current-source preparation.
#[derive(Deserialize, Serialize)]
struct Prepared {
    executable: PathBuf,
    git_sha: String,
    source_sha256: String,
    executable_sha256: String,
}

/// Collect only worktree-local source inputs; reject symlink escapes.
fn source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!(
                "source symlink is not admissible: {}",
                entry.path().display()
            )
            .into());
        }
        if kind.is_dir() {
            source_files(&entry.path(), files)?;
        } else if matches!(
            entry.path().extension().and_then(|v| v.to_str()),
            Some("rs" | "toml" | "feature")
        ) {
            files.push(entry.path());
        }
    }
    Ok(())
}

/// Tie a runner to its manifests, lockfile, bindings and executable scenarios.
fn source_fingerprint(root: &Path) -> Result<String> {
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join(".cargo/config.toml"),
    ];
    source_files(&root.join("code"), &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for file in files {
        let path = file.strip_prefix(root)?.to_string_lossy();
        let bytes = fs::read(&file)?;
        hash.update(u64::try_from(path.len())?.to_be_bytes());
        hash.update(path.as_bytes());
        hash.update(u64::try_from(bytes.len())?.to_be_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Prepare the exact Cucumber executable and persist its provenance.
fn prepare(root: &Path, sha: &str, attempt: &str) -> Result<usize> {
    let cargo = std::env::var_os("CARGO").ok_or("CARGO executable is unavailable")?;
    let result = Command::new(cargo)
        .current_dir(root)
        .args([
            "test",
            "--locked",
            "-p",
            "ai2screenwake_client",
            "--test",
            "foundation",
            "--no-run",
            "--message-format=json",
        ])
        .output()?;
    fs::write(
        root.join(EVIDENCE)
            .join(format!("{attempt}-preparation.stdout")),
        &result.stdout,
    )?;
    fs::write(
        root.join(EVIDENCE)
            .join(format!("{attempt}-preparation.stderr")),
        &result.stderr,
    )?;
    eprint!("{}", String::from_utf8_lossy(&result.stderr));
    if !result.status.success() {
        return Err(format!("foundation compilation exited {}", result.status).into());
    }
    let mut executable = None;
    for line in result
        .stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
    {
        let message: Value = serde_json::from_slice(line)?;
        if message["reason"] == "compiler-artifact" && message["target"]["name"] == "foundation" {
            if let Some(path) = message["executable"].as_str() {
                if executable.replace(PathBuf::from(path)).is_some() {
                    return Err("multiple foundation executables are not admissible".into());
                }
            }
        }
    }
    let executable = executable
        .ok_or("successful compilation did not identify a foundation runner")?
        .canonicalize()?;
    if !executable.starts_with(root.join("target")) {
        return Err("foundation executable is outside this worktree's target directory".into());
    }
    let prepared = Prepared {
        executable_sha256: checksum(&executable)?,
        executable,
        git_sha: sha.to_owned(),
        source_sha256: source_fingerprint(root)?,
    };
    fs::write(
        root.join(EVIDENCE).join("prepared.json"),
        serde_json::to_vec_pretty(&prepared)?,
    )?;
    println!(
        "FOUNDATION_RUNNER sha={sha} path={}",
        prepared.executable.display()
    );
    Ok(0)
}

/// Run only the prepared foundation scenarios, failing on stale artifacts or timeout.
fn sanity(root: &Path, sha: &str, attempt: &str) -> Result<usize> {
    let prepared: Prepared = serde_json::from_slice(
        &fs::read(root.join(EVIDENCE).join("prepared.json"))
            .map_err(|error| format!("run cargo xtask prepare first: {error}"))?,
    )?;
    if prepared.git_sha != sha
        || prepared.source_sha256 != source_fingerprint(root)?
        || !prepared
            .executable
            .canonicalize()?
            .starts_with(root.join("target"))
        || prepared.executable_sha256 != checksum(&prepared.executable)?
    {
        return Err("prepared artifact is stale; run cargo xtask prepare at current HEAD".into());
    }
    let expected = fs::read_to_string(root.join(SCENARIOS))?
        .lines()
        .filter(|line| line.trim_start().starts_with("Scenario:"))
        .count();
    if expected == 0 {
        return Err("foundation scenario selection is empty".into());
    }
    let log_path = root.join(EVIDENCE).join(format!("{attempt}-sanity.log"));
    let log = File::create(&log_path)?;
    let started = Instant::now();
    let mut child = Command::new(&prepared.executable)
        .current_dir(root.join("code/crates/ai2screenwake_client"))
        .args(["--concurrency", &WORKERS.to_string(), "--color", "never"])
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= SANITY_BUDGET {
            child.kill()?;
            child.wait()?;
            return Err(format!(
                "sanity exceeded 10 seconds; evidence: {}",
                log_path.display()
            )
            .into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let log = fs::read_to_string(&log_path)?;
    print!("{log}");
    if !status.success() {
        return Err(format!(
            "foundation scenarios exited {status}; see {}",
            log_path.display()
        )
        .into());
    }
    if !log.contains(&format!("{expected} scenarios ({expected} passed)")) {
        return Err(
            format!("expected {expected} passing scenarios; missing exact runner summary").into(),
        );
    }
    if started.elapsed() >= SANITY_BUDGET {
        return Err("sanity completed outside its 10-second budget".into());
    }
    Ok(expected)
}

/// Every attempt, including failures, records both its stage and total duration.
fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    if std::env::current_dir()?.canonicalize()? != root {
        return Err(format!("run cargo xtask from {}", root.display()).into());
    }
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let stage = match arguments.as_slice() {
        [stage] if matches!(stage.as_str(), "prepare" | "sanity" | "assets") => stage.as_str(),
        [stage, flag] if stage == "assets" && flag == "--check" => "assets-check",
        _ => return Err("usage: cargo xtask <prepare|sanity|assets [--check]>".into()),
    };
    let asset_stage = stage.starts_with("assets");
    let evidence_directory = if asset_stage {
        "target/screenwake/assets-evidence"
    } else {
        EVIDENCE
    };
    fs::create_dir_all(root.join(evidence_directory))?;
    let sha = head(&root)?;
    let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let attempt = format!("{started_at}-{}", std::process::id());
    let started = Instant::now();
    let mut result = match stage {
        "prepare" => prepare(&root, &sha, &attempt),
        "sanity" => sanity(&root, &sha, &attempt),
        "assets" => assets::export(&root, &sha).map(|manifest| manifest.outputs.len()),
        "assets-check" => assets::verify(&root, &sha).map(|manifest| manifest.outputs.len()),
        _ => unreachable!("validated above"),
    };
    let elapsed = started.elapsed().as_secs_f64();
    if stage == "sanity" && elapsed >= SANITY_BUDGET.as_secs_f64() {
        result =
            Err("complete sanity stage, including provenance/setup, exceeded 10 seconds".into());
    }
    let finished_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let mut evidence = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(evidence_directory).join("timing.jsonl"))?;
    for name in [stage, "total"] {
        let row = json!({
            "attempt": attempt, "stage": name, "started_at": started_at, "finished_at": finished_at,
            "elapsed_seconds": elapsed, "git_sha": sha,
            "environment": format!("{}-{}:debug:{}", std::env::consts::OS, std::env::consts::ARCH, if asset_stage { "assets" } else { "foundation" }),
            "worker_count": if stage == "sanity" { WORKERS } else { 1 },
            "scenario_count": if asset_stage { None } else { result.as_ref().ok() },
            "artifact_count": if asset_stage { result.as_ref().ok() } else { None },
            "exit_status": if result.is_ok() { 0 } else { 1 },
            "command": if stage == "assets-check" { "cargo xtask assets --check".to_owned() } else { format!("cargo xtask {stage}") },
            "error": result.as_ref().err().map(ToString::to_string),
        });
        writeln!(evidence, "{row}")?;
        println!("{row}");
    }
    evidence.flush()?;
    let count = result?;
    if asset_stage {
        println!(
            "ASSETS_{}_OK artifacts={count} elapsed_seconds={elapsed:.6} sha={sha} target={}",
            if stage == "assets" { "EXPORT" } else { "CHECK" },
            assets::TARGET
        );
    } else {
        println!(
            "FOUNDATION_{}_OK scenarios={count} elapsed_seconds={elapsed:.6} sha={sha}",
            stage.to_uppercase(),
        );
    }
    Ok(())
}

/// Exit nonzero on any failed gate, including evidence-writing failures.
fn main() {
    if let Err(error) = run() {
        eprintln!("xtask gate failed: {error}");
        std::process::exit(1);
    }
}
// end main.rs
