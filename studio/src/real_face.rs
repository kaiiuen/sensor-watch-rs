//! Bridge from the Studio Simulator to the REAL firmware faces through the
//! firmware host `Hw` seam.
//!
//! The firmware crate (`sensor-watch`) has a host `[lib]` (the `hostmock`
//! feature): on the host, `sensor_watch::movement::simple_clock` is the *real*
//! face source pulled in verbatim (see `src/host/movement/`), and the HAL free
//! functions (`slcd::*`, `rtc::get_date_time`, ...) forward to whatever `Hw`
//! backend is installed via the global seam. The Studio app drives the real
//! face's `WatchFace::activate`/`loop_` against a reusable `MockHw` (from
//! `sensor_watch_core::mock_hw`), so the rendered digits and indicators come
//! from the same code the firmware runs instead of the hand-written `face_sim`
//! reimplementation (`face_sim` remains the fallback for faces not yet migrated
//! through the seam).
//!
//! The host-migrated faces wired up here are the stock Casio set plus the other
//! faces whose host harness has landed in the firmware seam: `SIMPLE_CLOCK`,
//! `ALARM`, `COUNTER`, `WORLD_CLOCK`, `STOPWATCH`, `TIMER`, `COUNTDOWN`, and
//! `FLASHLIGHT`. New faces are added by extending [`new_face`] once their host
//! harness lands in the firmware seam.
//!
//! # Feature gating
//!
//! The bridge lives behind the Studio `real-faces` feature (see `Cargo.toml`),
//! because pulling the firmware *host lib* into the app as a dependency currently
//! requires the firmware seam to compile as a host lib (the firmware's `watch`
//! tree is mid-migration and not yet a clean host dependency). With the feature
//! **on**, this module drives the real faces through the seam; with it **off**
//! (the default), it exposes a fallback `RealFace` that is always `None`, so the
//! Simulator transparently keeps using `face_sim` and the app still compiles and
//! passes its tests. No Studio code needs changing; the main loop just sees
//! "seam unavailable" and falls back.

#[cfg(feature = "real-faces")]
use sensor_watch::movement::{
    alarm, countdown, counter, flashlight, simple_clock, stopwatch, timer, types, world_clock,
};
#[cfg(feature = "real-faces")]
use sensor_watch_core::datetime::DateTime;
#[cfg(feature = "real-faces")]
use sensor_watch_core::mock_hw::MockHw;

/// A snapshot of what a real face wrote to the mock LCD, in Studio terms.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealFaceSnapshot {
    /// The 10 LCD characters captured on the mock, position 0..10.
    pub chars: [char; 10],
    /// Whether the colon segment is on.
    pub colon: bool,
    /// The indicator flags, indexed+ordered like the LCD label row used by
    /// Studio's SVG mapping: signal, bell, pm, h24, lap.
    pub signal: bool,
    pub bell: bool,
    pub pm: bool,
    pub h24: bool,
    pub lap: bool,
}

// ---------------------------------------------------------------------------
// Real implementation (feature `real-faces` on).
// ---------------------------------------------------------------------------

/// A running real face. Holds the per-face state plus the mock it records onto.
#[cfg(feature = "real-faces")]
pub struct RealFace {
    /// The firmware `WatchFace`'s state.
    face: Box<dyn RealFaceTrait>,
    /// The name of the face this instance runs (used by the app to detect face
    /// switches).
    face_name: &'static str,
    /// The mock hardware the face draws onto.
    mock: MockHw,
    /// The settings the face mutates (the firmware's movement settings).
    settings: types::Settings,
    /// The display snapshot of the last render (derived from `mock`).
    snapshot: RealFaceSnapshot,
}

/// Object-safe seam over any migrated firmware `WatchFace`, so the Studio caller
/// can `activate`/`loop_` without knowing the concrete face type.
#[cfg(feature = "real-faces")]
trait RealFaceTrait {
    fn activate(&mut self, settings: &types::Settings);
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings);
}

// `SimpleClockFace`'s `WatchFace` impl supplies these via the REAL trait;
// forward the real types through so the untouched firmware face binds to the
// object-safe wrapper above.
#[cfg(feature = "real-faces")]
impl RealFaceTrait for simple_clock::SimpleClockFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

// The other host-migrated faces wired into [`new_face`] forward the same way.
// Each is the REAL firmware face; the `WatchFace` impl is the untouched trait.
#[cfg(feature = "real-faces")]
impl RealFaceTrait for alarm::AlarmFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for counter::CounterFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for world_clock::WorldClockFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for stopwatch::StopwatchFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for timer::TimerFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for countdown::CountdownFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for flashlight::FlashlightFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFace {
    /// Creates a running real face for `face_name`, if a real face of that name
    /// has been migrated into the firmware seam. Returns `None` otherwise.
    pub fn new(face_name: &str) -> Option<RealFace> {
        let face = new_face(face_name)?;
        let mut mock = MockHw::new();
        mock.vcc_mv = 3000; // healthy battery
                            // Install the mock into the host `Hw` seam so the real face's HAL calls
                            // (`slcd::*`, `rtc::get_date_time`, ...) forward to this mock instead of
                            // panicking with "no Hw installed". The `Drop` impl clears it when this
                            // face is dropped so the global slot doesn't leak between faces.
        sensor_watch::watch::seam::install_hw(&mut mock);
        let settings = types::Settings::default();
        Some(RealFace {
            face,
            mock,
            settings,
            snapshot: RealFaceSnapshot::default(),
            face_name: new_face_name(face_name),
        })
    }

