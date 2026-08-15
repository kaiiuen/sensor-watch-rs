use super::*;
use crate::watch::seam;
use sensor_watch_core::datetime::DateTime;
use sensor_watch_core::mock_hw::MockHw;

fn dt(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
    sensor_watch_core::mock_hw::dt(year, month, day, hour, minute, second)
}

#[test]
fn face_indexed_host_scheduler_forwards_and_polls_explicitly() {
    let mut hw = MockHw::new();
    hw.set_time(dt(2023, 1, 1, 0, 0, 0));
    seam::with_hw(&mut hw, || {
        schedule_background_task_for_face(7, dt(2023, 1, 1, 0, 0, 1));
    });
    assert_eq!(
        hw.background_tasks.scheduled(7),
        Some(dt(2023, 1, 1, 0, 0, 1))
    );
    hw.set_time(dt(2023, 1, 1, 0, 0, 1));
    assert_eq!(hw.poll_due_background_task(), Some(7));
    assert_eq!(hw.poll_due_background_task(), None);
}

#[test]
fn host_scheduler_cancel_and_backends_are_independent() {
    let mut first = MockHw::new();
    let mut second = MockHw::new();
    let now = dt(2023, 1, 1, 0, 0, 0);
    first.set_time(now);
    second.set_time(now);
    seam::with_hw(&mut first, || {
        schedule_background_task_for_face(1, dt(2023, 1, 1, 0, 0, 1));
    });
    seam::with_hw(&mut first, || cancel_background_task_for_face(1));
    assert_eq!(first.poll_due_background_task(), None);
    assert_eq!(second.poll_due_background_task(), None);
}

#[test]
fn current_face_host_apis_remain_no_op() {
    let mut hw = MockHw::new();
    hw.set_time(dt(2023, 1, 1, 0, 0, 0));
    seam::with_hw(&mut hw, || {
        schedule_background_task(dt(2023, 1, 1, 0, 0, 1));
        cancel_background_task();
    });
    assert_eq!(hw.poll_due_background_task(), None);
}
