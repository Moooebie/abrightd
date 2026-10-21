//! User-change detection via the sysfs backlight device.
//!
//! On KDE, PowerDevil tells us when the user touches the brightness.  Nothing
//! does elsewhere, so this module polls `/sys/class/backlight/<dev>/brightness`
//! and compares each reading with the raw value the daemon last wrote
//! (recorded by [`crate::output::tracking::TrackingSink`]).  A reading that
//! differs on two consecutive polls is forwarded as [`Event::UserBrightness`],
//! the same path the KDE monitor uses.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::Event;
use crate::output::sysfs::SysfsBacklight;
use crate::raw_to_brightness;

/// Readings within this many raw units of the commanded value are our own.
pub const RAW_TOLERANCE: u32 = 1;

/// Consecutive read failures tolerated before the monitor gives up.
const MAX_READ_FAILURES: u32 = 10;

/// Decides when a raw backlight reading is an external change.
///
/// A reading is external when it differs from the commanded value by more
/// than `tolerance` on two consecutive polls.  The second poll filters the
/// window between the daemon recording a new target and the write reaching
/// sysfs, where a single stale read would look like a user change.
#[derive(Debug, Default)]
pub struct ExternalChangeDetector {
    tolerance: u32,
    candidate: Option<u32>,
}

impl ExternalChangeDetector {
    pub fn new(tolerance: u32) -> Self {
        Self {
            tolerance,
            candidate: None,
        }
    }

    /// Feed one reading; returns the raw value to report as a user change, if
    /// any.  `last_commanded` is `None` until the daemon has written once.
    pub fn observe(&mut self, raw: u32, last_commanded: Option<u32>) -> Option<u32> {
        let external = last_commanded.is_some_and(|c| raw.abs_diff(c) > self.tolerance);
        if !external {
            self.candidate = None;
            return None;
        }
        if self.candidate.take() == Some(raw) {
            return Some(raw);
        }
        self.candidate = Some(raw);
        None
    }
}

/// Poll the backlight every `poll` and forward external changes as
/// [`Event::UserBrightness`].  Runs until the channel is closed.
pub fn spawn_backlight_monitor(
    tx: tokio::sync::mpsc::Sender<Event>,
    backlight: SysfsBacklight,
    last_commanded: Arc<AtomicI32>,
    poll: Duration,
) {
    tokio::spawn(async move {
        if let Err(err) = backlight_monitor(tx, backlight, last_commanded, poll).await {
            tracing::warn!("backlight monitor stopped: {err:#}");
        }
    });
}

async fn backlight_monitor(
    tx: tokio::sync::mpsc::Sender<Event>,
    backlight: SysfsBacklight,
    last_commanded: Arc<AtomicI32>,
    poll: Duration,
) -> anyhow::Result<()> {
    let max_raw = backlight.max_raw_sync().max(1);
    let backlight = Arc::new(backlight);
    let mut detector = ExternalChangeDetector::new(RAW_TOLERANCE);
    let mut failures = 0u32;

    loop {
        tokio::time::sleep(poll).await;

        // Blocking sysfs I/O must not run on the reactor.
        let device = backlight.clone();
        let raw = match tokio::task::spawn_blocking(move || device.read_raw()).await? {
            Ok(raw) => raw,
            Err(err) => {
                failures += 1;
                if failures >= MAX_READ_FAILURES {
                    return Err(err.context("reading backlight brightness"));
                }
                continue;
            }
        };
        failures = 0;

        let commanded = u32::try_from(last_commanded.load(Ordering::Relaxed)).ok();
        let Some(raw) = detector.observe(raw, commanded) else {
            continue;
        };
        // Claim the user's value so the next poll does not report it again.
        last_commanded.store(i32::try_from(raw).unwrap_or(i32::MAX), Ordering::Relaxed);
        let fraction = raw_to_brightness(raw, max_raw).clamp(0.0, 1.0);
        tracing::debug!("external backlight change: raw {raw} ({fraction:.4})");
        if tx.send(Event::UserBrightness(fraction)).await.is_err() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_reported_before_the_daemon_has_written() {
        let mut d = ExternalChangeDetector::new(1);
        assert_eq!(d.observe(300, None), None);
        assert_eq!(d.observe(300, None), None);
    }

    #[test]
    fn own_writes_are_ignored_within_tolerance() {
        let mut d = ExternalChangeDetector::new(1);
        assert_eq!(d.observe(200, Some(200)), None);
        assert_eq!(d.observe(201, Some(200)), None);
        assert_eq!(d.observe(199, Some(200)), None);
    }

    #[test]
    fn external_change_needs_two_consecutive_matching_reads() {
        let mut d = ExternalChangeDetector::new(1);
        assert_eq!(d.observe(240, Some(200)), None);
        assert_eq!(d.observe(240, Some(200)), Some(240));
    }

    #[test]
    fn a_single_stale_read_during_a_ramp_is_not_reported() {
        let mut d = ExternalChangeDetector::new(1);
        // Daemon claims 210, the write has not landed yet: one read of 200.
        assert_eq!(d.observe(200, Some(210)), None);
        // Next poll sees the write.
        assert_eq!(d.observe(210, Some(210)), None);
        // A later genuine change still needs two reads.
        assert_eq!(d.observe(300, Some(210)), None);
        assert_eq!(d.observe(300, Some(210)), Some(300));
    }

    #[test]
    fn a_changing_value_is_not_reported_until_it_settles() {
        let mut d = ExternalChangeDetector::new(1);
        // User holds the key: value keeps moving.
        assert_eq!(d.observe(220, Some(200)), None);
        assert_eq!(d.observe(240, Some(200)), None);
        assert_eq!(d.observe(260, Some(200)), None);
        // Released: settles at 260.
        assert_eq!(d.observe(260, Some(200)), Some(260));
    }

    #[test]
    fn once_claimed_the_value_is_quiet() {
        let mut d = ExternalChangeDetector::new(1);
        assert_eq!(d.observe(240, Some(200)), None);
        assert_eq!(d.observe(240, Some(200)), Some(240));
        // The monitor stores 240 as the new commanded value.
        assert_eq!(d.observe(240, Some(240)), None);
        assert_eq!(d.observe(240, Some(240)), None);
    }
}
