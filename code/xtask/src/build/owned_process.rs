// owned_process.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! A private POSIX session pins descendant ownership until verified teardown.
//! The supervisor also tears down on caller-descriptor EOF, without ancestry polling.

use crate::assets::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            net::UnixStream,
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const ENTRY: &str = "--screenwake-owned-process";
const SCOPE: &str = "SCREENWAKE_BUILD_SESSION";
const SCOPE_FD: &str = "SCREENWAKE_BUILD_SCOPE_FD";
const LIFETIMES: &str = "SCREENWAKE_BUILD_LIFETIME_FDS";
const CLEANUP: Duration = Duration::from_secs(2);

unsafe extern "C" {
    fn setsid() -> i32;
    fn getsid(pid: i32) -> i32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn getsockopt(fd: i32, level: i32, option: i32, value: *mut i32, length: *mut u32) -> i32;
}

#[derive(Serialize, Deserialize)]
struct Spec {
    program: Vec<u8>,
    arguments: Vec<Vec<u8>>,
    environment: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    directory: Vec<u8>,
}

/// A session leader is never reaped before its verified cleanup receipt.
pub(crate) struct OwnedChild {
    child: Child,
    control: Option<UnixStream>,
    receipt: Vec<u8>,
    status: Option<ExitStatus>,
    finished: bool,
    session: i32,
    lifetime: UnixStream,
}

