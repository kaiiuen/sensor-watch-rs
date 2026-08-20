use sensor_watch_launcher::Launcher;
use std::time::Duration;

fn main() {
    let base = match std::env::current_exe().and_then(|p| {
        p.parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| std::io::Error::other("launcher has no executable directory"))
    }) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("sensor-watch launcher: {error}");
            std::process::exit(1);
        }
    };
    let user_data = std::env::var_os("SENSOR_WATCH_USER_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| base.join("user-data"));
    let launcher = Launcher::new(base, user_data, "studio.exe");
    let timeout_ms = std::env::var("SENSOR_WATCH_STARTUP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15_000);
    if let Err(error) = launcher.run(Duration::from_millis(timeout_ms)) {
        eprintln!("sensor-watch launcher: {error}");
        launcher.report_failure(&error);
        std::process::exit(1);
    }
}
