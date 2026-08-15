use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};

pub(crate) const CHANNEL_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
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
}

pub(crate) struct ProgressReceiver {
    receiver: Receiver<ProgressEvent>,
    dropped: std::sync::Arc<AtomicUsize>,
}

pub(crate) fn channel(operation_id: u64) -> (ProgressSink, ProgressReceiver) {
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
    let dropped = std::sync::Arc::new(AtomicUsize::new(0));
    (
        ProgressSink {
            sender: Some(sender),
            operation_id,
            sequence: std::sync::Arc::new(AtomicU64::new(0)),
            dropped: dropped.clone(),
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
        }
    }

    pub(crate) fn emit(
        &self,
        phase: Phase,
        message: impl Into<String>,
        current: Option<u64>,
        total: Option<u64>,
    ) {
        let event = ProgressEvent {
            operation_id: self.operation_id,
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            phase,
            message: message.into(),
            current,
            total,
        };
        if self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.try_send(event).is_err())
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
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
    fn full_channel_counts_dropped_events() {
        let (sink, receiver) = channel(9);
        for index in 0..(CHANNEL_CAPACITY + 3) {
            sink.emit(Phase::Transfer, format!("event {index}"), None, None);
        }
        assert_eq!(receiver.drain().len(), CHANNEL_CAPACITY);
        assert_eq!(receiver.take_dropped(), 3);
    }
}
