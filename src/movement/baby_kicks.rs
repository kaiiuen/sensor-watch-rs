//! Baby kicks watch face.
//!
//! Port of the C `baby_kicks_face.c`. Counts the movements of an in-utero
//! baby. ALARM short press starts/increments the counter, ALARM long press
//! undoes the last count, and MODE long press resets the count to zero. It is
//! a pure state machine: it reacts to a single event and returns; it never
//! keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

/// Stop counting after 99 minutes. The classic LCD cannot display any larger
/// number in the "weekday digits" position.
const BABY_KICKS_TIMEOUT: u32 = 99;

#[derive(Clone, Copy, PartialEq)]
enum BabyKicksMode {
    Splash = 0,
    Active,
    TimedOut,
    LeMode,
}

/// Ring buffer to store and allow undoing up to 10 movements.
struct BabyKicksUndoBuffer {
    /// For each movement in the undo buffer, this array stores the value of
    /// `stretch_count` right before the movement was recorded.
    stretches: [u8; 10],
    /// Index of the next available slot in `.stretches`.
    head: u8,
}

impl BabyKicksUndoBuffer {
    const fn new_static() -> Self {
        BabyKicksUndoBuffer {
            stretches: [0xff; 10],
            head: 0,
        }
    }
}

/// The baby kicks face state.
pub struct BabyKicksFace {
    currently_displayed: bool,
    mode: BabyKicksMode,
    now: watch::rtc::DateTime,
    start: u32,
    latest_stretch_start: u32,
    stretch_count: u8,   // Between 0 and BABY_KICKS_TIMEOUT.
    movement_count: u16, // Between 0 and 9999.
    undo_buffer: BabyKicksUndoBuffer,
}

impl BabyKicksFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BabyKicksFace {
            currently_displayed: false,
            mode: BabyKicksMode::Splash,
            now: watch::rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            start: 0,
            latest_stretch_start: 0,
            stretch_count: 0,
            movement_count: 0,
            undo_buffer: BabyKicksUndoBuffer::new_static(),
        }
    }

    pub fn new() -> Self {
        BabyKicksFace::new_static()
    }

    fn play_failure_sound_if_beep_is_on() {
        if movement::button_should_sound() {
            movement::play_note(Note::E7, 0);
        }
    }

    fn play_successful_increment_sound_if_beep_is_on() {
        if movement::button_should_sound() {
            movement::play_note(Note::E6, 0);
        }
    }

    fn play_successful_decrement_sound_if_beep_is_on() {
        if movement::button_should_sound() {
            movement::play_note(Note::D6, 0);
        }
    }

    fn play_button_sound_if_beep_is_on() {
        if movement::button_should_sound() {
            movement::play_note(Note::C7, 0);
        }
    }

    /// Predicate for whether the counter has been started.
    fn is_running(&self) -> bool {
        self.start > 0
    }

    /// Gets the current time, and caches it for re-use.
    fn get_now(&mut self) -> watch::rtc::DateTime {
        if self.now.year == 0 {
            self.now = movement::get_local_date_time();
        }
        self.now
    }

    /// Clears the current time. Should only be called at the end of `loop_`.
    fn clear_now(&mut self) {
        if self.now.year > 0 {
            self.now = watch::rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            };
        }
    }

    /// Calculates the number of minutes since the timer was started. Returns
    /// 0xff if the counter has not been started.
    fn elapsed_minutes(&mut self) -> u32 {
        if !self.is_running() {
            return 0xff;
        }
        let now = self.get_now();
        (watch::utility::date_time_to_unix_time(now, 0) - self.start) / 60
    }

    /// Predicate for whether the counter has started but run for too long.
    fn has_timed_out(&mut self) -> bool {
        self.elapsed_minutes() > BABY_KICKS_TIMEOUT
    }

    /// Determines what we should display based on `state`.
    fn update_display_mode(&mut self) {
        if !self.is_running() {
            self.mode = BabyKicksMode::Splash;
        } else if self.has_timed_out() {
            self.mode = BabyKicksMode::TimedOut;
        } else {
            self.mode = BabyKicksMode::Active;
        }
    }

    /// Starts the counter.
    fn start(&mut self) {
        let now = self.get_now();
        let now_unix = watch::utility::date_time_to_unix_time(now, 0);
        self.start = now_unix;
    }

    /// Resets the counter. Zeros out the watch face state and clears the undo
    /// ring buffer. Effectively sets `mode` to `Splash`.
    fn reset(&mut self) {
        self.currently_displayed = false;
        self.mode = BabyKicksMode::Splash;
        self.now = watch::rtc::DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: 0,
            month: 0,
            year: 0,
        };
        self.start = 0;
        self.latest_stretch_start = 0;
        self.stretch_count = 0;
        self.movement_count = 0;
        self.undo_buffer = BabyKicksUndoBuffer::new_static();
    }

    /// Records a movement.
    fn increment_counts(&mut self) {
        let now = self.get_now();
        let now_unix = watch::utility::date_time_to_unix_time(now, 0);

        // Add movement to the undo ring buffer.
        self.undo_buffer.stretches[self.undo_buffer.head as usize] = self.stretch_count;
        self.undo_buffer.head = (self.undo_buffer.head + 1) % 10;

        self.movement_count += 1;

        if self.stretch_count == 0 || self.latest_stretch_start + 60 < now_unix {
            // Start new stretch.
            self.latest_stretch_start = now_unix;
            self.stretch_count += 1;
        }
    }

    /// Undoes the last movement. Returns true if and only if there was a
    /// movement to undo.
    fn successfully_undo(&mut self) -> bool {
        let latest_mvmt: u8;
        // The latest movement is stored one position before `.head`.
        if self.undo_buffer.head == 0 {
            latest_mvmt = 9;
        } else {
            latest_mvmt = self.undo_buffer.head - 1;
        }

        let pre_undo_stretch_count = self.undo_buffer.stretches[latest_mvmt as usize];

        if pre_undo_stretch_count == 0xff {
            // Nothing to undo.
            return false;
        } else if pre_undo_stretch_count < self.stretch_count {
            self.latest_stretch_start = 0;
            self.stretch_count -= 1;
        }

        self.movement_count -= 1;

        self.undo_buffer.stretches[latest_mvmt as usize] = 0xff;
        self.undo_buffer.head = latest_mvmt;

        true
    }

    /// Updates the display with the movement counts if the counter has been
    /// started.
    fn display_counts(&self) {
        if !self.is_running() {
            slcd::display_string("baby  ", 4);
            slcd::clear_colon();
        } else {
            // "%2d%4d": stretch count (2 digits) then movement count (4 digits).
            let mut buf = [0u8; 6];
            buf[0] = b'0' + (self.stretch_count / 10) % 10;
            buf[1] = b'0' + self.stretch_count % 10;
            buf[2] = b'0' + (self.movement_count / 1000 % 10) as u8;
            buf[3] = b'0' + (self.movement_count / 100 % 10) as u8;
            buf[4] = b'0' + (self.movement_count / 10 % 10) as u8;
            buf[5] = b'0' + (self.movement_count % 10) as u8;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
            slcd::set_colon();
        }
    }

    /// Updates the display with the number of minutes since the timer was
    /// started. If more than `BABY_KICKS_TIMEOUT` minutes have elapsed, then
    /// it displays "TO".
    fn display_elapsed_minutes(&mut self) {
        if !self.is_running() {
            slcd::display_string("  ", 0);
            slcd::display_string("  ", 2);
        } else if self.has_timed_out() {
            slcd::display_string("TO", 0);
            slcd::display_string("  ", 2);
        } else {
            // The elapsed minutes are split into 30-minute "laps": the elapsed
            // minutes in the current lap (0-29) are shown in the "day digits"
            // position, and the completed laps (0, 30, 60, or 90) are shown in
            // the "weekday digits" position.
            let elapsed_minutes = self.elapsed_minutes();
            let multiple = elapsed_minutes / 30;
            let remainder = elapsed_minutes % 30;

            if multiple == 0 {
                slcd::display_string("  ", 0);
            } else {
                let mut buf = [0u8; 2];
                buf[0] = b'0' + (multiple * 30 / 10) as u8;
                buf[1] = b'0' + (multiple * 30 % 10) as u8;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 0);
            }

            let mut buf = [0u8; 2];
            buf[0] = b'0' + (remainder / 10) as u8;
            buf[1] = b'0' + (remainder % 10) as u8;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
        }
    }

    fn update_display(&mut self) {
        self.display_counts();
        self.display_elapsed_minutes();
    }
}

