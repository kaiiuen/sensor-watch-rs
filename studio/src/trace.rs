//! Structured, bounded, durable operation traces for Studio.
//!
//! This is deliberately separate from Terminal and from the legacy debug log.
//! Workers can add structured events without forcing compiler output into the
//! concise user action surface.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const TRACE_DIRECTORY: &str = "traces";
pub const DEFAULT_MAX_RECORDS: usize = 2_000;
pub const DEFAULT_MAX_BYTES: usize = 512 * 1024;
pub const DEFAULT_RETENTION: usize = 32;
const MAX_MESSAGE_CHARS: usize = 512;
const MAX_SOURCE_CHARS: usize = 128;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Build,
    Flash,
    Detect,
    Inspect,
    Simulate,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Queued,
    Started,
    Running,
    Completed,
    Failed,
    Cancelled,
    Marker,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Trace,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Origin {
    Host,
    Hardware,
    Simulated,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Host => "Host",
            Self::Hardware => "Hardware",
            Self::Simulated => "Simulated",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Progress {
    pub completed: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TraceRecord {
    pub operation_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub elapsed_ms: u64,
    pub operation_kind: OperationKind,
    pub phase: Phase,
    pub severity: Severity,
    pub source: String,
    pub origin: Origin,
    pub message: String,
    pub progress: Option<Progress>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceLimits {
    pub max_records: usize,
    pub max_bytes: usize,
}
impl Default for TraceLimits {
    fn default() -> Self {
        Self {
            max_records: DEFAULT_MAX_RECORDS,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Debug)]
pub struct TraceStore {
    root: PathBuf,
    retention: usize,
    limits: TraceLimits,
}

#[derive(Clone, Debug)]
pub struct LoadedTrace {
    pub operation_id: String,
    pub records: Vec<TraceRecord>,
    pub path: PathBuf,
}

pub struct TraceOperation {
    id: String,
    kind: OperationKind,
    source: String,
    origin: Origin,
    started: Instant,
    sequence: u64,
    records: Vec<TraceRecord>,
    limits: TraceLimits,
    part_path: PathBuf,
    final_path: PathBuf,
    finished: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn bounded(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() && c.is_ascii())
        .take(max)
        .collect()
}

/// Redacts common secret-bearing assignments and bounds output to ASCII.
pub fn sanitize_message(message: &str) -> String {
    let mut value = bounded(message, MAX_MESSAGE_CHARS);
    for key in [
        "password",
        "passwd",
        "token",
        "secret",
        "api_key",
        "apikey",
        "authorization",
    ] {
        let mut start = 0;
        while start < value.len() {
            let lower = value.to_ascii_lowercase();
            let Some(found) = lower[start..].find(key) else {
                break;
            };
            let index = start + found;
            let after = index + key.len();
            let bytes = value.as_bytes();
            let mut cursor = after;
            while cursor < bytes.len()
                && (bytes[cursor] == b' ' || bytes[cursor] == b'\t' || bytes[cursor] == b'=')
            {
                cursor += 1;
            }
            if cursor < bytes.len()
                && (bytes[cursor] == b':' || bytes[cursor] == b'"' || bytes[cursor] == b'\'')
            {
                cursor += 1;
            }
            let end = value[cursor..]
                .find([' ', '\t', '\r', '\n', ',', ';'])
                .map(|n| cursor + n)
                .unwrap_or(value.len());
            if cursor < end {
                value.replace_range(cursor..end, "[REDACTED]");
            }
            start = (cursor + 10).min(value.len());
        }
    }
    value
}

impl TraceStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        Self::with_limits(root, TraceLimits::default(), DEFAULT_RETENTION)
    }
    pub fn with_limits(
        root: impl Into<PathBuf>,
        limits: TraceLimits,
        retention: usize,
    ) -> Result<Self, String> {
        let root = root.into().join(TRACE_DIRECTORY);
        fs::create_dir_all(&root).map_err(|e| format!("cannot create trace directory: {e}"))?;
        Ok(Self {
            root,
            limits: TraceLimits {
                max_records: limits.max_records.max(1),
                max_bytes: limits.max_bytes.max(256),
            },
            retention: retention.max(1),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn start(
        &self,
        kind: OperationKind,
        source: impl Into<String>,
        origin: Origin,
    ) -> Result<TraceOperation, String> {
        self.retain()?;
        let id = format!(
            "{:016x}-{:016x}",
            now_ms(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let part_path = self.root.join(format!("{id}.jsonl.part"));
        let final_path = self.root.join(format!("{id}.jsonl"));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part_path)
            .map_err(|e| format!("cannot create trace {id}: {e}"))?;
        Ok(TraceOperation {
            id,
            kind,
            source: bounded(&source.into(), MAX_SOURCE_CHARS),
            origin,
            started: Instant::now(),
            sequence: 0,
            records: Vec::new(),
            limits: self.limits,
            part_path,
            final_path,
            finished: false,
        })
    }
    pub fn load(&self, id: &str) -> Result<LoadedTrace, String> {
        load_file(&self.root.join(format!("{id}.jsonl")))
    }
    pub fn list(&self) -> Result<Vec<LoadedTrace>, String> {
        let mut paths: Vec<_> = fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        paths.sort();
        paths.into_iter().map(|p| load_file(&p)).collect()
    }
    pub fn export_jsonl(&self, id: &str, destination: &Path) -> Result<(), String> {
        let trace = self.load(id)?;
        atomic_write(destination, &serialize_jsonl(&trace.records))
    }
    pub fn export_text(&self, id: &str, destination: &Path) -> Result<(), String> {
        let trace = self.load(id)?;
        atomic_write(destination, &serialize_text(&trace.records))
    }
    pub fn retain(&self) -> Result<(), String> {
        let mut paths: Vec<_> = fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        paths.sort();
        while paths.len() > self.retention {
            if let Some(path) = paths.first() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
            paths.remove(0);
        }
        Ok(())
    }
}

impl TraceOperation {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn records(&self) -> &[TraceRecord] {
        &self.records
    }
    pub fn record(
        &mut self,
        phase: Phase,
        severity: Severity,
        message: impl AsRef<str>,
        progress: Option<Progress>,
    ) -> Result<(), String> {
        if self.finished {
            return Err("trace operation is already finalized".into());
        }
        self.sequence += 1;
        let original = message.as_ref();
        let clean = sanitize_message(original);
        let truncated = clean.chars().count()
            < original
                .chars()
                .filter(|c| !c.is_control() && c.is_ascii())
                .count()
            || original.chars().count() > MAX_MESSAGE_CHARS;
        let record = TraceRecord {
            operation_id: self.id.clone(),
            sequence: self.sequence,
            timestamp_ms: now_ms(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            operation_kind: self.kind,
            phase,
            severity,
            source: self.source.clone(),
            origin: self.origin,
            message: clean,
            progress,
        };
        self.push(record)?;
        if truncated {
            self.push_marker("message truncated")?;
        }
        self.persist_part()
    }
    fn push(&mut self, record: TraceRecord) -> Result<(), String> {
        self.records.push(record);
        let mut evicted = 0;
        while self.records.len() > self.limits.max_records
            || serialize_jsonl(&self.records).len() > self.limits.max_bytes
        {
            if self.records.len() <= 1 {
                break;
            }
            self.records.remove(0);
            evicted += 1;
        }
        if evicted > 0
            && !self
                .records
                .iter()
                .any(|r| r.phase == Phase::Marker && r.message.starts_with("records evicted"))
        {
            let marker = self.marker("records evicted");
            self.records.push(marker);
        }
        while self.records.len() > self.limits.max_records
            || serialize_jsonl(&self.records).len() > self.limits.max_bytes
        {
            let Some(index) = self.records.iter().position(|r| r.phase != Phase::Marker) else {
                break;
            };
            self.records.remove(index);
        }
        Ok(())
    }
    fn marker(&mut self, message: &str) -> TraceRecord {
        self.sequence += 1;
        TraceRecord {
            operation_id: self.id.clone(),
            sequence: self.sequence,
            timestamp_ms: now_ms(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            operation_kind: self.kind,
            phase: Phase::Marker,
            severity: Severity::Warning,
            source: "trace".into(),
            origin: Origin::Host,
            message: message.into(),
            progress: None,
        }
    }
    fn push_marker(&mut self, message: &str) -> Result<(), String> {
        let marker = self.marker(message);
        self.push(marker)
    }
    fn persist_part(&self) -> Result<(), String> {
        atomic_write(&self.part_path, &serialize_jsonl(&self.records))
    }
    pub fn finish(mut self, phase: Phase, message: impl AsRef<str>) -> Result<LoadedTrace, String> {
        self.record(
            phase,
            if phase == Phase::Failed {
                Severity::Error
            } else {
                Severity::Info
            },
            message,
            None,
        )?;
        atomic_write(&self.part_path, &serialize_jsonl(&self.records))?;
        fs::rename(&self.part_path, &self.final_path)
            .map_err(|e| format!("cannot finalize trace: {e}"))?;
        self.finished = true;
        Ok(LoadedTrace {
            operation_id: self.id,
            records: self.records,
            path: self.final_path,
        })
    }
}

fn serialize_jsonl(records: &[TraceRecord]) -> String {
    records
        .iter()
        .filter_map(|r| serde_json::to_string(r).ok())
        .map(|s| format!("{s}\n"))
        .collect()
}
pub fn serialize_text(records: &[TraceRecord]) -> String {
    records
        .iter()
        .map(|r| {
            format!(
                "#{:04} +{}ms {:?}/{:?} {:?} {:?} [{}] {}\n",
                r.sequence,
                r.elapsed_ms,
                r.origin,
                r.operation_kind,
                r.phase,
                r.severity,
                r.source,
                r.message
            )
        })
        .collect()
}
fn load_file(path: &Path) -> Result<LoadedTrace, String> {
    let file =
        File::open(path).map_err(|e| format!("cannot open trace {}: {e}", path.display()))?;
    let mut records = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if !line.trim().is_empty() {
            records.push(
                serde_json::from_str(&line).map_err(|e| format!("invalid trace record: {e}"))?,
            );
        }
    }
    let operation_id = records
        .first()
        .map(|r: &TraceRecord| r.operation_id.clone())
        .ok_or_else(|| "trace is empty".to_string())?;
    Ok(LoadedTrace {
        operation_id,
        records,
        path: path.to_path_buf(),
    })
}
fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    {
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("studio-trace-{name}-{}", now_ms()))
    }
    fn store(name: &str) -> (TraceStore, PathBuf) {
        let root = temp(name);
        (
            TraceStore::with_limits(
                &root,
                TraceLimits {
                    max_records: 8,
                    max_bytes: 4096,
                },
                2,
            )
            .unwrap(),
            root,
        )
    }
    #[test]
    fn ordering_ids_phases_and_origin_labels() {
        let (s, root) = store("ordering");
        let mut a = s
            .start(OperationKind::Build, "builder", Origin::Hardware)
            .unwrap();
        let id = a.id().to_string();
        a.record(
            Phase::Started,
            Severity::Info,
            "one",
            Some(Progress {
                completed: 1,
                total: Some(2),
            }),
        )
        .unwrap();
        a.record(Phase::Running, Severity::Trace, "two", None)
            .unwrap();
        let t = a.finish(Phase::Completed, "done").unwrap();
        assert_eq!(t.operation_id, id);
        assert_eq!(t.records[0].sequence, 1);
        assert!(t.records.windows(2).all(|w| w[0].sequence < w[1].sequence));
        assert_eq!(Origin::Hardware.label(), "Hardware");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn caps_emit_eviction_and_truncation_markers() {
        let (s, root) = store("caps");
        let mut op = s.start(OperationKind::Other, "test", Origin::Host).unwrap();
        for _ in 0..20 {
            op.record(
                Phase::Running,
                Severity::Info,
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                None,
            )
            .unwrap();
        }
        op.record(Phase::Running, Severity::Info, "token=secret-value", None)
            .unwrap();
        let messages: Vec<_> = op.records().iter().map(|r| r.message.as_str()).collect();
        assert!(op.records().len() <= 8);
        assert!(messages.iter().any(|m| m.contains("records evicted")));
        assert!(messages.iter().any(|m| m.contains("[REDACTED]")));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn persistence_reload_and_exports_are_jsonl_and_text() {
        let (s, root) = store("reload");
        let mut op = s
            .start(OperationKind::Flash, "flasher", Origin::Simulated)
            .unwrap();
        let id = op.id().to_string();
        op.record(Phase::Started, Severity::Info, "hello", None)
            .unwrap();
        op.finish(Phase::Completed, "done").unwrap();
        let loaded = s.load(&id).unwrap();
        assert_eq!(loaded.records.len(), 2);
        let json = root.join("out.jsonl");
        let text = root.join("out.txt");
        s.export_jsonl(&id, &json).unwrap();
        s.export_text(&id, &text).unwrap();
        assert_eq!(fs::read_to_string(json).unwrap().lines().count(), 2);
        assert!(fs::read_to_string(text).unwrap().contains("Simulated"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn retention_is_bounded() {
        let (s, root) = store("retention");
        for _ in 0..3 {
            let op = s.start(OperationKind::Other, "x", Origin::Host).unwrap();
            op.finish(Phase::Completed, "done").unwrap();
        }
        s.retain().unwrap();
        assert_eq!(s.list().unwrap().len(), 2);
        let _ = fs::remove_dir_all(root);
    }
}
