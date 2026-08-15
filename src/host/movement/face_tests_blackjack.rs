use crate::movement::blackjack::BlackjackFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{Indicator, MockHw, dt};

fn ready() -> (MockHw, Settings, BlackjackFace) {
    let mut hw = MockHw::new();
    hw.set_time(dt(2023, 1, 6, 15, 4, 0));
    hw.vcc_mv = 3000;
    (hw, Settings::default(), BlackjackFace::new())
}

fn activate(hw: &mut MockHw, face: &mut BlackjackFace, settings: &Settings) {
    seam::with_hw(hw, || face.activate(settings));
    seam::with_hw(hw, || face.loop_(Event::Activate, &mut settings.clone()));
}

#[test]
fn blackjack_activation_shows_title_and_activate_event_is_safe() {
    let (mut hw, settings, mut face) = ready();
    activate(&mut hw, &mut face, &settings);

    assert_eq!(hw.text(), "21  BLaKJK");
}

#[test]
fn blackjack_game_start_is_deterministic_for_a_fixed_rtc() {
    let (mut first_hw, first_settings, mut first) = ready();
    let (mut second_hw, second_settings, mut second) = ready();
    activate(&mut first_hw, &mut first, &first_settings);
    activate(&mut second_hw, &mut second, &second_settings);

    seam::with_hw(&mut first_hw, || {
        first.loop_(
            Event::Button(Button::Light, ButtonEvent::Up),
            &mut first_settings.clone(),
        )
    });
    seam::with_hw(&mut second_hw, || {
        second.loop_(
            Event::Button(Button::Light, ButtonEvent::Up),
            &mut second_settings.clone(),
        )
    });

    assert_eq!(first_hw.chars, second_hw.chars);
    assert_eq!(first_hw.colon, second_hw.colon);
}

#[test]
fn blackjack_hit_stand_and_dealer_end_states_are_bounded() {
    let (mut hw, settings, mut face) = ready();
    activate(&mut hw, &mut face, &settings);

    // Start, hit once, stand, then give the dealer enough ticks to finish. The
    // real face must remain within its 11-card hands and 10 LCD positions.
    seam::with_hw(&mut hw, || {
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::Up),
            &mut settings.clone(),
        );
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::Up),
            &mut settings.clone(),
        );
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::Up),
            &mut settings.clone(),
        );
        for _ in 0..32 {
            face.loop_(Event::Tick, &mut settings.clone());
        }
    });

    assert_eq!(hw.chars.len(), 10);
    assert!(hw.text().len() <= 10);
}

#[test]
fn blackjack_win_ratio_and_reset_path_are_safe() {
    let (mut hw, settings, mut face) = ready();
    activate(&mut hw, &mut face, &settings);

    seam::with_hw(&mut hw, || {
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::LongPress),
            &mut settings.clone(),
        );
    });
    assert!(hw.text().contains("WR"));

    seam::with_hw(&mut hw, || {
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::LongPress),
            &mut settings.clone(),
        );
    });
    assert!(hw.text().contains("0Pct"));
}

#[test]
fn blackjack_tap_falls_back_to_buttons_and_resign_is_safe() {
    let (mut hw, settings, mut face) = ready();
    activate(&mut hw, &mut face, &settings);
    let title = hw.chars;

    // Host tap detection is unavailable: a tap must not start a game.
    seam::with_hw(&mut hw, || {
        face.loop_(Event::SingleTap, &mut settings.clone())
    });
    assert_eq!(hw.chars, title);

    // Resign must remove a stale face-owned signal indicator even when tap
    // detection is already off, and repeated resigns must remain harmless.
    seam::with_hw(&mut hw, || {
        crate::watch::slcd::set_indicator(Indicator::Signal);
        crate::watch::slcd::set_indicator(Indicator::Bell);
    });
    assert!(hw.indicator(Indicator::Signal));
    assert!(hw.indicator(Indicator::Bell));

    seam::with_hw(&mut hw, || face.resign(&mut settings.clone()));
    assert!(!hw.indicator(Indicator::Signal));
    assert!(hw.indicator(Indicator::Bell));

    seam::with_hw(&mut hw, || face.resign(&mut settings.clone()));
    assert!(!hw.indicator(Indicator::Signal));
    assert!(hw.indicator(Indicator::Bell));
    assert_eq!(hw.chars.len(), 10);
}
