//! Curated, non-invasive error and fault reference for Firmware Studio.
//!
//! The catalog is descriptive only. It never invokes a recovery action or
//! claims that a simulated result came from physical hardware.

#[derive(Clone, Copy)]
pub struct Entry {
    pub code: &'static str,
    pub area: &'static str,
    pub meaning: &'static str,
    pub likely_cause: &'static str,
    pub safe_action: &'static str,
    pub do_not_do: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        code: "FAULT-1",
        area: "Firmware",
        meaning: "WatchdogReset: the hardware watchdog reset the watch.",
        likely_cause: "A hang or code path did not service the watchdog in time.",
        safe_action: "Record the reset and recent changes. Reproduce in the simulator first. On physical hardware, inspect logs before reflashing.",
        do_not_do: "Do not repeatedly power-cycle a watch that is hot or has a low battery.",
    },
    Entry {
        code: "FAULT-2",
        area: "Firmware",
        meaning: "Panic: firmware reached a panic handler.",
        likely_cause: "An unrecoverable software invariant or explicit panic was reached.",
        safe_action: "Capture the Pxxxxxx fingerprint and resolve it against the matching source tree. This resolution is host-side.",
        do_not_do: "Do not treat a fingerprint match as proof of a hardware fault or flash an unrelated build.",
    },
    Entry {
        code: "FAULT-3",
        area: "Firmware",
        meaning: "WakeTooLong: wake-event processing exceeded its time budget.",
        likely_cause: "A slow face, peripheral operation, or unexpected work ran during wake handling.",
        safe_action: "Reduce work in the wake path and test the change in the simulator. Inspect the physical watch only after it is stable.",
        do_not_do: "Do not disable the watchdog or hold buttons while the watch is unresponsive.",
    },
    Entry {
        code: "FAULT-4",
        area: "Firmware",
        meaning: "InvalidState: an invalid event or state was encountered.",
        likely_cause: "Unexpected input, corrupted state, or a firmware logic bug.",
        safe_action: "Save the log and reproduction steps. Reset only when the device is cool and adequately powered.",
        do_not_do: "Do not clear fault storage before recording the code and count.",
    },
    Entry {
        code: "FAULT-5",
        area: "Firmware",
        meaning: "BatteryLow: battery voltage is critically low.",
        likely_cause: "A depleted CR2016 or voltage sag under load.",
        safe_action: "Stop high-load actions and replace the battery with the specified type when safe. This is a physical recovery.",
        do_not_do: "Do not flash, buzz, or force repeated resets while the battery is low.",
    },
    Entry {
        code: "FAULT-6",
        area: "Firmware",
        meaning: "RtcLostTime: the RTC heartbeat stopped advancing or time was lost.",
        likely_cause: "RTC reset, oscillator issue, or a stalled clock path.",
        safe_action: "Set time again after checking power. Compare against NTP in Studio. Physical clock hardware needs inspection if it recurs.",
        do_not_do: "Do not claim NTP fixed the oscillator or rely on stale time for safety decisions.",
    },
    Entry {
        code: "FAULT-7",
        area: "Firmware",
        meaning: "CorruptImage: firmware CRC integrity check failed.",
        likely_cause: "Incomplete, damaged, or incompatible firmware image.",
        safe_action: "Stop boot attempts, verify the intended UF2, and use the bootloader with a known-good cable and image.",
        do_not_do: "Do not run an image after integrity validation fails or interrupt a verified flash copy.",
    },
    Entry {
        code: "FAULT-8",
        area: "Firmware",
        meaning: "ClockFailure: the 32 kHz crystal failed and the RTC fell back.",
        likely_cause: "Crystal or clock circuit failure. The internal oscillator is less accurate.",
        safe_action: "Treat displayed time as approximate, sync time if possible, and seek physical hardware inspection if persistent.",
        do_not_do: "Do not calibrate around a suspected failed crystal as if it were normal drift.",
    },
    Entry {
        code: "RESET-POR",
        area: "Reset",
        meaning: "PowerOn reset reason.",
        likely_cause: "Normal power application or a complete power loss.",
        safe_action: "No action is required unless it is unexpected. Compare with battery and power history.",
        do_not_do: "Do not infer a software crash from POR alone.",
    },
    Entry {
        code: "RESET-WDT",
        area: "Reset",
        meaning: "Watchdog reset reason.",
        likely_cause: "The CPU stopped servicing the watchdog.",
        safe_action: "Correlate with FAULT-1 and capture the preceding log.",
        do_not_do: "Do not erase the evidence before collecting it.",
    },
    Entry {
        code: "RESET-PANIC",
        area: "Reset",
        meaning: "Panic reset reason.",
        likely_cause: "The panic handler reset after recording a software fault.",
        safe_action: "Resolve the six-hex-digit fingerprint in the Bugs tab using matching sources.",
        do_not_do: "Do not use a fingerprint from a different firmware build.",
    },
    Entry {
        code: "RESET-SW",
        area: "Reset",
        meaning: "Software reset reason.",
        likely_cause: "Firmware or an intentional host/test action requested reset.",
        safe_action: "Check the action log and distinguish simulator behavior from a physical reset.",
        do_not_do: "Do not label a requested reset as a spontaneous crash.",
    },
    Entry {
        code: "PANIC-FP",
        area: "Panic",
        meaning: "Pxxxxxx is a six-digit hexadecimal, 24-bit panic fingerprint.",
        likely_cause: "Hash of the panic file, line, and column retained across reset.",
        safe_action: "Enter P plus exactly six hex digits in Bugs and resolve against the source tree used to build the image.",
        do_not_do: "Do not guess a source location from the number alone.",
    },
    Entry {
        code: "NTP-DNS",
        area: "NTP",
        meaning: "NTP host name could not be resolved.",
        likely_cause: "No DNS, offline network, invalid host, or blocked resolver.",
        safe_action: "Check the host name and network. Try a known configured server. Studio networking is host-side.",
        do_not_do: "Do not change watch time based on an unverified server.",
    },
    Entry {
        code: "NTP-TIMEOUT",
        area: "NTP",
        meaning: "NTP request timed out.",
        likely_cause: "Server, route, firewall, or network latency issue.",
        safe_action: "Retry later or select another trusted server and compare offset/latency.",
        do_not_do: "Do not treat a timeout as a zero offset.",
    },
    Entry {
        code: "NTP-PACKET",
        area: "NTP",
        meaning: "NTP reply was malformed or rejected.",
        likely_cause: "Unexpected packet, incompatible server response, or transport error.",
        safe_action: "Use a trusted NTP endpoint and retain the error for diagnosis.",
        do_not_do: "Do not apply an unparsed timestamp.",
    },
    Entry {
        code: "UF2-EMPTY",
        area: "UF2",
        meaning: "Firmware binary is empty or outside the supported size.",
        likely_cause: "Failed build output, wrong artifact, or image too large.",
        safe_action: "Build again and inspect the output path and size before flashing.",
        do_not_do: "Do not copy an empty or oversized file to the bootloader drive.",
    },
    Entry {
        code: "UF2-INVALID",
        area: "UF2",
        meaning: "Generated UF2 failed structural validation.",
        likely_cause: "Bad block headers, payload, family ID, or block count.",
        safe_action: "Keep the validation error, rebuild from the intended tree, and validate before copy.",
        do_not_do: "Do not bypass validation or edit UF2 bytes manually.",
    },
    Entry {
        code: "UF2-COPY",
        area: "UF2",
        meaning: "UF2 could not be staged or copied.",
        likely_cause: "Drive disappeared, permissions, cable, or file contention.",
        safe_action: "Stop the copy, reconnect only when safe, and retry with a known-good artifact.",
        do_not_do: "Do not unplug during an active write or delete the only known-good image.",
    },
    Entry {
        code: "BUILD-CARGO",
        area: "Build",
        meaning: "Cargo build failed.",
        likely_cause: "Compiler error, missing target/toolchain, dependency, or source issue.",
        safe_action: "Read the first compiler error, fix source/toolchain setup, then rebuild.",
        do_not_do: "Do not flash a stale artifact assuming the build succeeded.",
    },
    Entry {
        code: "BUILD-OBJCPY",
        area: "Build",
        meaning: "rust-objcopy is missing or failed.",
        likely_cause: "Tool not installed, not on PATH, or ELF conversion error.",
        safe_action: "Install/configure the expected tool and rerun the build.",
        do_not_do: "Do not rename an ELF or binary and call it a UF2.",
    },
    Entry {
        code: "FLASH-DRIVE",
        area: "Flash",
        meaning: "Bootloader drive was not found.",
        likely_cause: "Watch is not in bootloader, cable/USB issue, or OS mount delay.",
        safe_action: "Enter bootloader using the documented physical procedure and verify the drive before copying.",
        do_not_do: "Do not force a file copy to an unrelated removable drive.",
    },
    Entry {
        code: "SHELL-UNKNOWN",
        area: "Shell",
        meaning: "Shell command is unknown or malformed.",
        likely_cause: "Typo, unsupported command, or simulated command differs from firmware version.",
        safe_action: "Run help and use the command syntax for the matching firmware.",
        do_not_do: "Do not paste destructive commands from an untrusted source.",
    },
    Entry {
        code: "SHELL-NOLINK",
        area: "Shell",
        meaning: "Shell transport has no physical link.",
        likely_cause: "Studio is offline/simulated or UART is disconnected.",
        safe_action: "Treat output as simulated until a documented UART seam is connected and identified.",
        do_not_do: "Do not report simulated output as a physical watch observation.",
    },
    Entry {
        code: "SETTINGS-LOAD",
        area: "Settings",
        meaning: "Persisted Studio settings could not be loaded.",
        likely_cause: "Missing, malformed, inaccessible, or incompatible settings file.",
        safe_action: "Keep a backup, inspect the file, and use defaults if needed.",
        do_not_do: "Do not overwrite the only settings backup without checking it.",
    },
    Entry {
        code: "SETTINGS-SAVE",
        area: "Settings",
        meaning: "Studio settings could not be saved.",
        likely_cause: "Permission, full disk, or interrupted file replacement.",
        safe_action: "Check the path and free space. Export settings before retrying.",
        do_not_do: "Do not assume a setting persisted just because the UI changed.",
    },
    Entry {
        code: "SEAM-MOCK",
        area: "Host seam",
        meaning: "Host Hardware seam is using a mock implementation.",
        likely_cause: "Simulator path intentionally supplies fake buttons, display, or peripherals.",
        safe_action: "Use it for repeatable software tests and label results simulated.",
        do_not_do: "Do not use mock output to clear a physical hardware warning.",
    },
    Entry {
        code: "SEAM-MISMATCH",
        area: "Host seam",
        meaning: "Real-face seam and simulator state do not match.",
        likely_cause: "Unsupported face/peripheral or stale host configuration.",
        safe_action: "Reproduce with the basic simulator and compare the selected board/profile.",
        do_not_do: "Do not infer a board failure from a host-only mismatch.",
    },
    Entry {
        code: "HW-BROWNOUT",
        area: "Hardware safety",
        meaning: "Repeated boots may indicate a battery brown-out loop.",
        likely_cause: "Low CR2016 voltage sags when buzzer or LED load turns on.",
        safe_action: "Stop high-load activity, allow the watch to settle, and replace the battery when safe.",
        do_not_do: "Do not keep forcing boots or connect unknown power sources.",
    },
    Entry {
        code: "HW-HOT",
        area: "Hardware safety",
        meaning: "Unexpected heat, smell, swelling, or visible damage is present.",
        likely_cause: "Electrical fault, damaged battery, short, or incorrect power.",
        safe_action: "Disconnect power if safe, stop using the device, and seek qualified inspection.",
        do_not_do: "Do not charge, flash, open, or continue testing a hot or damaged device.",
    },
];

pub fn areas() -> impl Iterator<Item = &'static str> {
    [
        "All",
        "Firmware",
        "Reset",
        "Panic",
        "NTP",
        "UF2",
        "Build",
        "Flash",
        "Shell",
        "Settings",
        "Host seam",
        "Hardware safety",
    ]
    .into_iter()
}

pub fn matches(entry: &Entry, query: &str, area: &str) -> bool {
    if area != "All" && entry.area != area {
        return false;
    }
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || [
            entry.code,
            entry.area,
            entry.meaning,
            entry.likely_cause,
            entry.safe_action,
            entry.do_not_do,
        ]
        .into_iter()
        .any(|text| text.to_ascii_lowercase().contains(&query))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_all_firmware_fault_codes() {
        for code in 1..=8 {
            assert!(ENTRIES
                .iter()
                .any(|entry| entry.code == format!("FAULT-{code}")));
        }
    }
    #[test]
    fn search_is_case_insensitive() {
        assert!(matches(&ENTRIES[0], "WATCHDOG", "All"));
    }
}
