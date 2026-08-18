//! Bounded optical/IrDA receive boundary.
//!
//! `pro-irda-rx` is a Sensor Watch Pro-only, opt-in path. It ports the
//! reference wiring/mode setup but deliberately does not port the community
//! file upload/delete protocol. TimeSync frames remain receive-only until
//! production authentication/key provisioning exists.

#[cfg(feature = "pro-irda-rx")]
use sensor_watch_core::optical::{self, DecodeError, Decoder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Disabled,
    SensorUnavailable,
    ReceiveOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    SensorUnavailable,
    Disabled,
}

#[cfg(feature = "pro-irda-rx")]
static mut DECODER: Decoder = Decoder::new();
#[cfg(feature = "pro-irda-rx")]
static mut FRAMES: u32 = 0;
#[cfg(feature = "pro-irda-rx")]
static mut ERRORS: u32 = 0;

pub const fn state() -> State {
    if cfg!(feature = "pro-irda-rx") {
        State::ReceiveOnly
    } else {
        State::SensorUnavailable
    }
}

/// Starts the Pro receiver. The 900 baud setting is experimental, matching the
/// checked-in community/reference behavior; it is not a hardware validation.
#[cfg(feature = "pro-irda-rx")]
pub fn enable() -> bool {
    crate::watch::uart::enable_pro_irda_receive(900)
}

#[cfg(not(feature = "pro-irda-rx"))]
pub fn enable() -> bool {
    false
}

/// Services at most 64 UART bytes and returns immediately; callers can then
/// enter standby. No RTC writes occur in this receive-only integration.
#[cfg(feature = "pro-irda-rx")]
pub fn poll_at(now_ms: u32) {
    crate::watch::uart::service_pro_irda_rx();
    for _ in 0..64 {
        let Some(byte) = crate::watch::uart::try_getc_pro_irda() else {
            break;
        };
        let result = critical_section::with(|_| unsafe { DECODER.push(byte, now_ms, None) });
        if let Some(result) = result {
            match result {
                Ok(frame) => unsafe {
                    FRAMES = FRAMES.saturating_add(1);
                    if frame.command == optical::CommandType::TimeSync {
                        ERRORS = ERRORS.saturating_add(1);
                    }
                },
                Err(DecodeError::Authentication) => unsafe { ERRORS = ERRORS.saturating_add(1) },
                Err(_) => unsafe { ERRORS = ERRORS.saturating_add(1) },
            }
        }
    }
}

#[cfg(not(feature = "pro-irda-rx"))]
pub fn poll_at(_now_ms: u32) {}

/// Compatibility poll entry point. The movement service supplies real tick
/// time through `poll_at`; this no-op keeps unsupported boards unavailable.
pub fn poll() -> Result<(), Error> {
    if state() == State::SensorUnavailable {
        Err(Error::SensorUnavailable)
    } else {
        Ok(())
    }
}

pub const fn status_text() -> &'static str {
    if cfg!(feature = "pro-irda-rx") {
        "OPTICAL receive-only: Pro IrDA experimental 900 baud; RTC mutation disabled; NOT TESTED"
    } else {
        "OPTICAL disabled: SensorUnavailable (no board receiver configured)"
    }
}

#[cfg(feature = "pro-irda-rx")]
pub fn diagnostics() -> (u32, u32) {
    critical_section::with(|_| unsafe { (FRAMES, ERRORS) })
}
