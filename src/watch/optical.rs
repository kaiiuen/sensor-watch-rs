//! Bounded optical/IrDA receive boundary.
//!
//! `pro-irda-rx` is a Sensor Watch Pro-only, opt-in path. It ports the
//! reference wiring/mode setup but deliberately does not port the community
//! file upload/delete protocol. TimeSync frames remain receive-only until
//! production authentication/key provisioning exists.

#[cfg(feature = "pro-irda-rx")]
use sensor_watch_core::optical::{OpticalIo, OpticalSession, SessionState};

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
struct UartIo {
    now_ms: u32,
}

#[cfg(feature = "pro-irda-rx")]
impl OpticalIo for UartIo {
    fn read_byte(&mut self) -> Option<u8> {
        crate::watch::uart::try_getc_pro_irda()
    }
    fn now_ms(&mut self) -> u32 {
        self.now_ms
    }
    fn queue_ack(&mut self, _sequence: u32) {}
    fn apply_rtc(&mut self, _packed_datetime: u32) -> Result<(), ()> {
        Err(())
    }
}

#[cfg(feature = "pro-irda-rx")]
static mut SESSION: OpticalSession = OpticalSession::receive_only();
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
    let mut io = UartIo { now_ms };
    let result = critical_section::with(|_| unsafe { SESSION.service(&mut io, None) });
    if let Some(result) = result {
        match result {
            Ok(SessionState::AckQueued) => unsafe { FRAMES = FRAMES.saturating_add(1) },
            Err(_) => unsafe { ERRORS = ERRORS.saturating_add(1) },
            _ => {}
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
