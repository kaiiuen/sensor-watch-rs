//! Movement framework core.
//!
//! Port of the C `movement.c`. Manages the main loop, event dispatch, button
//! handling, LED illumination, alarms, background tasks, and low-energy mode.

pub mod simple_clock;
pub mod types;

use crate::movement::types::*;
use crate::watch;
use crate::watch::buzzer::{self, Note as BuzzerNote};
use crate::watch::rtc::{self, DateTime};
use alloc::boxed::Box;

/// The global movement state.
pub static mut MOVEMENT_STATE: MovementState = MovementState::new_static();

/// The global event.
pub static mut EVENT: Event = Event {
    event_type: EventType::None,
    subsecond: 0,
};

/// The list of watch faces (filled in by the app).
pub static mut WATCH_FACES: [Option<&'static mut dyn WatchFace>; MOVEMENT_NUM_FACES] =
    [const { None }; MOVEMENT_NUM_FACES];

/// Scheduled background tasks per face.
pub static mut SCHEDULED_TASKS: [u32; MOVEMENT_NUM_FACES] = [0; MOVEMENT_NUM_FACES];

/// Resets the inactivity countdowns.
fn reset_inactivity_countdown() {
    unsafe {
        let s = &mut MOVEMENT_STATE;
        s.le_mode_ticks = LE_INACTIVITY_DEADLINES[s.settings.le_interval() as usize];
        s.timeout_ticks = TIMEOUT_INACTIVITY_DEADLINES[s.settings.to_interval() as usize];
    }
}

/// Enables the 128 Hz fast tick if not already enabled.
fn enable_fast_tick_if_needed() {
    unsafe {
        let s = &mut MOVEMENT_STATE;
        if !s.fast_tick_enabled {
            s.fast_ticks = 0;
            rtc::register_periodic_callback(cb_fast_tick, 128);
            s.fast_tick_enabled = true;
        }
    }
}

/// Disables the 128 Hz fast tick if possible.
fn disable_fast_tick_if_possible() {
    unsafe {
        let s = &mut MOVEMENT_STATE;
        if s.light_ticks == -1
            && s.alarm_ticks == -1
            && (s.light_down_timestamp + s.mode_down_timestamp + s.alarm_down_timestamp) == 0
        {
            s.fast_tick_enabled = false;
            rtc::disable_periodic_callback(128);
        }
    }
}

/// Handles background tasks for all faces.
fn handle_background_tasks() {
    unsafe {
        for i in 0..MOVEMENT_NUM_FACES {
            if let Some(face) = WATCH_FACES[i].as_deref_mut() {
                if face.wants_background_task(&MOVEMENT_STATE.settings) {
                    let background_event = Event {
                        event_type: EventType::BackgroundTask,
                        subsecond: 0,
                    };
                    face.loop_(background_event, &MOVEMENT_STATE.settings);
                }
            }
        }
        MOVEMENT_STATE.needs_background_tasks_handled = false;
    }
}

/// Handles scheduled background tasks.
fn handle_scheduled_tasks() {
    unsafe {
        let date_time = rtc::get_date_time();
        let mut num_active_tasks = 0;
        for i in 0..MOVEMENT_NUM_FACES {
            if SCHEDULED_TASKS[i] != 0 {
                if SCHEDULED_TASKS[i] <= date_time.to_reg() {
                    SCHEDULED_TASKS[i] = 0;
                    let background_event = Event {
                        event_type: EventType::BackgroundTask,
                        subsecond: 0,
                    };
                    if let Some(face) = WATCH_FACES[i].as_deref_mut() {
                        face.loop_(background_event, &MOVEMENT_STATE.settings);
                    }
                    if SCHEDULED_TASKS[i] != 0 {
                        num_active_tasks += 1;
                    }
                } else {
                    num_active_tasks += 1;
                }
            }
        }
        if num_active_tasks == 0 {
            MOVEMENT_STATE.has_scheduled_background_task = false;
        } else {
            reset_inactivity_countdown();
        }
    }
}

/// Requests a tick frequency (must be a power of 2 from 1 to 64).
pub fn request_tick_frequency(freq: u8) {
    // Movement uses the 128 Hz tick internally.
    if freq == 128 {
        return;
    }
    let freq = if freq == 0 || !freq.is_power_of_two() {
        1
    } else {
        freq
    };
    // Disable all callbacks except the 128 Hz one.
    rtc::disable_matching_periodic_callbacks(0xFE);
    unsafe {
        MOVEMENT_STATE.subsecond = 0;
        MOVEMENT_STATE.tick_frequency = freq;
    }
    rtc::register_periodic_callback(cb_tick, freq);
}

/// Illuminates the LED.
pub fn illuminate_led() {
    unsafe {
        let s = &mut MOVEMENT_STATE;
        if s.settings.led_duration() != 0b111 {
            let red = if s.settings.led_red_color() != 0 {
                (0xF | (s.settings.led_red_color() << 4)) as u8
            } else {
                0
            };
            let green = if s.settings.led_green_color() != 0 {
                (0xF | (s.settings.led_green_color() << 4)) as u8
            } else {
                0
            };
            watch::led::set_led_color(red, green);
            if s.settings.led_duration() == 0 {
                s.light_ticks = 1;
            } else {
                s.light_ticks = (s.settings.led_duration() as i16 * 2 - 1) * 128;
            }
            enable_fast_tick_if_needed();
        }
    }
}

/// Turns the LED off.
fn led_off() {
    unsafe {
        watch::led::set_led_off();
        MOVEMENT_STATE.light_ticks = -1;
        disable_fast_tick_if_possible();
    }
}

/// The default loop handler: mode button advances faces, light button illuminates.
pub fn default_loop_handler(event: Event, _settings: &Settings) -> bool {
    match event.event_type {
        EventType::ModeButtonUp => move_to_next_face(),
        EventType::LightButtonDown => illuminate_led(),
        EventType::LightButtonUp => unsafe {
            if MOVEMENT_STATE.settings.led_duration() == 0 {
                led_off();
            }
        },
        EventType::ModeLongPress => unsafe {
            if MOVEMENT_STATE.current_face_idx == 0 {
                move_to_face(0);
            } else {
                move_to_face(0);
            }
        },
        _ => {}
    }
    true
}

/// Moves to the given watch face.
pub fn move_to_face(watch_face_index: usize) {
    unsafe {
        MOVEMENT_STATE.watch_face_changed = true;
        MOVEMENT_STATE.next_face_idx = watch_face_index;
    }
}

/// Moves to the next watch face.
pub fn move_to_next_face() {
    unsafe {
        let face_max = MOVEMENT_NUM_FACES;
        move_to_face((MOVEMENT_STATE.current_face_idx + 1) % face_max);
    }
}

/// Schedules a background task for the current face.
pub fn schedule_background_task(date_time: DateTime) {
    unsafe {
        schedule_background_task_for_face(MOVEMENT_STATE.current_face_idx, date_time);
    }
}

/// Cancels the background task for the current face.
pub fn cancel_background_task() {
    unsafe {
        cancel_background_task_for_face(MOVEMENT_STATE.current_face_idx);
    }
}

/// Schedules a background task for a specific face.
pub fn schedule_background_task_for_face(watch_face_index: usize, date_time: DateTime) {
    unsafe {
        let now = rtc::get_date_time();
        if date_time.to_reg() > now.to_reg() {
            MOVEMENT_STATE.has_scheduled_background_task = true;
            SCHEDULED_TASKS[watch_face_index] = date_time.to_reg();
        }
    }
}

/// Cancels the background task for a specific face.
pub fn cancel_background_task_for_face(watch_face_index: usize) {
    unsafe {
        SCHEDULED_TASKS[watch_face_index] = 0;
        let mut other_tasks_scheduled = false;
        for i in 0..MOVEMENT_NUM_FACES {
            if SCHEDULED_TASKS[i] != 0 {
                other_tasks_scheduled = true;
                break;
            }
        }
        MOVEMENT_STATE.has_scheduled_background_task = other_tasks_scheduled;
    }
}

/// Requests a wake from low-energy mode.
pub fn request_wake() {
    unsafe {
        MOVEMENT_STATE.needs_wake = true;
        reset_inactivity_countdown();
    }
}

/// Plays the signal tune.
pub fn play_signal() {
    // TODO: port the signal tune sequence.
    unsafe {
        MOVEMENT_STATE.is_buzzing = true;
        buzzer::enable_buzzer();
        if MOVEMENT_STATE.le_mode_ticks == -1 {
            MOVEMENT_STATE.needs_wake = true;
            MOVEMENT_STATE.le_mode_ticks = 1;
        }
    }
}

/// Plays the alarm.
pub fn play_alarm() {
    play_alarm_beeps(5, BuzzerNote::C8);
}

/// Plays alarm beeps.
pub fn play_alarm_beeps(rounds: u8, alarm_note: BuzzerNote) {
    let mut rounds = rounds;
    if rounds == 0 {
        rounds = 1;
    }
    if rounds > 20 {
        rounds = 20;
    }
    request_wake();
    unsafe {
        MOVEMENT_STATE.alarm_note = alarm_note;
        MOVEMENT_STATE.alarm_ticks = 128 * rounds as i16 - 75;
        enable_fast_tick_if_needed();
    }
}

/// Claims a backup register (4-7).
pub fn claim_backup_register() -> u8 {
    unsafe {
        if MOVEMENT_STATE.next_available_backup_register >= 8 {
            return 0;
        }
        let reg = MOVEMENT_STATE.next_available_backup_register;
        MOVEMENT_STATE.next_available_backup_register += 1;
        reg
    }
}

/// App init: called once at boot.
pub fn app_init() {
    unsafe {
        rtc::freqcorr_write(22, 0);
        MOVEMENT_STATE = MovementState::new();
        MOVEMENT_STATE.settings.set_clock_mode_24h(false);
        MOVEMENT_STATE.settings.set_led_red_color(0x0);
        MOVEMENT_STATE.settings.set_led_green_color(0xF);
        MOVEMENT_STATE.settings.set_button_should_sound(true);
        MOVEMENT_STATE.settings.set_to_interval(0);
        MOVEMENT_STATE.settings.set_le_interval(2);
        MOVEMENT_STATE.settings.set_led_duration(1);
        MOVEMENT_STATE.light_ticks = -1;
        MOVEMENT_STATE.alarm_ticks = -1;
        MOVEMENT_STATE.next_available_backup_register = 4;
        reset_inactivity_countdown();
    }
}

/// App setup: called when entering the foreground.
pub fn app_setup() {
    unsafe {
        watch::deepsleep::store_backup_data(MOVEMENT_STATE.settings.reg, 0);

        // Set up the 1-minute alarm for background tasks and low-power updates.
        let alarm_time = DateTime {
            second: 59,
            minute: 0,
            hour: 0,
            day: 0,
            month: 0,
            year: 0,
        };
        rtc::register_alarm_callback(cb_alarm_fired, alarm_time, rtc::AlarmMatch::Ss);

        if MOVEMENT_STATE.le_mode_ticks != -1 {
            watch::extint::enable_external_interrupts();
            watch::extint::register_interrupt_callback(
                watch::extint::BTN_MODE,
                cb_mode_btn_interrupt,
                watch::extint::Trigger::Both,
            );
            watch::extint::register_interrupt_callback(
                watch::extint::BTN_LIGHT,
                cb_light_btn_interrupt,
                watch::extint::Trigger::Both,
            );
            watch::extint::register_interrupt_callback(
                watch::extint::BTN_ALARM,
                cb_alarm_btn_interrupt,
                watch::extint::Trigger::Both,
            );

            buzzer::enable_buzzer();
            watch::led::enable_leds();
            watch::slcd::enable_display();

            request_tick_frequency(1);

            // Register the watch faces.
            if WATCH_FACES[0].is_none() {
                WATCH_FACES[0] = Some(Box::leak(Box::new(simple_clock::SimpleClockFace::new())));
            }

            for i in 0..MOVEMENT_NUM_FACES {
                if let Some(face) = WATCH_FACES[i].as_deref_mut() {
                    face.setup(&MOVEMENT_STATE.settings, i);
                }
            }

            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.activate(&MOVEMENT_STATE.settings);
            }
            EVENT.subsecond = 0;
            EVENT.event_type = EventType::Activate;
        }
    }
}

