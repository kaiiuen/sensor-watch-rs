//! Movement types and constants.
//!
//! Port of the C `movement.h`, restructured around an event-driven, interrupt-
//! powered model. The CPU is a start/stop resource: it wakes only to react to
//! a single event, then immediately returns to STANDBY. All timekeeping is
//! owned by the RTC, never by the CPU.

use crate::watch::buzzer::Note as BuzzerNote;

/// Number of watch faces (set by the config; default to a small number for now).
pub const MOVEMENT_NUM_FACES: usize = 79;

/// Long press threshold in fast ticks (128 Hz).
pub const MOVEMENT_LONG_PRESS_TICKS: u16 = 64;

/// Global settings covering watch behavior, stored in RTC backup register 0.
#[derive(Clone, Copy, Debug, Default)]
pub struct Settings {
    pub reg: u32,
}

impl Settings {
    /// The inactivity interval for asking the active face to resign.
    pub fn to_interval(self) -> u8 {
        ((self.reg >> 1) & 0x3) as u8
    }
    /// If true, always time out from the active face to face 0.
    pub fn to_always(self) -> bool {
        (self.reg >> 3) & 0x1 != 0
    }
    /// 0 to disable low energy mode, or an inactivity interval for LE mode.
    pub fn le_interval(self) -> u8 {
        ((self.reg >> 4) & 0x7) as u8
    }
    /// How many seconds to shine the LED for (x2); 0 = only while pressed.
    pub fn led_duration(self) -> u8 {
        ((self.reg >> 7) & 0x7) as u8
    }
    /// Red LED value (0-15) for general illumination.
    pub fn led_red_color(self) -> u8 {
        ((self.reg >> 10) & 0xF) as u8
    }
    /// Green LED value (0-15) for general illumination.
    pub fn led_green_color(self) -> u8 {
        ((self.reg >> 14) & 0xF) as u8
    }
    /// An index into the time zone table.
    pub fn time_zone(self) -> u8 {
        ((self.reg >> 18) & 0x3F) as u8
    }
    /// Whether the clock should use 12 or 24 hour mode.
    pub fn clock_mode_24h(self) -> bool {
        (self.reg >> 24) & 0x1 != 0
    }
    /// Whether the clock should show a leading zero in 24h mode.
    pub fn clock_24h_leading_zero(self) -> bool {
        (self.reg >> 25) & 0x1 != 0
    }
    /// Whether to use imperial units.
    pub fn use_imperial_units(self) -> bool {
        (self.reg >> 26) & 0x1 != 0
    }
    /// Whether there is at least one alarm enabled.
    pub fn alarm_enabled(self) -> bool {
        (self.reg >> 27) & 0x1 != 0
    }
    /// Whether pressing a button emits a sound.
    pub fn button_should_sound(self) -> bool {
        self.reg & 0x1 != 0
    }
    /// Whether the clock shows seconds (false = power-saving, wake once/min).
    pub fn show_seconds(self) -> bool {
        (self.reg >> 28) & 0x1 != 0
    }

    pub fn set_show_seconds(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 28)) | ((v as u32) << 28);
    }

    pub fn set_button_should_sound(&mut self, v: bool) {
        self.reg = (self.reg & !0x1) | (v as u32);
    }
    pub fn set_to_interval(&mut self, v: u8) {
        self.reg = (self.reg & !(0x3 << 1)) | ((v as u32 & 0x3) << 1);
    }
    pub fn set_to_always(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 3)) | ((v as u32) << 3);
    }
    pub fn set_le_interval(&mut self, v: u8) {
        self.reg = (self.reg & !(0x7 << 4)) | ((v as u32 & 0x7) << 4);
    }
    pub fn set_led_duration(&mut self, v: u8) {
        self.reg = (self.reg & !(0x7 << 7)) | ((v as u32 & 0x7) << 7);
    }
    pub fn set_led_red_color(&mut self, v: u8) {
        self.reg = (self.reg & !(0xF << 10)) | ((v as u32 & 0xF) << 10);
    }
    pub fn set_led_green_color(&mut self, v: u8) {
        self.reg = (self.reg & !(0xF << 14)) | ((v as u32 & 0xF) << 14);
    }
    pub fn set_time_zone(&mut self, v: u8) {
        self.reg = (self.reg & !(0x3F << 18)) | ((v as u32 & 0x3F) << 18);
    }
    pub fn set_clock_mode_24h(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 24)) | ((v as u32) << 24);
    }
    pub fn set_clock_24h_leading_zero(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 25)) | ((v as u32) << 25);
    }
    pub fn set_use_imperial_units(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 26)) | ((v as u32) << 26);
    }
    pub fn set_alarm_enabled(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 27)) | ((v as u32) << 27);
    }
}

/// A button on the watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Light,
    Mode,
    Alarm,
}

/// A button press event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
    Down,
    Up,
    LongPress,
    LongUp,
}

/// The closed set of events that wake the CPU.
///
/// The CPU wakes only for one of these, reacts, and returns to STANDBY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A watch face entered the foreground.
    Activate,
    /// The RTC ticked (once per second).
    Tick,
    /// A scheduled background task is due.
    BackgroundTask,
    /// A button was pressed.
    Button(Button, ButtonEvent),
}

impl Event {
    /// Returns the subsecond value (0 on non-tick events).
    pub fn subsecond(&self) -> u8 {
        0
    }
}

/// The interface a watch face must implement.
///
/// Faces are pure state machines: they react to a single event and return.
/// They never keep the CPU awake; periodic work is scheduled via the RTC.
pub trait WatchFace {
    /// Perform setup for the watch face (called once at boot and after sleep).
    fn setup(&mut self, settings: &Settings, watch_face_index: usize);
    /// Prepare to go on-screen.
    fn activate(&mut self, settings: &Settings);
    /// React to a single event and update the display.
    fn loop_(&mut self, event: Event, settings: &mut Settings);
    /// Prepare to go off-screen.
    fn resign(&mut self, settings: &mut Settings);
    /// OPTIONAL: request an opportunity to run a background task.
    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        false
    }
}

/// Global movement state.
pub struct MovementState {
    pub settings: Settings,
    pub current_face_idx: usize,
    pub next_face_idx: usize,
    pub watch_face_changed: bool,
    pub is_buzzing: bool,
    pub alarm_note: BuzzerNote,
    pub next_available_backup_register: u8,
}

impl MovementState {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MovementState {
            settings: Settings { reg: 0 },
            current_face_idx: 0,
            next_face_idx: 0,
            watch_face_changed: false,
            is_buzzing: false,
            alarm_note: BuzzerNote::C8,
            next_available_backup_register: 4,
        }
    }

    pub fn new() -> Self {
        MovementState {
            settings: Settings::default(),
            current_face_idx: 0,
            next_face_idx: 0,
            watch_face_changed: false,
            is_buzzing: false,
            alarm_note: BuzzerNote::C8,
            next_available_backup_register: 4,
        }
    }
}

/// Time zone offsets in minutes from UTC.
pub const TIMEZONE_OFFSETS: [i16; 41] = [
    0, 60, 120, 180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630,
    660, 720, 765, 780, 825, 840, -720, -660, -600, -570, -540, -480, -420, -360, -300, -270, -240,
    -210, -180, -150, -120, -60,
];