    /// Sets the mock's RTC clock to the given wall-clock date/time.
    pub fn set_time(
        &mut self,
        year: u32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) {
        self.mock.now = DateTime {
            second: second as u8,
            minute: minute as u8,
            hour: hour as u8,
            day: day as u8,
            month: month as u8,
            year: (year - sensor_watch_core::datetime::WATCH_RTC_REFERENCE_YEAR as u32) as u8,
        };
    }

    /// Ready the face the way the firmware does at power-up: tell the face it's
    /// entering the foreground, then let it draw the current time. The clock
    /// mode (12/24) is mirrored to the firmware settings so both render paths
    /// agree with the watch settings in the app.
    pub fn activate(&mut self, time_mode_24: bool) {
        self.settings.set_clock_mode_24h(time_mode_24);
        self.face.activate(&self.settings);
        self.face.loop_(types::Event::Tick, &mut self.settings);
        self.snapshot_from_mock();
    }

    /// Advances the face by one second (a `Tick`), refreshing the display.
    pub fn tick(&mut self) {
        self.face.loop_(types::Event::Tick, &mut self.settings);
        self.snapshot_from_mock();
    }

    /// Drives a button press into the face. `Light` is the L button; `Alarm` is
    /// the A button (the C button cycles faces in the app rather than reaching
    /// the face).
    pub fn press(&mut self, light: bool, alarm: bool) {
        if light {
            self.face.loop_(
                types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
                &mut self.settings,
            );
        }
        if alarm {
            self.face.loop_(
                types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
                &mut self.settings,
            );
        }
        self.snapshot_from_mock();
    }

    /// The current display snapshot (LCD chars + indicators).
    pub fn snapshot(&self) -> RealFaceSnapshot {
        self.snapshot
    }

    /// The name of the face this instance runs (used by the app to detect face
    /// switches).
    pub fn face_name(&self) -> &str {
        self.face_name
    }

    fn snapshot_from_mock(&mut self) {
        let m = &self.mock;
        self.snapshot = RealFaceSnapshot {
            chars: m.chars,
            colon: m.colon,
            signal: m.indicator(sensor_watch_core::mock_hw::Indicator::Signal),
            bell: m.indicator(sensor_watch_core::mock_hw::Indicator::Bell),
            pm: m.indicator(sensor_watch_core::mock_hw::Indicator::Pm),
            h24: m.indicator(sensor_watch_core::mock_hw::Indicator::H24),
            lap: m.indicator(sensor_watch_core::mock_hw::Indicator::Lap),
        };
    }
}

/// Clears the mock from the host `Hw` seam so the global slot doesn't leak
/// between faces (e.g. when the Studio app swaps the simulated face).
impl Drop for RealFace {
    fn drop(&mut self) {
        sensor_watch::watch::seam::clear_hw();
    }
}

/// Returns a heap-allocated real face for `face_name`, if a face of that name is
/// migrated through the firmware seam. Matrix the name against the firmware's
/// upper-cased face-const name so presets ("SIMPLE_CLOCK", "simple_clock", ...)
/// resolve.
///
/// Faces not yet migrated through the seam (or whose real type needs extra setup
/// beyond a plain constructor) are intentionally absent, so the app falls back to
/// `face_sim` for them.
#[cfg(feature = "real-faces")]
fn new_face(face_name: &str) -> Option<Box<dyn RealFaceTrait>> {
    let upper = face_name.to_ascii_uppercase();
    match upper.as_str() {
        "SIMPLE_CLOCK" => Some(Box::new(simple_clock::SimpleClockFace::new())),
        "ALARM" => Some(Box::new(alarm::AlarmFace::new_static())),
        "COUNTER" => Some(Box::new(counter::CounterFace::new_static())),
        "WORLD_CLOCK" => Some(Box::new(world_clock::WorldClockFace::new_static())),
        "STOPWATCH" => Some(Box::new(stopwatch::StopwatchFace::new())),
        "TIMER" => Some(Box::new(timer::TimerFace::new())),
        "COUNTDOWN" => Some(Box::new(countdown::CountdownFace::new_static())),
        "FLASHLIGHT" => Some(Box::new(flashlight::FlashlightFace::new_static())),
        _ => None,
    }
}