/// The main app loop. Returns true if the watch can enter standby.
pub fn app_loop() -> bool {
    unsafe {
        let mut woke_up_for_buzzer = false;

        if MOVEMENT_STATE.watch_face_changed {
            if MOVEMENT_STATE.settings.button_should_sound() {
                let note = if MOVEMENT_STATE.next_face_idx != 0 {
                    BuzzerNote::C7
                } else {
                    BuzzerNote::C8
                };
                buzzer::play_note(note, 50);
            }
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.resign(&MOVEMENT_STATE.settings);
            }
            MOVEMENT_STATE.current_face_idx = MOVEMENT_STATE.next_face_idx;
            watch::slcd::clear_display();
            request_tick_frequency(1);
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.activate(&MOVEMENT_STATE.settings);
            }
            EVENT.subsecond = 0;
            EVENT.event_type = EventType::Activate;
            MOVEMENT_STATE.watch_face_changed = false;
        }

        // Turn the LED off if it should be off.
        if MOVEMENT_STATE.light_ticks == 0 {
            if watch::gpio::get_pin_level(watch::extint::BTN_LIGHT) {
                MOVEMENT_STATE.light_ticks = 1;
            } else {
                led_off();
            }
        }

        // Handle background tasks.
        if MOVEMENT_STATE.needs_background_tasks_handled {
            handle_background_tasks();
        }

        // Handle scheduled background tasks.
        if EVENT.event_type == EventType::Tick && MOVEMENT_STATE.has_scheduled_background_task {
            handle_scheduled_tasks();
        }

        // Enter low-energy mode if timed out.
        if MOVEMENT_STATE.le_mode_ticks == 0 {
            MOVEMENT_STATE.le_mode_ticks = -1;
            watch::deepsleep::register_extwake_callback(
                watch::deepsleep::BTN_ALARM,
                cb_alarm_btn_extwake,
                true,
            );
            EVENT.event_type = EventType::None;
            EVENT.subsecond = 0;
            sleep_mode_app_loop();
            if MOVEMENT_STATE.is_buzzing {
                woke_up_for_buzzer = true;
            }
            EVENT.event_type = EventType::Activate;
            app_setup();
        }

        let mut can_sleep = true;

        if EVENT.event_type != EventType::None {
            EVENT.subsecond = MOVEMENT_STATE.subsecond;
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                can_sleep = face.loop_(EVENT, &MOVEMENT_STATE.settings);
            }

            if MOVEMENT_STATE.light_ticks > 0 {
                match EVENT.event_type {
                    EventType::LightButtonDown
                    | EventType::ModeButtonDown
                    | EventType::AlarmButtonDown => illuminate_led(),
                    _ => {}
                }
            }
            EVENT.event_type = EventType::None;
        }

        // Handle timeout.
        if MOVEMENT_STATE.timeout_ticks == 0 {
            MOVEMENT_STATE.timeout_ticks = -1;
            if !MOVEMENT_STATE.settings.to_always() {
                EVENT.event_type = EventType::Timeout;
            }
            EVENT.subsecond = MOVEMENT_STATE.subsecond;
            let mut can_sleep2 = true;
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                can_sleep2 = face.loop_(EVENT, &MOVEMENT_STATE.settings);
            }
            can_sleep = can_sleep && can_sleep2;
            EVENT.event_type = EventType::None;
            if MOVEMENT_STATE.settings.to_always() && MOVEMENT_STATE.current_face_idx != 0 {
                move_to_face(0);
            }
        }

        // Handle the alarm.
        if MOVEMENT_STATE.alarm_ticks >= 0 {
            let buzzer_phase = (MOVEMENT_STATE.alarm_ticks + 80) % 128;
            if buzzer_phase == 127 {
                if !watch::led::is_enabled() {
                    buzzer::enable_buzzer();
                }
                for i in 0..4 {
                    buzzer::play_note(MOVEMENT_STATE.alarm_note, if i != 3 { 50 } else { 75 });
                    if i != 3 {
                        buzzer::play_note(BuzzerNote::Rest, 50);
                    }
                }
            }
            if MOVEMENT_STATE.alarm_ticks == 0 {
                MOVEMENT_STATE.alarm_ticks = -1;
                disable_fast_tick_if_possible();
            }
        }

        EVENT.subsecond = 0;

        if MOVEMENT_STATE.watch_face_changed {
            can_sleep = false;
        }
        if woke_up_for_buzzer {
            while watch::led::is_enabled() {}
        }
        if MOVEMENT_STATE.light_ticks != -1 {
            can_sleep = false;
        }

        can_sleep
    }
}

