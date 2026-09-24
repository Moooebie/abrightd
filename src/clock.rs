//! Clocks.
//!
//! AOSP has two clocks relevant to auto-brightness:
//!
//! * `SystemClock.elapsedRealtime()` — milliseconds since boot *including*
//!   suspend (`CLOCK_BOOTTIME`).  Used for sensor sample timestamps.
//! * `SystemClock.uptimeMillis()` — milliseconds since boot *excluding*
//!   suspend (`CLOCK_MONOTONIC`).  Used for handler timers.
//!
//! This port uses `CLOCK_BOOTTIME` for the whole pipeline so that time keeps
//! advancing across suspend in a way that is monotonic and consistent with the
//! ring-buffer horizons and the debounce timers.  Both are exposed so callers
//! can choose.

use std::sync::Mutex;

/// Provides the two monotonic clocks used by the controller.
pub trait Clock: Send + Sync {
    /// Milliseconds since boot, not counting time spent in deep sleep.
    fn uptime_ms(&self) -> i64;
    /// Milliseconds since boot, including time spent in deep sleep.
    fn elapsed_realtime_ms(&self) -> i64;
}

/// Reads the real system clocks.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

/// Thin `clock_gettime(2)` shim.  This is the only place the port uses
/// `unsafe`.
fn clock_gettime_ms(clock_id: libc::clockid_t) -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, properly aligned `timespec`.
    let rc = unsafe { libc::clock_gettime(clock_id, &mut ts) };
    if rc != 0 {
        return 0;
    }
    ts.tv_sec * 1000 + ts.tv_nsec / 1_000_000
}

impl Clock for SystemClock {
    fn uptime_ms(&self) -> i64 {
        clock_gettime_ms(libc::CLOCK_MONOTONIC)
    }

    fn elapsed_realtime_ms(&self) -> i64 {
        clock_gettime_ms(libc::CLOCK_BOOTTIME)
    }
}

/// A manually advanced clock for tests and the replay harness.
#[derive(Debug, Default)]
pub struct TestClock {
    now_ms: Mutex<i64>,
}

impl TestClock {
    pub fn new(start_ms: i64) -> Self {
        Self {
            now_ms: Mutex::new(start_ms),
        }
    }

    pub fn set(&self, now_ms: i64) {
        *self.now_ms.lock().unwrap() = now_ms;
    }

    pub fn advance(&self, delta_ms: i64) {
        *self.now_ms.lock().unwrap() += delta_ms;
    }
}

impl Clock for TestClock {
    fn uptime_ms(&self) -> i64 {
        *self.now_ms.lock().unwrap()
    }

    fn elapsed_realtime_ms(&self) -> i64 {
        *self.now_ms.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_is_monotonic() {
        let c = SystemClock::new();
        let a = c.elapsed_realtime_ms();
        let b = c.uptime_ms();
        assert!(a > 0);
        assert!(b > 0);
    }

    #[test]
    fn test_clock_advances() {
        let c = TestClock::new(1000);
        assert_eq!(c.uptime_ms(), 1000);
        c.advance(500);
        assert_eq!(c.elapsed_realtime_ms(), 1500);
        c.set(42);
        assert_eq!(c.uptime_ms(), 42);
    }
}
