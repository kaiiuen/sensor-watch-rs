//! Decimal time watch face.
//!
//! Port of the C `decimal_time_face.c`. Displays the time in decimal (French
//! Revolutionary) format, where each day is divided into 10 decimal hours.
//! Always 24h-style. The Alarm button cycles through display modes; a long
//! press toggles an hourly chime.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The decimal time face state.
pub struct DecimalTimeFace {
    chime_enabled: bool,
    features_to_show: u8,
}

impl DecimalTimeFace {
    pub const fn new_static() -> Self {
        DecimalTimeFace {
            chime_enabled: false,
            features_to_show: 0,
        }
    }
}

impl WatchFace for DecimalTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        // This face is always 24h.
        watch::slcd::set_indicator(Indicator::H24);
        watch::slcd::clear_indicator(Indicator::Pm);
        if self.chime_enabled {
            watch::slcd::set_indicator(Indicator::Bell);
        }
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                let dt = rtc::get_date_time();
                let minutes_of_hour = dt.minute as u32 * 60 + dt.second as u32;
                let centihours = (minutes_of_hour * 100) / 3600;
                let decimal_seconds = minutes_of_hour % 36;

                let mut buf = [0u8; 11];
                buf[0] = b'd';
                buf[1] = b'T';
                buf[2] = b' ';
                match self.features_to_show {
                    0 => {
                        buf[3] = b' ';
                        buf[4] = b'0' + dt.hour / 10;
                        buf[5] = b'0' + dt.hour % 10;
                        buf[6] = b'0' + (centihours / 10) as u8;
                        buf[7] = b'0' + (centihours % 10) as u8;
                        buf[8] = b' ';
                        buf[9] = b' ';
                    }
                    1 => {
                        buf[3] = b' ';
                        buf[4] = b'0' + dt.hour / 10;
                        buf[5] = b'0' + dt.hour % 10;
                        buf[6] = b'0' + (centihours / 10) as u8;
                        buf[7] = b'0' + (centihours % 10) as u8;
                        buf[8] = b'0' + (decimal_seconds / 10) as u8;
                        buf[9] = b'0' + (decimal_seconds % 10) as u8;
                    }
                    2 => {
                        buf[3] = b'0' + dt.day / 10;
                        buf[4] = b'0' + dt.day % 10;
                        buf[5] = b'0' + dt.hour / 10;
                        buf[6] = b'0' + dt.hour % 10;
                        buf[7] = b'0' + (centihours / 10) as u8;
                        buf[8] = b'0' + (centihours % 10) as u8;
                        buf[9] = b' ';
                    }
                    _ => {
                        buf[3] = b'0' + dt.day / 10;
                        buf[4] = b'0' + dt.day % 10;
                        buf[5] = b'0' + dt.hour / 10;
                        buf[6] = b'0' + dt.hour % 10;
                        buf[7] = b'0' + (centihours / 10) as u8;
                        buf[8] = b'0' + (centihours % 10) as u8;
                        buf[9] = b'0' + (decimal_seconds / 10) as u8;
                    }
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);

                // Chime at the top of every hour.
                if dt.minute == 0 && dt.second == 0 && self.chime_enabled {
                    watch::slcd::set_indicator(Indicator::Signal);
                    crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::E6);
                    watch::slcd::clear_indicator(Indicator::Signal);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.chime_enabled = !self.chime_enabled;
                if self.chime_enabled {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.features_to_show = (self.features_to_show + 1) % 4;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
