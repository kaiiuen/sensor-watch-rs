//! The host hardware seam: a global one-slot dispatch to an installed `Hw`.
//!
//! On the host, the firmware HAL free functions in this tree
//! (`slcd::display_string`, `rtc::get_date_time`, `adc::get_vcc_voltage`,
//! `gpio::get_button_level`, ...) forward to whatever [`Hw`] is installed via
//! [`install_hw`]. Host tests/Studio install a reuseable
//! [`MockHw`](sensor_watch_core::mock_hw::MockHw), so the *real face code* runs
//! against a recording LCD instead of SAM L22 MMIO.
//!
//! # The target vs. host split
//!
//! This module only exists in the host (`cfg(all(not(target_arch = "arm"),
//! feature = "hostmock"))`) build of the lib target. The `thumbv6m-none-eabi`
//! firmware build compiles the *real* `src/watch/*` MMIO drivers directly from
//! the binary and is completely unchanged by this seam.
//!
//! # Threading / safety
//!
//! The dispatch holds a `static mut Option<*mut dyn Hw>` (a raw fat pointer;
//! `Option` avoids building a null pointer to a `dyn` whose metadata is not
//! `Thin`). This is sound only for single-threaded host use (unit tests, a
//! single Studio simulation), which is exactly how the seam is consumed. The
//! returned short-lived `&mut dyn Hw` is re-borrowed per call, mimicking the
//! firmware's global singleton HAL functions.

use sensor_watch_core::mock_hw::Hw;

/// The installed host backend (None until [`install_hw`] is called).
static mut DISPATCH: Option<*mut dyn Hw> = None;

/// Installs `hw` as the backend that the `watch` free functions forward to.
///
/// A host test installs a `&mut MockHw` here before driving a face; the same
/// mock then records every LCD write the face makes. It is the **host analogue
/// of `watch::init()`** on the target.
///
/// The seam is single-threaded and deliberately unsafe: the `&mut` borrow is
/// promoted to `'static` so it can live in a global slot for the duration of a
/// test/simulation. The caller must keep the mock alive until the face is done
/// writing (a requirement that mirrors the ARM HAL, where the registers live
/// for the whole firmware).
pub fn install_hw(hw: &mut dyn Hw) {
    // The seam is single-threaded and deliberately leaky: the `&mut` borrow is
    // surfaced to `'static` so it can live in a global slot for the duration of
    // a test/simulation. `*mut dyn Trait` is lifetime-invariant on newer rustc,
    // so the borrow lifetime is erased explicitly. The caller must keep the mock
    // alive until the face is done writing (mirrors the ARM HAL, where the
    // registers live for the whole firmware).
    let leaked: *mut (dyn Hw + 'static) = unsafe { core::mem::transmute(hw) };
    unsafe {
        DISPATCH = Some(leaked);
    }
}

/// Clears the installed backend. Optional; for completeness.
pub fn clear_hw() {
    unsafe {
        DISPATCH = None;
    }
}

/// Returns the installed backend, panicking if none is installed.
///
/// This is the single seam point that every host `watch::*` free function calls.
#[inline]
pub fn hw() -> &'static mut dyn Hw {
    unsafe {
        match DISPATCH {
            Some(ptr) => &mut *ptr,
            None => {
                panic!(
                    "host watch: no Hw installed; call sensor_watch::watch::seam::install_hw \
                     with a &mut MockHw before driving a face"
                )
            }
        }
    }
}