/// The sleep-mode mini runloop.
fn sleep_mode_app_loop() {
    unsafe {
        MOVEMENT_STATE.needs_wake = false;
        while MOVEMENT_STATE.le_mode_ticks == -1 {
            if MOVEMENT_STATE.needs_background_tasks_handled {
                handle_background_tasks();
            }
            EVENT.event_type = EventType::LowEnergyUpdate;
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.loop_(EVENT, &MOVEMENT_STATE.settings);
            }
            if MOVEMENT_STATE.needs_wake {
                return;
            } else {
                watch::deepsleep::enter_sleep_mode();
            }
        }
    }
}

/// Figures out the button event from the pin level and down timestamp.
fn figure_out_button_event(
    pin_level: bool,
    button_down_event_type: EventType,
    down_timestamp: &mut u16,
) -> EventType {
    unsafe {
        if MOVEMENT_STATE.alarm_ticks != 0 {
            MOVEMENT_STATE.alarm_ticks = 0;
        }
        if pin_level {
            enable_fast_tick_if_needed();
            *down_timestamp = (MOVEMENT_STATE.fast_ticks + 1) as u16;
            button_down_event_type
        } else {
            if MOVEMENT_STATE.light_ticks == 1 {
                MOVEMENT_STATE.light_ticks = 0;
            }
            let diff = MOVEMENT_STATE.fast_ticks as u16 - *down_timestamp;
            *down_timestamp = 0;
            disable_fast_tick_if_possible();
            if diff > MOVEMENT_LONG_PRESS_TICKS {
                button_down_event_type as u8 + 3
            } else {
                button_down_event_type as u8 + 1
            }
            .into()
        }
    }
}

