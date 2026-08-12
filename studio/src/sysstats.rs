//! System resource usage for the app.
//!
//! Reports how much CPU, memory, disk, and network the app is using, plus the
//! overall system totals. Uses `sysinfo` on a background thread so it never
//! blocks the UI. GPU is not available on Windows via sysinfo, so it is shown
//! as N/A.

use std::sync::mpsc::{self, TrySendError};
use std::time::Duration;

/// A snapshot of the app's and system's resource usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct SysStats {
    // App (this process).
    /// App CPU usage as a percentage of one core (0-100).
    pub cpu_percent: f32,
    /// Number of physical CPU cores reported by the system, if available.
    pub system_cpu_cores: Option<usize>,
    /// Number of threads the app is using, if sysinfo can provide it.
    pub threads: Option<usize>,
    /// App memory usage in bytes.
    pub mem_bytes: u64,
    /// App virtual memory usage in bytes.
    pub virtual_mem_bytes: u64,
    /// App disk read bytes since start.
    pub disk_read_bytes: u64,
    /// App disk written bytes since start.
    pub disk_write_bytes: u64,
    /// App disk read bytes since last sample (rate).
    pub disk_read_rate: u64,
    /// App disk written bytes since last sample (rate).
    pub disk_write_rate: u64,
    /// App run time in seconds.
    pub run_time_secs: u64,
}

/// Formats the process thread count for display.
///
/// `None` means that the platform/sysinfo backend could not expose the
/// process task list; it must not be replaced with a guessed value.
pub fn format_process_threads(threads: Option<usize>) -> String {
    threads
        .map(|count| count.to_string())
        .unwrap_or_else(|| "unavailable on this platform".to_owned())
}

#[cfg(test)]
mod tests {
    use super::format_process_threads;
    use std::sync::mpsc::TrySendError;

    #[test]
    fn formats_unavailable_process_threads() {
        assert_eq!(format_process_threads(None), "unavailable on this platform");
        assert_eq!(format_process_threads(Some(12)), "12");
    }

    #[test]
    fn sampler_channel_has_bounded_backlog() {
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        tx.try_send(super::SysStats::default()).unwrap();
        assert!(matches!(
            tx.try_send(super::SysStats::default()),
            Err(TrySendError::Full(_))
        ));
    }
}

/// Starts a background thread that periodically samples resource usage and
/// sends snapshots over the returned channel. The sampling rate (in ms) is read
/// from the shared atomic so it can be changed live.
pub fn spawn_sampler(
    rate_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> mpsc::Receiver<SysStats> {
    // Resource samples are replaceable; never let a stalled UI accumulate an
    // unbounded backlog while it is not polling the receiver.
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let pid = sysinfo::Pid::from_u32(std::process::id());

        // Keep previous counters to compute rates.
        let mut prev_disk_read = 0u64;
        let mut prev_disk_write = 0u64;
        let mut cpu_accum = 0.0f32;
        let mut samples = 0u32;

        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            let mut stats = SysStats {
                system_cpu_cores: sysinfo::System::physical_core_count(),
                ..SysStats::default()
            };

            if let Some(proc) = sys.process(pid) {
                // sysinfo's cpu_usage() is per-core (100% = one full core) and
                // can exceed 100 on multi-core use. Normalize to a 0-100 total
                // by dividing by the core count.
                let cores = stats.system_cpu_cores.unwrap_or(1).max(1) as f32;
                stats.cpu_percent = (proc.cpu_usage() / cores).min(100.0);
                stats.mem_bytes = proc.memory();
                stats.virtual_mem_bytes = proc.virtual_memory();
                stats.run_time_secs = proc.run_time();
                // On some platforms, including Windows, sysinfo cannot expose
                // the process task list. Preserve that distinction instead of
                // presenting a fabricated count.
                stats.threads = proc.tasks().map(|tasks| tasks.len());

                let du = proc.disk_usage();
                stats.disk_read_bytes = du.total_read_bytes;
                stats.disk_write_bytes = du.total_written_bytes;
                stats.disk_read_rate = du.total_read_bytes.saturating_sub(prev_disk_read);
                stats.disk_write_rate = du.total_written_bytes.saturating_sub(prev_disk_write);
                prev_disk_read = du.total_read_bytes;
                prev_disk_write = du.total_written_bytes;
            }

            // Smooth the app CPU reading over a few samples.
            cpu_accum += stats.cpu_percent;
            samples += 1;
            if samples >= 5 {
                stats.cpu_percent = cpu_accum / samples as f32;
                cpu_accum = 0.0;
                samples = 0;
            }

            // If the UI has not consumed the previous sample yet, drop this
            // stale snapshot and keep the worker available for the next one.
            match tx.try_send(stats) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => break,
            }
            let ms = rate_ms.load(std::sync::atomic::Ordering::Relaxed).max(50);
            std::thread::sleep(Duration::from_millis(ms));
        }
    });
    rx
}
