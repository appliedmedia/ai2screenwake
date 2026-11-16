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
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};

const VERSION: u8 = 1;
const FILE_LIMIT: u64 = 1024 * 1024;
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

/// One owner-locked journal plus one bounded archive, both private and checksummed.
pub(crate) struct FileJournal {
    active: PathBuf,
    previous: PathBuf,
    uid: u32,
    state: Mutex<State>,
}

impl FileJournal {
    pub(crate) fn open(directory: &Path, uid: u32) -> Result<Self, Fault> {
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
        let size = u32::try_from(bytes.len()).map_err(|_| {
            Fault::new(
                "journal_entry_large",
                "Owner journal entry length is invalid.",
                false,
            )
        })?;
        let framed = u64::from(size) + 36;
        if state.bytes > 0 && state.bytes + framed > FILE_LIMIT {
            state.file.sync_all().map_err(io_fault)?;
            match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
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
}
// end action_log.rs