impl OwnedChild {
    pub(crate) fn spawn(command: Command, stdout: &Path, stderr: &Path) -> Result<Self> {
        if let Some(scope) = std::env::var_os(SCOPE) {
            let scope: i32 = scope
                .to_str()
                .ok_or("invalid inherited build scope")?
                .parse()?;
            if unsafe { getsid(0) } != scope || scope == std::process::id() as i32 {
                return Err(
                    "inherited build scope does not match the private supervisor session".into(),
                );
            }
            let fd: i32 = std::env::var(SCOPE_FD)?.parse()?;
            let mut peer = 0;
            let mut size = std::mem::size_of::<i32>() as u32;
            // macOS public SOL_LOCAL / LOCAL_PEERPID verifies the inherited capability.
            if unsafe { getsockopt(fd, 0, 2, &mut peer, &mut size) } != 0 || peer != scope {
                return Err("inherited build scope has no live supervisor descriptor".into());
            }
        } else if std::env::var_os(LIFETIMES).is_some() {
            return Err("unowned inherited build lifetime descriptors".into());
        }
        let spec = Spec {
            program: command.get_program().as_bytes().to_vec(),
            arguments: command
                .get_args()
                .map(|arg| arg.as_bytes().to_vec())
                .collect(),
            environment: command
                .get_envs()
                .map(|(key, value)| {
                    (
                        key.as_bytes().to_vec(),
                        value.map(|value| value.as_bytes().to_vec()),
                    )
                })
                .collect(),
            directory: command
                .get_current_dir()
                .ok_or("owned command requires an explicit directory")?
                .as_os_str()
                .as_bytes()
                .to_vec(),
        };
        let (control, endpoint) = UnixStream::pair()?;
        let (lifetime, writer) = UnixStream::pair()?;
        lifetime.set_nonblocking(true)?;
        let writer_fd = writer.as_raw_fd();
        let fd = endpoint.as_raw_fd();
        let mut supervisor = Command::new(std::env::current_exe()?);
        supervisor
            .args([ENTRY, &fd.to_string(), &writer_fd.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::from(File::create(stdout)?))
            .stderr(Stdio::from(File::create(stderr)?));
        // Only async-signal-safe syscalls run after fork and before exec.
        unsafe {
            supervisor.pre_exec(move || {
                if setsid() == -1 || fcntl(fd, 2, 0) == -1 || fcntl(writer_fd, 2, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = supervisor.spawn()?;
        let session = i32::try_from(child.id())?;
        drop(endpoint);
        drop(writer);
        let mut owned = Self {
            child,
            control: Some(control),
            receipt: Vec::new(),
            status: None,
            finished: false,
            session,
            lifetime,
        };
        let control = owned.control.as_mut().expect("new control descriptor");
        serde_json::to_writer(&mut *control, &spec)?;
        control.write_all(b"\n")?;
        control.set_nonblocking(true)?;
        Ok(owned)
    }

    fn lifetime_complete(&self) -> Result<bool> {
        let mut reader = &self.lifetime;
        match reader.read(&mut [0]) {
            Ok(0) => Ok(true),
            Ok(_) => Err("unexpected data on owned process lifetime descriptor".into()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(self.lifetime_complete()?.then_some(status));
        }
        let control = self
            .control
            .as_mut()
            .ok_or("owned control was already closed")?;
        let mut bytes = [0; 1024];
        match control.read(&mut bytes) {
            Ok(0) => {
                return Err("owned supervisor exited without a verified teardown receipt".into());
            }
            Ok(count) => self.receipt.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        if self.receipt.len() > 8192 {
            return Err("oversized owned teardown receipt".into());
        }
        if self.receipt.last() != Some(&b'\n') {
            return Ok(None);
        }
        let receipt = std::str::from_utf8(&self.receipt)?.trim();
        let status: i32 = receipt
            .strip_prefix("OK ")
            .ok_or_else(|| format!("owned cleanup failed: {receipt}"))?
            .parse()?;
        let status = ExitStatus::from_raw(status);
        self.status = Some(status);
        Ok(self.lifetime_complete()?.then_some(status))
    }

    pub(crate) fn finish(&mut self, _failed: bool) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let session = self.session;
        if let Some(control) = self.control.take() {
            if let Err(error) = control.shutdown(std::net::Shutdown::Both) {
                if error.kind() != std::io::ErrorKind::NotConnected {
                    return Err(error.into());
                }
            }
        }
        let started = Instant::now();
        let mut forced = false;
        loop {
            let clean = self.lifetime_complete()?;
            if let Some(status) = if clean { self.child.try_wait()? } else { None } {
                self.finished = true;
                if !status.success() {
                    return Err("owned supervisor failed to acknowledge cleanup".into());
                }
                return Ok(());
            }
            if !forced && started.elapsed() >= CLEANUP / 2 {
                // The unreaped leader pins this exact private session even on supervisor failure.
                terminate_session(session, None)?;
                forced = true;
            }
            if started.elapsed() >= CLEANUP {
                self.finished = true;
                return Err("owned descendant lifetime did not close within two seconds".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.finish(true) {
                eprintln!("owned command teardown failed: {error}");
            }
        }
    }
}

fn members(session: i32, except: Option<i32>) -> Result<Vec<i32>> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,stat="])
        .output()?;
    if !output.status.success() {
        return Err("private session inspection failed".into());
    }
    let mut members = Vec::new();
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        let mut fields = line.split_whitespace();
        let pid: i32 = fields.next().ok_or("missing process identity")?.parse()?;
        let state = fields.next().ok_or("missing process state")?;
        // Zombies have exited and cannot fork or write; their real parent performs reaping.
        if except != Some(pid) && !state.starts_with('Z') && unsafe { getsid(pid) } == session {
            members.push(pid);
        }
    }
    Ok(members)
}

fn signal_member(session: i32, pid: i32, signal: i32) -> Result<()> {
    if unsafe { getsid(pid) } != session {
        return Ok(());
    }
    if unsafe { kill(pid, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(3) {
        return Ok(());
    }
    Err(error.into())
}

fn terminate_session(session: i32, except: Option<i32>) -> Result<()> {
    if session <= 0
        || (unsafe { getsid(0) } == session && except != Some(std::process::id() as i32))
    {
        return Err("refusing to signal the caller/shared process session".into());
    }
    let deadline = Instant::now() + CLEANUP;
    #[cfg(target_os = "macos")]
    const STOP: i32 = 17;
    #[cfg(not(target_os = "macos"))]
    const STOP: i32 = 19;
    let mut stopped = BTreeSet::new();
    loop {
        let added: Vec<_> = members(session, except)?
            .into_iter()
            .filter(|pid| !stopped.contains(pid))
            .collect();
        if added.is_empty() {
            break;
        }
        for pid in added {
            signal_member(session, pid, STOP)?;
            stopped.insert(pid);
        }
        if Instant::now() >= deadline {
            return Err("private session quiescence exceeded two seconds".into());
        }
    }
    for pid in stopped.into_iter().rev() {
        signal_member(session, pid, 9)?;
    }
    while !members(session, except)?.is_empty() {
        if Instant::now() >= deadline {
            return Err("private session still has live output writers".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn supervise(control: &mut UnixStream, writer: i32) -> Result<i32> {
    let session = i32::try_from(std::process::id())?;
    if unsafe { getsid(0) } != session {
        return Err("supervisor requires its own newly created session".into());
    }
    if unsafe { fcntl(control.as_raw_fd(), 2, 1) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut line = String::new();
    BufReader::new(&mut *control)
        .take(1024 * 1024)
        .read_line(&mut line)?;
    let spec: Spec = serde_json::from_str(&line)?;
    let mut command = Command::new(OsString::from_vec(spec.program));
    let mut lifetimes = match std::env::var(LIFETIMES) {
        Ok(value) if !value.is_empty() => value
            .split(',')
            .map(str::parse::<i32>)
            .collect::<std::result::Result<Vec<_>, _>>()?,
        Ok(_) | Err(std::env::VarError::NotPresent) => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    lifetimes.push(writer);
    if lifetimes.iter().any(|fd| *fd < 3)
        || lifetimes.iter().collect::<BTreeSet<_>>().len() != lifetimes.len()
    {
        return Err("invalid owned lifetime descriptor chain".into());
    }
    let (_scope_owner, scope_witness) = UnixStream::pair()?;
    let witness = scope_witness.as_raw_fd();
    if unsafe { fcntl(witness, 2, 0) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    command
        .args(spec.arguments.into_iter().map(OsString::from_vec))
        .current_dir(OsString::from_vec(spec.directory))
        .process_group(0);
    for (key, value) in spec.environment {
        let key = OsString::from_vec(key);
        match value {
            Some(value) => {
                command.env(key, OsString::from_vec(value));
            }
            None => {
                command.env_remove(key);
            }
        }
    }
    command
        .env(SCOPE, session.to_string())
        .env(SCOPE_FD, witness.to_string());
    command.env(
        LIFETIMES,
        lifetimes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
    );
    let mut child = command.spawn()?;
    control.set_nonblocking(true)?;
    let result = (|| -> Result<Option<ExitStatus>> {
        loop {
            let mut byte = [0];
            match control.read(&mut byte) {
                Ok(0) => return Ok(None),
                Ok(_) => return Err("unexpected caller control message".into()),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
            if let Some(status) = child.try_wait()? {
                return Ok(Some(status));
            }
            thread::sleep(Duration::from_millis(5));
        }
    })();
    let cleanup = terminate_session(session, Some(session));
    child.wait()?;
    cleanup?;
    // EOF on each ancestor's own pipe proves every nested session has also finished.
    for fd in lifetimes {
        drop(unsafe { UnixStream::from_raw_fd(fd) });
    }
    if let Some(status) = result? {
        if let Err(error) = writeln!(control, "OK {}", status.into_raw()) {
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) {
                return Ok(0);
            }
            return Err(error.into());
        }
        loop {
            let mut byte = [0];
            match control.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => return Err("unexpected cleanup acknowledgement".into()),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(0)
}

/// The same binary hosts its bounded supervisor before ordinary CLI/test parsing.
pub(crate) fn supervisor_entry() -> bool {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_none_or(|argument| argument != ENTRY) {
        return false;
    }
    let result = (|| -> Result<i32> {
        if arguments.len() != 3 {
            return Err("invalid owned supervisor invocation".into());
        }
        let fd = arguments[1]
            .to_str()
            .ok_or("invalid owned control descriptor")?
            .parse::<i32>()?;
        if fd < 3 {
            return Err("invalid owned control descriptor".into());
        }
        let mut control = unsafe { UnixStream::from_raw_fd(fd) };
        let writer = arguments[2]
            .to_str()
            .ok_or("invalid lifetime descriptor")?
            .parse::<i32>()?;
        if writer < 3 || writer == fd {
            return Err("invalid lifetime descriptor".into());
        }
        supervise(&mut control, writer)
    })();
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("owned supervisor failed: {error}");
            let session = std::process::id() as i32;
            if unsafe { getsid(0) } == session {
                if let Err(cleanup) = terminate_session(session, Some(session)) {
                    eprintln!("owned supervisor error teardown failed: {cleanup}");
                }
            }
            std::process::exit(1);
        }
    }
}
// end owned_process.rs
