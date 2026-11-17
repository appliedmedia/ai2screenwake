// action_log.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Private checked write-ahead diagnostics, never resource/intent restoration.

use crate::contract::{
    ActionJournal, Fault, JournalEntry, JournalHistory, KvEnvelope, LOG_MEMORY_LIMIT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::Mutex,
};

const VERSION: u8 = 1;
pub(crate) const FILE_LIMIT: u64 = 1024 * 1024;
pub(crate) const FRAME_HEADER_BYTES: u64 = 36;
const RECORD_LIMIT: usize = 256 * 1024;
const ACTIVE: &str = "actions.journal";
const PREVIOUS: &str = "actions.previous";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u8,
    sequence: u64,
    envelope: KvEnvelope,
}

/// Encode the canonical stored payload; framers and boundary tests share this codec.
pub(crate) fn encode_entry(sequence: u64, envelope: &KvEnvelope) -> Result<Vec<u8>, Fault> {
    let bytes = serde_json::to_vec(&Stored {
        version: VERSION,
        sequence,
        envelope: envelope.clone(),
    })
    .map_err(|_| {
        Fault::new(
            "journal_encoding",
            "Cannot encode the owner journal entry.",
            false,
        )
    })?;
    if bytes.len() > RECORD_LIMIT {
        return Err(Fault::new(
            "journal_entry_large",
            "Owner journal entry exceeds its record limit.",
            false,
        ));
    }
    Ok(bytes)
}

fn io_fault(error: io::Error) -> Fault {
    Fault::new(
        "journal_io",
        &format!("Owner journal I/O failed ({:?}).", error.kind()),
        false,
    )
}

fn corrupt() -> Fault {
    Fault::new(
        "journal_corrupt",
        "The owner journal is corrupt; its bytes were preserved.",
        false,
    )
}

fn read_record(file: &mut File, bytes: &mut [u8]) -> Result<(), Fault> {
    file.read_exact(bytes).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            corrupt()
        } else {
            io_fault(error)
        }
    })
}

fn validate(file: &File, uid: u32) -> Result<u64, Fault> {
    let metadata = file.metadata().map_err(io_fault)?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
    {
        return Err(Fault::new(
            "journal_unsafe",
            "The owner journal is not a private owned regular file.",
            false,
        ));
    }
    if metadata.len() > FILE_LIMIT {
        return Err(Fault::new(
            "journal_too_large",
            "The owner journal exceeds its retained-file limit.",
            false,
        ));
    }
    Ok(metadata.len())
}

fn scan(
    path: &Path,
    uid: u32,
    recent: &mut VecDeque<JournalEntry>,
    next: &mut Option<u64>,
) -> Result<(), Fault> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_fault(error)),
    };
    validate(&file, uid)?;
    loop {
        let mut size = [0_u8; 4];
        match file.read(&mut size[..1]) {
            Ok(0) => break,
            Ok(1) => {}
            Ok(_) => unreachable!("one-byte read"),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_fault(error)),
        }
        read_record(&mut file, &mut size[1..])?;
        let size = u32::from_be_bytes(size) as usize;
        if size == 0 || size > RECORD_LIMIT {
            return Err(corrupt());
        }
        let mut checksum = [0_u8; 32];
        read_record(&mut file, &mut checksum)?;
        let mut bytes = vec![0_u8; size];
        read_record(&mut file, &mut bytes)?;
        if Sha256::digest(&bytes).as_slice() != checksum {
            return Err(corrupt());
        }
        let record: Stored = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
        if record.version != VERSION || next.is_some_and(|next| next != record.sequence) {
            return Err(corrupt());
        }
        record.envelope.validate_history().map_err(|_| corrupt())?;
        *next = Some(record.sequence.checked_add(1).ok_or_else(corrupt)?);
        recent.push_back(JournalEntry {
            sequence: record.sequence,
            envelope: record.envelope,
        });
        if recent.len() > LOG_MEMORY_LIMIT {
            recent.pop_front();
        }
    }
    Ok(())
}

