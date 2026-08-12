//! Debug output log.
//!
//! A bounded ring buffer of timestamped log lines. This lets the user see what
//! the app is doing in the background (builds, flashes, face discovery) and
//! detect hangs - if the log stops advancing while an operation is in flight,
//! something is stuck. The log is bounded so it cannot grow without limit.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// The default maximum number of log lines kept.
const MAX_LINES: usize = 500;

/// Controls where high-frequency tick/process events are displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickVerbosity {
    Hide,
    Dedicated,
    Main,
}

/// Identifies high-frequency tick/process messages without allocating.
pub fn is_tick_or_process(message: &str) -> bool {
    fn contains_word(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(a, b)| a.to_ascii_lowercase() == *b)
        })
    }
    let bytes = message.as_bytes();
    contains_word(bytes, b"tick") || contains_word(bytes, b"process")
}

impl TickVerbosity {
    pub const ALL: [Self; 3] = [Self::Hide, Self::Dedicated, Self::Main];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hide => "Hide ticks",
            Self::Dedicated => "Tick log",
            Self::Main => "Show all in main output",
        }
    }

    pub fn setting_name(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Dedicated => "dedicated",
            Self::Main => "main",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value {
            "dedicated" => Self::Dedicated,
            "main" => Self::Main,
            _ => Self::Hide,
        }
    }
}

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
    entries: VecDeque<LogEntry>,
    /// The maximum number of lines kept; oldest lines are dropped past this.
    limit: usize,
}

impl DebugLog {
    /// Creates an empty log with the default line limit.
    pub fn new() -> Self {
        DebugLog {
            entries: VecDeque::with_capacity(MAX_LINES),
            limit: MAX_LINES,
        }
    }

    /// Sets the maximum number of lines kept, dropping oldest lines immediately
    /// if the log already exceeds the new limit.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(..excess);
        }
    }

    /// Appends a message with the current timestamp.
    pub fn log(&mut self, message: impl Into<String>) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.entries.push_back(LogEntry {
            timestamp: ts,
            message: message.into(),
        });
        // Bound the log.
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(..excess);
        }
    }

    /// Clears the log.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns a reference to the entries.
    pub fn entries(&self) -> &VecDeque<LogEntry> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_limit_drops_oldest_entries_and_never_allows_zero() {
        let mut log = DebugLog::new();
        log.set_limit(3);
        for message in ["one", "two", "three", "four"] {
            log.log(message);
        }
        assert_eq!(
            log.entries()
                .iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            ["two", "three", "four"]
        );

        log.set_limit(0);
        log.log("five");
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].message, "five");
    }

    #[test]
    fn tick_detection_is_case_insensitive_and_does_not_misclassify_unrelated_text() {
        assert!(is_tick_or_process("PROCESS completed"));
        assert!(is_tick_or_process("face tick"));
        assert!(!is_tick_or_process("build completed"));
    }
}
