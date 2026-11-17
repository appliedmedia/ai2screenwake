// journal.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Injected write-ahead observation history; replay never dispatches native work.

use super::{Fault, KvEnvelope};
use serde::{Deserialize, Serialize};

/// Recent live-memory window; older retained records remain in the durable journal.
pub const LOG_MEMORY_LIMIT: usize = 256;

/// One globally ordered entry within the retained owner journal.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    /// Persistent append position, not a file modification timestamp.
    pub sequence: u64,
    /// Exact canonical wire entry.
    pub envelope: KvEnvelope,
}

/// Startup diagnostics only, never commands to restore into the new owner.
#[derive(Clone, Debug, Default)]
pub struct JournalHistory {
    /// At most LOG_MEMORY_LIMIT recent entries in contiguous sequence order.
    pub entries: Vec<JournalEntry>,
    /// Next durable append position, including history outside the memory window.
    pub next_sequence: u64,
}

/// Mandatory first log observer when persistence is configured.
///
/// Append must durably complete before an enabling handler or successful
/// settlement is delivered. A failed append is explicit and disables further
/// durable claims; implementations must preserve failed bytes for diagnosis.
pub trait ActionJournal: Send + Sync {
    /// Load a bounded diagnostic history without replaying its actions.
    fn history(&self) -> Result<JournalHistory, Fault>;
    /// Append exactly this sequence and envelope or return an explicit failure.
    fn append(&self, sequence: u64, envelope: &KvEnvelope) -> Result<(), Fault>;
    /// Publish derived diagnostics on explicit Show Log; ephemeral journals have no file.
    fn export_log(&self, _text: &str) -> Result<Option<String>, Fault> {
        Ok(None)
    }
}
// end journal.rs
