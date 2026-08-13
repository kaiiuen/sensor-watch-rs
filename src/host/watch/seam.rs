//! Scoped host hardware access for the `watch` HAL.
//!
//! Host HAL functions run inside [`with_hw`]. The active backend is visible only
//! while that closure is running, and each HAL call borrows it through
//! [`with_current_hw`]. This keeps the synchronization held for the complete
//! lifetime of every backend borrow without exposing a lifetime-erased reference.

#[cfg(not(test))]
use core::sync::atomic::{AtomicBool, Ordering};
use sensor_watch_core::mock_hw::Hw;

#[cfg(test)]
use core::cell::Cell;

#[derive(Clone, Copy)]
struct DispatchPtr(*mut dyn Hw);

// The pointer is used only while the scoped dispatch lock is held. The backend
// itself is still borrowed by `with_hw` for the duration of that scope.
unsafe impl Send for DispatchPtr {}

#[cfg(not(test))]
static mut DISPATCH: Option<DispatchPtr> = None;
#[cfg(not(test))]
static DISPATCH_LOCKED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
std::thread_local! {
    static TEST_DISPATCH: Cell<Option<DispatchPtr>> = const { Cell::new(None) };
}

#[cfg(not(test))]
fn lock_dispatch() {
    while DISPATCH_LOCKED
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
fn unlock_dispatch() {
    DISPATCH_LOCKED.store(false, Ordering::Release);
}

struct DispatchScope {
    #[cfg(not(test))]
    locked: bool,
}

union PointerParts<'a> {
    reference: *mut (dyn Hw + 'a),
    parts: (*mut (), *mut ()),
}

union RebuildPointer {
    parts: (*mut (), *mut ()),
    erased: *mut (dyn Hw + 'static),
}

fn erase_pointer(hw: &mut dyn Hw) -> *mut (dyn Hw + 'static) {
    // SAFETY: these operations copy only the two fat-pointer words. The
    // resulting pointer is installed and dereferenced exclusively inside the
    // borrow scope owned by the caller; no reference lifetime is exposed.
    unsafe {
        let parts = PointerParts::<'_> { reference: hw }.parts;
        RebuildPointer { parts }.erased
    }
}

impl DispatchScope {
    fn enter(hw: &mut dyn Hw) -> Self {
        #[cfg(test)]
        TEST_DISPATCH.with(|slot| slot.set(Some(DispatchPtr(erase_pointer(hw)))));
        #[cfg(not(test))]
        {
            lock_dispatch();
            unsafe {
                // The slot stores only the address while this scope is active;
                // the borrow is kept alive by `DispatchScope` and cannot escape
                // `with_hw`. A union is used only to carry the raw fat pointer
                // into the scoped slot; no reference lifetime is exposed.
                DISPATCH = Some(DispatchPtr(erase_pointer(hw)));
            }
        }
        Self {
            #[cfg(not(test))]
            locked: true,
        }
    }
}

impl Drop for DispatchScope {
    fn drop(&mut self) {
        #[cfg(test)]
        TEST_DISPATCH.with(|slot| slot.set(None));
        #[cfg(not(test))]
        {
            unsafe {
                DISPATCH = None;
            }
            if self.locked {
                unlock_dispatch();
                self.locked = false;
            }
        }
    }
}

/// Runs `f` with `hw` installed for exactly the duration of the closure.
///
/// The backend may not be used by host HAL calls outside this scope. Nested
/// scopes are intentionally unsupported because a single firmware HAL has one
/// active backend at a time.
pub fn with_hw<R>(hw: &mut dyn Hw, f: impl FnOnce() -> R) -> R {
    let _scope = DispatchScope::enter(hw);
    f()
}

/// Borrows the active backend for one host HAL operation.
///
/// This is private to the host HAL seam: callers must use [`with_hw`] so the
/// dispatch lock outlives the returned borrow and is released even on panic.
#[inline]
pub(crate) fn with_current_hw<R>(f: impl FnOnce(&mut dyn Hw) -> R) -> R {
    #[cfg(test)]
    let ptr = TEST_DISPATCH.with(|slot| match slot.get() {
        Some(DispatchPtr(ptr)) => ptr,
        None => panic!("host watch: no Hw installed; call sensor_watch::watch::seam::with_hw"),
    });
    #[cfg(not(test))]
    let ptr = unsafe {
        match DISPATCH {
            Some(DispatchPtr(ptr)) => ptr,
            None => panic!("host watch: no Hw installed; call sensor_watch::watch::seam::with_hw"),
        }
    };

    // SAFETY: `with_hw` installs this pointer only for the duration of its
    // closure, while its dispatch scope holds the synchronization. Every host
    // HAL caller reaches this function inside that scope.
    unsafe { f(&mut *ptr) }
}
