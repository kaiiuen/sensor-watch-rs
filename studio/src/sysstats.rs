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
    /// App CPU frequency in MHz (best-effort; 0 if unknown).
    pub cpu_freq_mhz: u64,
    /// Number of threads the app is using.
    pub threads: usize,
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

    // System (whole machine).
    /// Total system CPU usage as a percentage (0-100).
    pub sys_cpu_percent: f32,
    /// Total system memory used in bytes.
    pub sys_mem_used_bytes: u64,
    /// Total system memory in bytes.
    pub total_mem_bytes: u64,
    /// Number of physical CPU cores.
    pub physical_cores: usize,
    /// Total system network received bytes since last sample (rate).
    pub sys_net_rx_rate: u64,
    /// Total system network transmitted bytes since last sample (rate).
    pub sys_net_tx_rate: u64,
}

/// Starts a background thread that periodically samples resource usage and
/// sends snapshots over the returned channel.
pub fn spawn_sampler() -> mpsc::Receiver<SysStats> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let mut networks = sysinfo::Networks::new_with_refreshed_list();
        let pid = sysinfo::Pid::from_u32(std::process::id());

        // Keep previous counters to compute rates.
        let mut prev_disk_read = 0u64;
        let mut prev_disk_write = 0u64;
        let mut prev_net_rx = 0u64;
        let mut prev_net_tx = 0u64;
        let mut cpu_accum = 0.0f32;
        let mut samples = 0u32;

        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            networks.refresh(true);

            let sys_cpu = sys.global_cpu_usage();
            let total_mem = sys.total_memory();
            let used_mem = sys.used_memory();
            let cores = sysinfo::System::physical_core_count().unwrap_or(0);

            // Sum network rates across all interfaces.
            let mut net_rx = 0u64;
            let mut net_tx = 0u64;
            for data in networks.list().values() {
                net_rx += data.received();
                net_tx += data.transmitted();
            }

            let mut stats = SysStats {
                sys_cpu_percent: sys_cpu,
                sys_mem_used_bytes: used_mem,
                total_mem_bytes: total_mem,
                physical_cores: cores,
                sys_net_rx_rate: net_rx.saturating_sub(prev_net_rx),
                sys_net_tx_rate: net_tx.saturating_sub(prev_net_tx),
                ..Default::default()
            };
            prev_net_rx = net_rx;
            prev_net_tx = net_tx;

            if let Some(proc) = sys.process(pid) {
                stats.cpu_percent = proc.cpu_usage();
                stats.mem_bytes = proc.memory();
                stats.virtual_mem_bytes = proc.virtual_memory();
                stats.run_time_secs = proc.run_time();
                stats.threads = proc.tasks().map(|t| t.len()).unwrap_or(0);

                let du = proc.disk_usage();
                stats.disk_read_bytes = du.total_read_bytes;
                stats.disk_write_bytes = du.total_written_bytes;
                stats.disk_read_rate = du.total_read_bytes.saturating_sub(prev_disk_read);
                stats.disk_write_rate = du.total_written_bytes.saturating_sub(prev_disk_write);
                prev_disk_read = du.total_read_bytes;
                prev_disk_write = du.total_written_bytes;
            }

            // CPU frequency: report the max across cores as a rough app value.
            stats.cpu_freq_mhz = sys.cpus().iter().map(|c| c.frequency()).max().unwrap_or(0);

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
            std::thread::sleep(Duration::from_millis(1000));
        }
    });
    rx
}
