//! Host-compilable library target for the real firmware watch faces.
//!
//! The `sensor-watch` crate is primarily a bare-metal binary (`src/main.rs`,
//! `#![no_std]` / `#![no_main]`, Cortex-M0+ target) that owns the ARM `watch` +
//! `movement` module trees. This `lib` target lets the *same real face code* be
//! compiled and run on the host (dev machine, Studio, fuzzers) through the `Hw`
//! seam, instead of Studio's hand-written `face_sim.rs`.
//!
//! # Target vs. host split
//!
//! - **ARM** (`target_arch = "arm"`, i.e. `thumbv6m-none-eabi`): the lib simply
//!   re-exports the *real*, untouched `watch` (`src/watch/mod.rs`) and `movement`
//!   (`src/movement/mod.rs`) module trees. This is the same code the firmware
//!   binary compiles, so the on-target firmware is unchanged (verified by release
//!   build hash). A minimal `#[panic_handler]` is provided because this lib
//!   artifact is compiled (but never linked into the firmware binary, which has
//!   its own handler in `src/panic.rs`).
//! - **Host** (non-arm, behind the `hostmock` feature): `watch` and `movement`
//!   resolve via `#[path]` to the host-safe implementations under `src/host/`.
//!   They REUSE the real `movement/types.rs` and real `movement/simple_clock.rs`
//!   verbatim (no edits to the real face) and route the HAL free functions
//!   (`slcd::*`, `rtc::get_date_time`, `adc::get_vcc_voltage`,
//!   `gpio::get_button_level`) through the `Hw` seam to the reusable mock from
//!   `sensor_watch_core::mock_hw`. Register-heavy submodules (`slcd`, `rtc`,
//!   `adc`, `gpio`, ...) that only exist on ARM are provided as thin dispatch
//!   shims.
//!
//! The firmware entry point (`#[no_main]`, `cortex_m_rt::entry`) stays in the
//! binary `src/main.rs`; nothing here installs a main/entry, so this lib can link
//! into a host test harness.

#![no_std]
// Host + ARM both keep this lib target warning-free; the HAL exposes a broad API
// surface not all of which is reachable from the lib.
#![allow(dead_code)]
// The host seam holds a global `static mut *mut dyn Hw` pointer. See
// `src/host/watch/seam.rs`.
#![allow(static_mut_refs)]

// ---------------------------------------------------------------------------
// Firmware target: reuse the real, byte-identical module trees untouched.
// ---------------------------------------------------------------------------
#[cfg(target_arch = "arm")]
pub mod movement;
#[cfg(target_arch = "arm")]
pub mod watch;

// The ARM lib artifact is compiled by `-p sensor-watch --target thumbv6m` but
// never linked into the firmware binary (the bin's `src/panic.rs` provides the
// production `#[panic_handler]`). Provide a minimal handler so the lib target
// itself compiles. It is unreachable in a real firmware image.
#[cfg(target_arch = "arm")]
#[panic_handler]
fn arm_panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// ---------------------------------------------------------------------------
// Host target: `watch` and `movement` at the crate root (faces use
// `crate::watch::*` / `crate::movement::*`), pointing at the host seam versions.
// ---------------------------------------------------------------------------
#[cfg(all(not(target_arch = "arm"), feature = "hostmock"))]
#[path = "host/movement/mod.rs"]
pub mod movement;
#[cfg(all(not(target_arch = "arm"), feature = "hostmock"))]
#[path = "host/watch/mod.rs"]
pub mod watch;

// ---------------------------------------------------------------------------
// Host `no_std` runtime essentials.
//
// The lib is `#![no_std]`, so a host (non-arm) build needs a `#[panic_handler]`
// and a global allocator (the core crate uses `alloc`, e.g. `BTreeMap`/`String`
// in the mock). On the ARM firmware target these are provided by the binary
// (`src/panic.rs` + cortex-m) and this lib artifact is never linked; on host
// `cargo test`, the `#[test]` harness drives the crate under `std` (the
// `profile.test` target, which keeps panic=unwind), so this `no_std` runtime is
// excluded under `cfg(test)` to avoid conflicting with `std`'s own panic/alloc.
// ---------------------------------------------------------------------------
#[cfg(all(not(target_arch = "arm"), feature = "hostmock", not(test)))]
pub mod panic {
    use core::panic::PanicInfo;

    /// A minimal host panic handler for the `no_std` test/lib artifact.
    #[panic_handler]
    fn host_panic(_info: &PanicInfo) -> ! {
        loop {}
    }
}

#[cfg(all(not(target_arch = "arm"), feature = "hostmock", not(test)))]
mod host_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    /// A bump allocator over a static byte pool, sufficient for the mock's
    /// small heap use (`BTreeMap`/`String`) in host tests.
    struct BumpAllocator;

    const POOL_SIZE: usize = 16 * 1024;
    static mut POOL: [u8; POOL_SIZE] = [0; POOL_SIZE];
    static mut NEXT: usize = 0;

    unsafe impl GlobalAlloc for BumpAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();
            let start = unsafe { NEXT };
            let aligned = (start + align - 1) & !(align - 1);
            if aligned + size > unsafe { POOL.len() } {
                return core::ptr::null_mut();
            }
            unsafe {
                NEXT = aligned + size;
            }
            unsafe { POOL.as_mut_ptr().add(aligned) }
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
            // Bump allocator: no free.
        }
    }

    #[global_allocator]
    static ALLOCATOR_GLOBAL: BumpAllocator = BumpAllocator;
}
