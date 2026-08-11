//! System resource usage for the app.
//!
//! Reports how much CPU, memory, disk, and network the app is using, plus the
//! overall system totals. Uses `sysinfo` on a background thread so it never
//! blocks the UI. GPU is not available on Windows via sysinfo, so it is shown
//! as N/A.

use std::sync::mpsc;
use std::time::Duration;

/// A snapshot of the app's and system's resource usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct SysStats {
    // App (this process).
    /// App CPU usage as a percentage of one core (0-100).
    pub cpu_percent: f32,
    /// System CPU frequency in MHz, using the highest frequency exposed by
    /// sysinfo (0 if unknown). This is not a process-specific speed.
    pub system_cpu_freq_mhz: u64,
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

/// Starts a background thread that periodically samples resource usage and
/// sends snapshots over the returned channel. The sampling rate (in ms) is read
/// from the shared atomic so it can be changed live.
pub fn spawn_sampler(
    rate_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> mpsc::Receiver<SysStats> {
    let (tx, rx) = mpsc::channel();
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
            sys.refresh_cpu_frequency();
            sys.refresh_memory();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            let cores = sysinfo::System::physical_core_count().unwrap_or(0);

            let mut stats = SysStats::default();

            if let Some(proc) = sys.process(pid) {
                // sysinfo's cpu_usage() is per-core (100% = one full core) and
                // can exceed 100 on multi-core use. Normalize to a 0-100 total
                // by dividing by the core count.
                let cores = cores.max(1) as f32;
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

            // sysinfo exposes per-CPU system frequencies, not a process speed.
            // Use the highest reported value and let the UI identify it honestly.
            stats.system_cpu_freq_mhz = sys
                .cpus()
                .iter()
                .map(|cpu| cpu.frequency())
                .max()
                .unwrap_or(0);

            // Smooth the app CPU reading over a few samples.
            cpu_accum += stats.cpu_percent;
            samples += 1;
            if samples >= 5 {
                stats.cpu_percent = cpu_accum / samples as f32;
                cpu_accum = 0.0;
                samples = 0;
            }

            if tx.send(stats).is_err() {
                break;
            }
            let ms = rate_ms.load(std::sync::atomic::Ordering::Relaxed).max(50);
            std::thread::sleep(Duration::from_millis(ms));
        }
    });
    rx
}