fn active(path: &Path, uid: u32) -> Result<(File, u64), Fault> {
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(io_fault)?;
    let size = validate(&file, uid)?;
    Ok((file, size))
}

struct State {
    file: File,
    bytes: u64,
    next: u64,
    recent: VecDeque<JournalEntry>,
    failed: bool,
}

struct Readable {
    path: Result<PathBuf, Fault>,
    published: Option<(u64, u64)>,
}

/// One owner-locked journal plus one bounded archive, both private and checksummed.
pub(crate) struct FileJournal {
    active: PathBuf,
    previous: PathBuf,
    uid: u32,
    state: Mutex<State>,
    readable: Mutex<Readable>,
}

impl FileJournal {
    pub(crate) fn open_with_readable_log(
        directory: &Path,
        uid: u32,
        readable: Result<PathBuf, Fault>,
    ) -> Result<Self, Fault> {
        let active_path = directory.join(ACTIVE);
        let previous = directory.join(PREVIOUS);
        let mut recent = VecDeque::new();
        let mut next = None;
        scan(&previous, uid, &mut recent, &mut next)?;
        scan(&active_path, uid, &mut recent, &mut next)?;
        let (file, bytes) = active(&active_path, uid)?;
        Ok(Self {
            active: active_path,
            previous,
            uid,
            state: Mutex::new(State {
                file,
                bytes,
                next: next.unwrap_or(0),
                recent,
                failed: false,
            }),
            readable: Mutex::new(Readable {
                path: readable,
                published: None,
            }),
        })
    }

    fn write(&self, state: &mut State, sequence: u64, envelope: &KvEnvelope) -> Result<(), Fault> {
        if sequence != state.next {
            return Err(Fault::new(
                "journal_sequence",
                "Owner journal append order is inconsistent.",
                false,
            ));
        }
        let bytes = encode_entry(sequence, envelope)?;
        let size = u32::try_from(bytes.len()).map_err(|_| {
            Fault::new(
                "journal_entry_large",
                "Owner journal entry length is invalid.",
                false,
            )
        })?;
        let framed = u64::from(size) + FRAME_HEADER_BYTES;
        if state.bytes > 0 && state.bytes + framed > FILE_LIMIT {
            state.file.sync_all().map_err(io_fault)?;
            match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                .open(&self.previous)
            {
                Ok(previous) => {
                    validate(&previous, self.uid)?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_fault(error)),
            }
            fs::rename(&self.active, &self.previous).map_err(io_fault)?;
            let (file, count) = active(&self.active, self.uid)?;
            if count != 0 {
                return Err(corrupt());
            }
            state.file = file;
            state.bytes = 0;
        }
        state
            .file
            .write_all(&size.to_be_bytes())
            .map_err(io_fault)?;
        state
            .file
            .write_all(&Sha256::digest(&bytes))
            .map_err(io_fault)?;
        state.file.write_all(&bytes).map_err(io_fault)?;
        state.file.sync_data().map_err(io_fault)?;
        state.bytes += framed;
        state.next = sequence.checked_add(1).ok_or_else(|| {
            Fault::new(
                "journal_sequence",
                "Owner journal sequence is exhausted.",
                false,
            )
        })?;
        state.recent.push_back(JournalEntry {
            sequence,
            envelope: envelope.clone(),
        });
        if state.recent.len() > LOG_MEMORY_LIMIT {
            state.recent.pop_front();
        }
        Ok(())
    }
}

impl ActionJournal for FileJournal {
    fn history(&self) -> Result<JournalHistory, Fault> {
        let state = self.state.lock().expect("journal state poisoned");
        Ok(JournalHistory {
            entries: state.recent.iter().cloned().collect(),
            next_sequence: state.next,
        })
    }

