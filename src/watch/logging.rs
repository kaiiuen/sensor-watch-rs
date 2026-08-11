//! Optional structured firmware logging.
//!
//! The fixed `event_log` ring remains the always-available fallback. When the
//! `defmt-log` feature is enabled, the same events are also emitted to the RTT
//! transport supplied by `defmt-rtt` for an SWD probe.

#[cfg(feature = "defmt-log")]
use defmt_rtt as _;

/// Emits one structured event when RTT logging is enabled.
#[cfg(feature = "defmt-log")]
#[inline]
pub fn event(event: super::event_log::Event) {
    defmt::info!(
        "event seq={} timestamp={} code={} data={}",
        event.sequence,
        event.timestamp,
        event.code,
        event.data
    );
}

/// No-op event sink for the normal firmware build.
#[cfg(not(feature = "defmt-log"))]
#[inline]
pub fn event(_event: super::event_log::Event) {}

/// Emits a fault record, including the stable fault code.
#[cfg(feature = "defmt-log")]
#[inline]
pub fn fault(code: u8) {
    defmt::error!("fault code={}", code);
}

/// No-op fault sink for the normal firmware build.
#[cfg(not(feature = "defmt-log"))]
#[inline]
pub fn fault(_code: u8) {}

/// Emits a reset reason record.
#[cfg(feature = "defmt-log")]
#[inline]
pub fn reset(reason: u8) {
    defmt::warn!("reset reason={}", reason);
}

/// No-op reset sink for the normal firmware build.
#[cfg(not(feature = "defmt-log"))]
#[inline]
pub fn reset(_reason: u8) {}

/// Emits the persisted panic fingerprint when one is available.
#[cfg(feature = "defmt-log")]
#[inline]
pub fn panic_fingerprint(fingerprint: u32) {
    defmt::error!("panic fingerprint={=u32:08x}", fingerprint);
}

/// No-op panic fingerprint sink for the normal firmware build.
#[cfg(not(feature = "defmt-log"))]
#[inline]
pub fn panic_fingerprint(_fingerprint: u32) {}
