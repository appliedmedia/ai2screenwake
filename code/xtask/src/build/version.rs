// version.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! A build reserves no counter: only an atomically published owned app advances it.

use super::{Result, TARGET, verify_previous};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::SystemTime};

/// One host-local date and bounded native build revision, frozen before compilation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildVersion {
    pub(crate) date: String,
    pub(crate) revision: u32,
}

impl BuildVersion {
    /// Validate calendar input and advance only a successfully published same-day revision.
    pub(crate) fn next(date: &str, previous: Option<&Self>) -> Result<Self> {
        validate_date(date)?;
        let revision = if let Some(previous) = previous {
            previous.validate()?;
            if previous.date.as_str() > date {
                return Err(
                    "local build date precedes the published version; refusing reuse".into(),
                );
            }
            if previous.date == date {
                previous
                    .revision
                    .checked_add(1)
                    .ok_or("build revision overflow")?
            } else {
                1
            }
        } else {
            1
        };
        let version = Self {
            date: date.into(),
            revision,
        };
        version.validate()?;
        Ok(version)
    }

    fn validate(&self) -> Result<()> {
        validate_date(&self.date)?;
        if !(1..=9999).contains(&self.revision) {
            return Err("native build revision must be between 1 and 9999".into());
        }
        Ok(())
    }

    /// Exact product copy shared by the compiled runtime, MCP, native footer and receipts.
    pub(crate) fn display(&self) -> String {
        format!("v{}({})", self.date, self.revision)
    }

    /// Changing even only the revision changes every Cargo producer's tracked compiler input.
    pub(crate) fn stamp(&self, sha: &str) -> String {
        format!(
            "AI2SW_BUILD:{}:{sha}:release:aarch64-apple-darwin:macos13.0",
            self.display()
        )
    }

    /// Require the expected compiled stamp before capturing or consuming producer bytes.
    pub(crate) fn verify_bytes(&self, bytes: &[u8], sha: &str) -> Result<()> {
        let stamp = self.stamp(sha);
        if !bytes
            .windows(stamp.len())
            .any(|window| window == stamp.as_bytes())
        {
            return Err("compiler producer build-version stamp mismatch".into());
        }
        Ok(())
    }

    /// Generate native metadata without modifying the committed producer template.
    pub(crate) fn plist(&self, template: &str) -> Result<String> {
        self.validate()?;
        if template.matches("@AI2SW_BUILD_DATE@").count() != 1
            || template.matches("@AI2SW_BUILD_REVISION@").count() != 1
        {
            return Err(
                "native plist must contain exactly one frozen date and revision input".into(),
            );
        }
        Ok(template
            .replace("@AI2SW_BUILD_DATE@", &self.date)
            .replace("@AI2SW_BUILD_REVISION@", &self.revision.to_string()))
    }
}

fn validate_date(date: &str) -> Result<()> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'.'
        || bytes[7] != b'.'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return Err("build date must be YYYY.MM.DD".into());
    }
    let year: u32 = date[..4].parse()?;
    let month: usize = date[5..7].parse()?;
    let day: u32 = date[8..].parse()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > days[month - 1]
    {
        return Err("build date is not a valid local calendar date".into());
    }
    Ok(())
}

/// Capture the host's local calendar exactly once, not the UTC day or a shell date.
pub(crate) fn local_date(instant: SystemTime) -> Result<String> {
    let seconds: libc::time_t = instant
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs()
        .try_into()?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r initializes one tm on success; neither pointer escapes.
    if unsafe { libc::localtime_r(&seconds, local.as_mut_ptr()) }.is_null() {
        return Err("cannot resolve the host-local build date".into());
    }
    let local = unsafe { local.assume_init() };
    let year = local
        .tm_year
        .checked_add(1900)
        .ok_or("local year overflow")?;
    let date = format!("{year:04}.{:02}.{:02}", local.tm_mon + 1, local.tm_mday);
    validate_date(&date)?;
    Ok(date)
}

fn plist_metadata(app: &Path) -> Result<Value> {
    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "previous version plist failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The caller holds the same-root exclusive build lock through atomic publication.
pub(crate) fn allocate(root: &Path, date: &str) -> Result<BuildVersion> {
    let app = super::safe_path(root, TARGET)?;
    if !app.exists() {
        return BuildVersion::next(date, None);
    }
    verify_previous(root, &app)?;
    let provenance: Value = serde_json::from_slice(&fs::read(
        app.join("Contents/Resources/build-provenance.json"),
    )?)?;
    let plist = plist_metadata(&app)?;
    let previous = if let Some(value) = provenance.get("build_version") {
        let version: BuildVersion = serde_json::from_value(value.clone())?;
        version.validate()?;
        let revision = version.revision.to_string();
        if plist["CFBundleShortVersionString"] != version.date
            || plist["CFBundleVersion"].as_str() != Some(revision.as_str())
            || provenance["build_stamp"]
                != version.stamp(
                    provenance["git_sha"]
                        .as_str()
                        .ok_or("missing previous source SHA")?,
                )
        {
            return Err("previous signed bundle/version provenance does not agree".into());
        }
        Some(version)
    } else {
        if plist["CFBundleShortVersionString"] != "0.1.0" || plist["CFBundleVersion"] != "1" {
            return Err("previous bundle has no recognized legacy version provenance".into());
        }
        None
    };
    BuildVersion::next(date, previous.as_ref())
}

/// An actual producer executable must report the same immutable identity before consumption.
pub(crate) fn verify_report(output: &str, version: &BuildVersion, sha: &str) -> Result<()> {
    let report: Value = serde_json::from_str(output)?;
    if report != json!({"version": version.display(), "stamp": version.stamp(sha)}) {
        return Err("native executable build-version identity mismatch".into());
    }
    Ok(())
}
// end version.rs