    fn append(&self, sequence: u64, envelope: &KvEnvelope) -> Result<(), Fault> {
        let mut state = self.state.lock().expect("journal state poisoned");
        if state.failed {
            return Err(Fault::new(
                "journal_unavailable",
                "The owner journal already failed.",
                false,
            ));
        }
        let result = self.write(&mut state, sequence, envelope);
        if result.is_err() {
            state.failed = true;
        }
        result
    }

    fn export_log(&self, text: &str) -> Result<Option<String>, Fault> {
        let failure = || {
            Fault::new(
                "readable_log_io",
                "The readable diagnostic file could not be saved.",
                true,
            )
        };
        let mut readable = self.readable.lock().expect("readable log poisoned");
        let requested = readable.path.clone()?;
        if !requested.is_absolute() {
            return Err(failure());
        }
        let directory = requested.parent().ok_or_else(failure)?;
        let mut component_path = PathBuf::new();
        for component in directory.components() {
            if !matches!(
                component,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            ) {
                return Err(failure());
            }
            component_path.push(component);
            match fs::symlink_metadata(&component_path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err(failure()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::DirBuilder::new()
                        .mode(0o700)
                        .create(&component_path)
                        .map_err(|_| failure())?;
                }
                Err(_) => return Err(failure()),
            }
        }
        let metadata = fs::symlink_metadata(directory).map_err(|_| failure())?;
        if metadata.uid() != self.uid || metadata.mode() & 0o7777 != 0o700 {
            return Err(failure());
        }
        let directory = directory.canonicalize().map_err(|_| failure())?;
        let name = requested.file_name().ok_or_else(failure)?;
        let path = directory.join(name);
        let absolute = path.to_str().ok_or_else(failure)?;
        let content = format!("{absolute}\n{text}");
        if content.len() > 64 * 1024 {
            return Err(failure());
        }
        let existing_identity = || -> Result<Option<(u64, u64)>, Fault> {
            match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                .open(&path)
            {
                Ok(file) => {
                    validate(&file, self.uid).map_err(|_| failure())?;
                    let metadata = file.metadata().map_err(|_| failure())?;
                    Ok(Some((metadata.dev(), metadata.ino())))
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err(failure()),
            }
        };
        if existing_identity()? != readable.published {
            return Err(failure());
        }
        let mut pending_name = name.to_os_string();
        pending_name.push(".pending");
        let pending = directory.join(pending_name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&pending)
            .map_err(|_| failure())?;
        validate(&file, self.uid).map_err(|_| failure())?;
        let written = file.metadata().map_err(|_| failure())?;
        let result = (|| {
            file.write_all(content.as_bytes()).map_err(|_| failure())?;
            file.sync_all().map_err(|_| failure())?;
            let named = fs::symlink_metadata(&pending).map_err(|_| failure())?;
            if !named.is_file()
                || named.dev() != written.dev()
                || named.ino() != written.ino()
                || existing_identity()? != readable.published
            {
                return Err(failure());
            }
            let from =
                std::ffi::CString::new(pending.as_os_str().as_bytes()).map_err(|_| failure())?;
            let to = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| failure())?;
            unsafe extern "C" {
                fn renamex_np(
                    from: *const libc::c_char,
                    to: *const libc::c_char,
                    flags: u32,
                ) -> i32;
            }
            // First publication must never replace a different run's same-second filename.
            let flags = if readable.published.is_none() {
                0x0000_0004
            } else {
                0
            };
            if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), flags) } != 0 {
                return Err(failure());
            }
            readable.published = Some((written.dev(), written.ino()));
            Ok(Some(absolute.into()))
        })();
        if result.is_err() {
            if let Ok(named) = fs::symlink_metadata(&pending) {
                if named.is_file() && named.dev() == written.dev() && named.ino() == written.ino() {
                    fs::remove_file(&pending).map_err(|_| failure())?;
                }
            }
        }
        result
    }
}
// end action_log.rs
