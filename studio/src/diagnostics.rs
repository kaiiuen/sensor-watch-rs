//! Offline diagnostics state for the Studio watch shell/simulator.
//!
//! The UI deliberately keeps the transport model explicit: this module stores
//! results and bounded activity only. Real UART support can later replace the
//! command executor in `main.rs` without making simulated results look like
//! physical hardware observations.

const MAX_LOG_LINES: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Pass,
    Blocked,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Pending => "Not run",
            Status::Pass => "PASS",
            Status::Blocked => "Blocked",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
}

pub struct DiagnosticsState {
    pub rows: Vec<Row>,
    pub log: Vec<String>,
    pub running: bool,
    pub last_report: String,
}

impl DiagnosticsState {
    pub fn new() -> Self {
        let names = [
            "Shell help",
            "Time read",
            "Settime round-trip",
            "Drift command parsing",
            "RTC / clock display",
            "Face cycling",
            "Button input",
            "LCD output",
            "Watchdog / fault status",
            "UF2 / board info",
            "Optical protocol (software only)",
        ];
        Self {
            rows: names
                .into_iter()
                .map(|name| Row {
                    name,
                    status: Status::Pending,
                    detail: String::new(),
                })
                .collect(),
            log: Vec::new(),
            running: false,
            last_report: String::new(),
        }
    }

    pub fn reset(&mut self) {
        for row in &mut self.rows {
            row.status = Status::Pending;
            row.detail.clear();
        }
        self.log.clear();
        self.last_report.clear();
        self.running = true;
    }

    pub fn record(&mut self, index: usize, status: Status, detail: impl Into<String>) {
        if let Some(row) = self.rows.get_mut(index) {
            row.status = status;
            row.detail = detail.into();
        }
    }

    pub fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(0..excess);
        }
    }

    pub fn finish(&mut self, mode: &str) {
        self.running = false;
        let mut report = format!("Sensor Watch diagnostics\nMode: {mode}\n\n");
        for row in &self.rows {
            report.push_str(&format!("{}: {}", row.name, row.status.label()));
            if !row.detail.is_empty() {
                report.push_str(&format!(" - {}", row.detail));
            }
            report.push('\n');
        }
        report.push_str("\nLive log\n");
        report.push_str(&self.log.join("\n"));
        self.last_report = report;
    }
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self::new()
    }
}