/// The canonical upper-cased name of the face `face_name` resolves to, mirroring
/// [`new_face`]. Used to detect face switches in the app.
#[cfg(feature = "real-faces")]
fn new_face_name(face_name: &str) -> &'static str {
    let upper = face_name.to_ascii_uppercase();
    match upper.as_str() {
        "SIMPLE_CLOCK" => "SIMPLE_CLOCK",
        "ALARM" => "ALARM",
        "COUNTER" => "COUNTER",
        "WORLD_CLOCK" => "WORLD_CLOCK",
        "STOPWATCH" => "STOPWATCH",
        "TIMER" => "TIMER",
        "COUNTDOWN" => "COUNTDOWN",
        "FLASHLIGHT" => "FLASHLIGHT",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Fallback implementation (feature `real-faces` off).
//
// Keeps a stable `RealFace` API so `main.rs` needs no `cfg`: `new` always
// returns `None`, so the Simulator transparently falls back to `face_sim`.
// ---------------------------------------------------------------------------

/// Placeholder when the firmware seam is not enabled. `new` always yields `None`.
#[cfg(not(feature = "real-faces"))]
pub struct RealFace {
    _private: (),
}

#[cfg(not(feature = "real-faces"))]
impl RealFace {
    pub fn new(_face_name: &str) -> Option<RealFace> {
        None
    }
    pub fn set_time(&mut self, _y: u32, _mo: u32, _d: u32, _h: u32, _mi: u32, _s: u32) {}
    pub fn activate(&mut self, _time_mode_24: bool) {}
    pub fn press(&mut self, _light: bool, _alarm: bool) {}
    pub fn snapshot(&self) -> RealFaceSnapshot {
        RealFaceSnapshot::default()
    }
    pub fn face_name(&self) -> &str {
        ""
    }
}

/// Runs the real face `face_name` through the seam for the current time +
/// button flags and returns its captured LCD chars. `None` means no real face of
/// that name is available (or the seam is disabled), so the caller should fall
/// back to `face_sim`.
///
/// This is a stateless one-shot convenience API for hosting the rendered frame
/// without keeping a long-lived [`RealFace`]; the interactive Simulator instead
/// keeps a running [`RealFace`] so button/tick state persists across frames.
#[cfg(not(feature = "real-faces"))]
#[allow(dead_code)]
pub fn render_real_face(
    _face_name: &str,
    _year: u32,
    _month: u32,
    _day: u32,
    _hour: u32,
    _minute: u32,
    _second: u32,
    _weekday: u32,
    _time_mode_24: bool,
    _press_light: bool,
    _press_alarm: bool,
) -> Option<RealFaceSnapshot> {
    None
}

#[cfg(feature = "real-faces")]
#[allow(dead_code)]
pub fn render_real_face(
    face_name: &str,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    _weekday: u32,
    time_mode_24: bool,
    press_light: bool,
    press_alarm: bool,
) -> Option<RealFaceSnapshot> {
    let mut face = RealFace::new(face_name)?;
    face.set_time(year, month, day, hour, minute, second);
    face.activate(time_mode_24);
    face.press(press_light, press_alarm);
    Some(face.snapshot())
}

#[cfg(all(test, feature = "real-faces"))]
mod tests {
    use super::*;

    /// A known Friday (2023-01-06) afternoon in the app's `(year, month, day,
    /// hour, minute, second)` order, matching the reference core tests.
    fn friday() -> (u32, u32, u32, u32, u32, u32) {
        (2023, 1, 6, 15, 4, 0)
    }

    #[test]
    fn face_available_for_migrated_face() {
        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
        assert!(RealFace::new("simple_clock").is_some());
        // The stock Casio set + other host-migrated faces resolve through the seam.
        for name in [
            "ALARM",
            "COUNTER",
            "WORLD_CLOCK",
            "STOPWATCH",
            "TIMER",
            "COUNTDOWN",
            "FLASHLIGHT",
        ] {
            assert!(RealFace::new(name).is_some(), "{name} should be migrated");
        }
        // Not yet migrated through the seam => falls back in the app.
        assert!(RealFace::new("INVADERS").is_none());
    }

    #[test]
    fn real_simple_clock_renders_24h_with_seconds() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("SIMPLE_CLOCK", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("SIMPLE_CLOCK is migrated");
        // The REAL write path: FR + day 06 + HH:MM:SS with seconds on, colon on,
        // 24h indicator set.
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end(), "FR06150400");
        assert!(snap.colon);
        assert!(snap.h24);
    }

    #[test]
    fn real_simple_clock_12h_sets_pm() {
        let (y, mo, d, h, mi, s) = friday();
        // 15:04 is PM in 12-hour mode.
        let snap =
            render_real_face("SIMPLE_CLOCK", y, mo, d, h, mi, s, 5, false, false, false).unwrap();
        assert!(snap.pm);
        assert!(!snap.h24);
    }

    #[test]
    fn unmigrated_face_falls_back() {
        assert!(
            render_real_face("INVADERS", 2023, 1, 6, 15, 4, 0, 5, true, false, false).is_none()
        );
    }

    #[test]
    fn real_alarm_renders_24h() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("ALARM", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("ALARM is migrated");
        // The REAL alarm face writes a day-of-week + alarm index + time.
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end(), "AL01 000");
        assert!(snap.colon);
    }

    #[test]
    fn real_counter_renders_zero() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("COUNTER", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("COUNTER is migrated");
        // The REAL counter face shows "CO 00" and sets the signal indicator.
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end(), "CO    00");
        assert!(snap.signal);
    }
}