impl WatchFace for BabyKicksFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        self.reset();
    }

    fn activate(&mut self, _settings: &Settings) {
        // Sleep animation handling is not ported; nothing to do here.
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.currently_displayed = true;
                self.update_display_mode();
                self.update_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                // Update `mode` in case we have a running counter that has
                // just timed out.
                self.update_display_mode();
                match self.mode {
                    BabyKicksMode::Splash => {
                        self.start();
                        self.update_display_mode();
                        self.update_display();
                        Self::play_button_sound_if_beep_is_on();
                    }
                    BabyKicksMode::Active => {
                        self.increment_counts();
                        self.update_display();
                        Self::play_successful_increment_sound_if_beep_is_on();
                    }
                    BabyKicksMode::TimedOut => {
                        Self::play_failure_sound_if_beep_is_on();
                    }
                    BabyKicksMode::LeMode => {}
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.update_display_mode();
                match self.mode {
                    BabyKicksMode::Active => {
                        if !self.successfully_undo() {
                            Self::play_failure_sound_if_beep_is_on();
                        } else {
                            self.update_display();
                            Self::play_successful_decrement_sound_if_beep_is_on();
                        }
                    }
                    BabyKicksMode::Splash | BabyKicksMode::TimedOut => {
                        Self::play_failure_sound_if_beep_is_on();
                    }
                    BabyKicksMode::LeMode => {}
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.update_display_mode();
                match self.mode {
                    BabyKicksMode::Active | BabyKicksMode::TimedOut => {
                        self.reset();
                        // This shows the splash screen because `reset` sets
                        // `mode` to `Splash`.
                        self.update_display();
                        Self::play_button_sound_if_beep_is_on();
                    }
                    BabyKicksMode::Splash => {
                        Self::play_failure_sound_if_beep_is_on();
                    }
                    BabyKicksMode::LeMode => {}
                }
            }
            Event::BackgroundTask => {
                self.update_display_mode();
                match self.mode {
                    BabyKicksMode::Active | BabyKicksMode::TimedOut => {
                        if self.currently_displayed {
                            self.display_elapsed_minutes();
                        }
                    }
                    BabyKicksMode::LeMode | BabyKicksMode::Splash => {}
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }

        self.clear_now();
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.currently_displayed = false;
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        self.mode == BabyKicksMode::Active
    }
}