// --- Interrupt callbacks ---

fn cb_light_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_LIGHT);
        reset_inactivity_countdown();
        EVENT.event_type = figure_out_button_event(
            pin_level,
            EventType::LightButtonDown,
            &mut MOVEMENT_STATE.light_down_timestamp,
        );
    }
}

fn cb_mode_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_MODE);
        reset_inactivity_countdown();
        EVENT.event_type = figure_out_button_event(
            pin_level,
            EventType::ModeButtonDown,
            &mut MOVEMENT_STATE.mode_down_timestamp,
        );
    }
}

fn cb_alarm_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_ALARM);
        reset_inactivity_countdown();
        EVENT.event_type = figure_out_button_event(
            pin_level,
            EventType::AlarmButtonDown,
            &mut MOVEMENT_STATE.alarm_down_timestamp,
        );
    }
}

fn cb_alarm_btn_extwake() {
    reset_inactivity_countdown();
}

fn cb_alarm_fired() {
    unsafe {
        MOVEMENT_STATE.needs_background_tasks_handled = true;
    }
}

fn cb_fast_tick() {
    unsafe {
        MOVEMENT_STATE.fast_ticks += 1;
        if MOVEMENT_STATE.light_ticks > 0 {
            MOVEMENT_STATE.light_ticks -= 1;
        }
        if MOVEMENT_STATE.alarm_ticks > 0 {
            MOVEMENT_STATE.alarm_ticks -= 1;
        }
        if MOVEMENT_STATE.light_down_timestamp > 0
            && MOVEMENT_STATE.fast_ticks as u16 - MOVEMENT_STATE.light_down_timestamp
                == MOVEMENT_LONG_PRESS_TICKS + 1
        {
            EVENT.event_type = EventType::LightLongPress;
        }
        if MOVEMENT_STATE.mode_down_timestamp > 0
            && MOVEMENT_STATE.fast_ticks as u16 - MOVEMENT_STATE.mode_down_timestamp
                == MOVEMENT_LONG_PRESS_TICKS + 1
        {
            EVENT.event_type = EventType::ModeLongPress;
        }
        if MOVEMENT_STATE.alarm_down_timestamp > 0
            && MOVEMENT_STATE.fast_ticks as u16 - MOVEMENT_STATE.alarm_down_timestamp
                == MOVEMENT_LONG_PRESS_TICKS + 1
        {
            EVENT.event_type = EventType::AlarmLongPress;
        }
        if MOVEMENT_STATE.fast_ticks >= 128 * 20 {
            rtc::disable_periodic_callback(128);
            MOVEMENT_STATE.fast_tick_enabled = false;
        }
    }
}

fn cb_tick() {
    unsafe {
        EVENT.event_type = EventType::Tick;
        let date_time = rtc::get_date_time();
        if date_time.second != MOVEMENT_STATE.last_second {
            if MOVEMENT_STATE.settings.le_interval() != 0 && MOVEMENT_STATE.le_mode_ticks > 0 {
                MOVEMENT_STATE.le_mode_ticks -= 1;
            }
            if MOVEMENT_STATE.timeout_ticks > 0 {
                MOVEMENT_STATE.timeout_ticks -= 1;
            }
            MOVEMENT_STATE.last_second = date_time.second;
            MOVEMENT_STATE.subsecond = 0;
        } else {
            MOVEMENT_STATE.subsecond += 1;
        }
    }
}
