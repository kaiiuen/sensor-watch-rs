//! Debug output log.
//!
//! A bounded ring buffer of timestamped log lines. This lets the user see what
//! the app is doing in the background (builds, flashes, face discovery) and
//! detect hangs - if the log stops advancing while an operation is in flight,
//! something is stuck. The log is bounded so it cannot grow without limit.

use std::time::{SystemTime, UNIX_EPOCH};

/// The default maximum number of log lines kept.
const MAX_LINES: usize = 500;

/// A single log entry.
#[derive(Clone)]
pub struct LogEntry {
    /// A monotonic-ish timestamp (seconds since the Unix epoch).
    pub timestamp: u64,
    /// The log message.
    pub message: String,
}

/// The debug log.
pub struct DebugLog {
    entries: Vec<LogEntry>,
    /// The maximum number of lines kept; oldest lines are dropped past this.
    limit: usize,
}

impl DebugLog {
    /// Creates an empty log with the default line limit.
    pub fn new() -> Self {
        DebugLog {
            entries: Vec::with_capacity(MAX_LINES),
            limit: MAX_LINES,
        }
    }

    /// Sets the maximum number of lines kept, dropping oldest lines immediately
    /// if the log already exceeds the new limit.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(0..excess);
        }
    }

    /// Appends a message with the current timestamp.
    pub fn log(&mut self, message: impl Into<String>) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.entries.push(LogEntry {
            timestamp: ts,
            message: message.into(),
        });
        // Bound the log.
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(0..excess);
        }
    }

    /// Clears the log.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns a reference to the entries.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Returns true if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for DebugLog {
    fn default() -> Self {
        Self::new()
    }
}
