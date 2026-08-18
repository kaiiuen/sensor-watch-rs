use super::diagnostics;
use crate::movement::battery::{self, BatteryType};
use crate::movement::board::{Board, BoardConfig};
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::MockHw;

fn enter_category(face: &mut diagnostics::DiagnosticsFace, mock: &mut MockHw, cursor_moves: u8) {
    let mut settings = Settings::default();
    seam::with_hw(mock, || face.activate(&settings));
    for _ in 0..cursor_moves {
        seam::with_hw(mock, || {
            face.loop_(Event::Button(Button::Light, ButtonEvent::Up), &mut settings)
        });
    }
    seam::with_hw(mock, || {
        face.loop_(Event::Button(Button::Alarm, ButtonEvent::Up), &mut settings)
    });
}

#[test]
fn battery_selection_uses_mock_backup_register_and_cycles() {
    let mut mock = MockHw::new();
    let mut face = diagnostics::DiagnosticsFace::new_static();
    enter_category(&mut face, &mut mock, 8);

    assert_eq!(
        seam::with_hw(&mut mock, battery::battery_type),
        BatteryType::Cr2012
    );
    let mut settings = Settings::default();
    seam::with_hw(&mut mock, || {
        face.loop_(Event::Button(Button::Alarm, ButtonEvent::Up), &mut settings)
    });
    assert_eq!(
        seam::with_hw(&mut mock, battery::battery_type),
        BatteryType::Cr2016
    );
}

#[test]
fn settings_board_selection_persists_in_mock_backup_register() {
    let mut mock = MockHw::new();
    let mut face = diagnostics::DiagnosticsFace::new_static();
    enter_category(&mut face, &mut mock, 6);

    assert_eq!(
        seam::with_hw(&mut mock, || BoardConfig::read().board),
        Board::Green
    );
    let mut settings = Settings::default();
    seam::with_hw(&mut mock, || {
        face.loop_(Event::Button(Button::Alarm, ButtonEvent::Up), &mut settings)
    });
    assert_eq!(
        seam::with_hw(&mut mock, || BoardConfig::read().board),
        Board::Red
    );
}

#[test]
fn accelerometer_test_is_explicitly_absent_on_host() {
    let mut mock = MockHw::new();
    let mut face = diagnostics::DiagnosticsFace::new_static();
    enter_category(&mut face, &mut mock, 9);
    let mut settings = Settings::default();
    for _ in 0..3 {
        seam::with_hw(&mut mock, || {
            face.loop_(Event::Button(Button::Light, ButtonEvent::Up), &mut settings)
        });
    }
    seam::with_hw(&mut mock, || {
        face.loop_(Event::Button(Button::Alarm, ButtonEvent::Up), &mut settings)
    });
    // The breadcrumb deliberately occupies positions 0..3 after each draw;
    // the remaining display contains the tail of the explicit NO ACCEL result.
    assert_eq!(mock.chars[4], 'L');
    assert!(!seam::with_hw(&mut mock, crate::watch::lis2dw::begin));
}
