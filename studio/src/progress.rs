use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

use crate::trace::{Phase as TracePhase, Progress as TraceProgress, Severity, TraceOperation};

pub(crate) const CHANNEL_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preflight,
    OutputRoot,
    Workspace,
    SourceSnapshot,
    GeneratedInputs,
    Lock,
    Cargo,
    Elf,
    PanicMap,
    Objcopy,
    Binary,
    Uf2,
    Provenance,
    Publication,
    Metadata,
    Approval,
    Artifact,
    Revalidation,
    DriveEnumeration,
    Identity,
    Selection,
    Transfer,
    Rollback,
    Cleanup,
    Complete,
    Failure,
}

impl Phase {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::OutputRoot => "output-root",
            Self::Workspace => "isolated-workspace",
            Self::SourceSnapshot => "source-snapshot",
            Self::GeneratedInputs => "generated-inputs",
            Self::Lock => "lock",
            Self::Cargo => "cargo",
            Self::Elf => "elf-discovery",
            Self::PanicMap => "panic-map",
            Self::Objcopy => "rust-objcopy",
            Self::Binary => "binary",
            Self::Uf2 => "uf2",
            Self::Provenance => "provenance",
            Self::Publication => "publication",
            Self::Metadata => "latest-recovery-metadata",
            Self::Approval => "approval",
            Self::Artifact => "artifact",
            Self::Revalidation => "revalidation",
            Self::DriveEnumeration => "drive enumeration",
            Self::Identity => "identity",
            Self::Selection => "selection",
            Self::Transfer => "transfer",
            Self::Rollback => "rollback",
            Self::Cleanup => "cleanup",
            Self::Complete => "complete",
            Self::Failure => "failure",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProgressEvent {
    pub(crate) operation_id: u64,
    pub(crate) sequence: u64,
    pub(crate) phase: Phase,
    pub(crate) message: String,
    pub(crate) current: Option<u64>,
    pub(crate) total: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct ProgressSink {
    sender: Option<SyncSender<ProgressEvent>>,
    operation_id: u64,
    sequence: std::sync::Arc<AtomicU64>,
    dropped: std::sync::Arc<AtomicUsize>,
    trace: Option<Arc<Mutex<Option<TraceOperation>>>>,
}

pub(crate) struct ProgressReceiver {
    receiver: Receiver<ProgressEvent>,
    dropped: std::sync::Arc<AtomicUsize>,
}

pub(crate) fn channel(operation_id: u64) -> (ProgressSink, ProgressReceiver) {
    channel_with_trace(operation_id, None)
}

pub(crate) fn channel_with_trace(
    operation_id: u64,
    trace: Option<TraceOperation>,
) -> (ProgressSink, ProgressReceiver) {
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
    let dropped = std::sync::Arc::new(AtomicUsize::new(0));
    let trace = trace.map(|mut operation| {
        let _ = operation.record(
            TracePhase::Started,
            Severity::Info,
            "operation started",
            None,
        );
        Arc::new(Mutex::new(Some(operation)))
    });
    (
        ProgressSink {
            sender: Some(sender),
            operation_id,
            sequence: std::sync::Arc::new(AtomicU64::new(0)),
            dropped: dropped.clone(),
            trace,
        },
        ProgressReceiver { receiver, dropped },
    )
}

impl ProgressSink {
    pub(crate) fn disabled() -> Self {
        Self {
            sender: None,
            operation_id: 0,
            sequence: std::sync::Arc::new(AtomicU64::new(0)),
            dropped: std::sync::Arc::new(AtomicUsize::new(0)),
            trace: None,
        }
    }

    pub(crate) fn finish(&self, success: bool, message: &str) {
        let Some(trace) = &self.trace else { return };
        if let Ok(mut operation) = trace.lock() {
            if let Some(operation) = operation.take() {
                let _ = operation.finish(
                    if success {
                        TracePhase::Completed
                    } else {
                        TracePhase::Failed
                    },
                    message,
                );
            }
        }
    }

    pub(crate) fn emit(
        &self,
        phase: Phase,
        message: impl Into<String>,
        current: Option<u64>,
        total: Option<u64>,
    ) {
        let message = message.into();
        if let Some(trace) = &self.trace {
            if let Ok(mut operation) = trace.lock() {
                if let Some(operation) = operation.as_mut() {
                    let _ = operation.record(
                        match phase {
                            Phase::Complete => TracePhase::Completed,
                            Phase::Failure => TracePhase::Failed,
                            _ => TracePhase::Running,
                        },
                        if phase == Phase::Failure {
                            Severity::Error
                        } else {
                            Severity::Info
                        },
                        format!("{}: {}", phase.label(), message),
                        current.map(|completed| TraceProgress { completed, total }),
                    );
                }
            }
        }
        let event = ProgressEvent {
            operation_id: self.operation_id,
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            phase,
            message,
            current,
            total,
        };
        if self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.try_send(event).is_err())
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            if let Some(trace) = &self.trace {
                if let Ok(mut operation) = trace.lock() {
                    if let Some(operation) = operation.as_mut() {
                        let _ = operation.record(
                            TracePhase::Marker,
                            Severity::Warning,
                            "UI progress event dropped; durable trace retained it",
                            None,
                        );
                    }
                }
            }
        }
    }
}

impl ProgressReceiver {
    pub(crate) fn drain(&self) -> Vec<ProgressEvent> {
        let mut events = Vec::with_capacity(CHANNEL_CAPACITY);
        loop {
            match self.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    pub(crate) fn take_dropped(&self) -> usize {
        self.dropped.swap(0, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_have_deterministic_order_and_progress() {
        let (sink, receiver) = channel(7);
        sink.emit(Phase::Artifact, "selected", None, None);
        sink.emit(Phase::DriveEnumeration, "root 1", Some(1), Some(3));
        sink.emit(Phase::Complete, "done", Some(3), Some(3));
        let events = receiver.drain();
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        assert_eq!(events[1].current, Some(1));
        assert_eq!(events[1].total, Some(3));
    }

    #[test]
    fn build_and_flash_phase_labels_are_ordered_and_specific() {
        let labels = [
            Phase::Preflight,
            Phase::OutputRoot,
            Phase::Workspace,
            Phase::SourceSnapshot,
            Phase::GeneratedInputs,
            Phase::Lock,
            Phase::Cargo,
            Phase::Elf,
            Phase::PanicMap,
            Phase::Objcopy,
            Phase::Binary,
            Phase::Uf2,
            Phase::Provenance,
            Phase::Publication,
            Phase::Metadata,
            Phase::Approval,
            Phase::DriveEnumeration,
            Phase::Identity,
            Phase::Transfer,
            Phase::Rollback,
            Phase::Cleanup,
            Phase::Complete,
        ];
        assert!(labels
            .windows(2)
            .all(|pair| pair[0].label() != pair[1].label()));
        assert_eq!(Phase::Cargo.label(), "cargo");
        assert_eq!(Phase::Objcopy.label(), "rust-objcopy");
    }

    #[test]
    fn full_channel_counts_dropped_events() {
        let (sink, receiver) = channel(9);
        for index in 0..(CHANNEL_CAPACITY + 3) {
            sink.emit(Phase::Transfer, format!("event {index}"), None, None);
        }
        assert_eq!(receiver.drain().len(), CHANNEL_CAPACITY);
        assert_eq!(receiver.take_dropped(), 3);
    }
}
