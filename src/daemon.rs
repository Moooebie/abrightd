//! Daemon orchestration: wire an [`AlsSource`] through the controller and
//! [`Ramp`] into a [`BacklightSink`], and the deterministic replay harness.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use tracing::{debug, info};

use crate::als::{AlsSource, Sample};
use crate::clock::Clock;
use crate::controller::{AutomaticBrightnessController, ControllerOutput};
use crate::desktop::Event;
use crate::output::BacklightSink;
use crate::ramp::Ramp;
use crate::state::PersistedState;
use crate::status::{Command, SharedState};

/// One point of a replayed trace.
#[derive(Debug, Clone, Copy)]
pub struct ReplayPoint {
    pub time_ms: i64,
    pub lux: f32,
    pub controller_brightness: f32,
    pub output: f32,
}

/// Drives the pipeline.  Time is read from an injected [`Clock`] so the loop is
/// deterministic under test.
pub struct Daemon {
    controller: AutomaticBrightnessController,
    ramp: Ramp,
    ramp_min_interval_ms: i64,
    sink: Box<dyn BacklightSink>,
    clock: std::sync::Arc<dyn Clock>,
    target: f32,
    last_applied: f32,
    ramp_deadline: Option<i64>,
    shared: Option<Arc<Mutex<SharedState>>>,
    events: Option<tokio::sync::mpsc::Receiver<Event>>,
}

impl Daemon {
    pub fn new(
        controller: AutomaticBrightnessController,
        ramp: Ramp,
        ramp_min_interval_ms: i64,
        sink: Box<dyn BacklightSink>,
        clock: Arc<dyn Clock>,
        shared: Option<Arc<Mutex<SharedState>>>,
        events: Option<tokio::sync::mpsc::Receiver<Event>>,
    ) -> Self {
        Self {
            controller,
            ramp,
            ramp_min_interval_ms: ramp_min_interval_ms.max(1),
            sink,
            clock,
            target: f32::NAN,
            last_applied: f32::NAN,
            ramp_deadline: None,
            shared,
            events,
        }
    }

    pub fn controller(&self) -> &AutomaticBrightnessController {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut AutomaticBrightnessController {
        &mut self.controller
    }

    pub fn last_applied(&self) -> f32 {
        self.last_applied
    }

    /// Apply commands queued by the D-Bus service.
    pub fn apply_commands(&mut self) {
        let Some(shared) = self.shared.clone() else {
            return;
        };
        let commands: Vec<Command> = {
            let mut s = shared.lock().unwrap();
            s.commands.drain(..).collect()
        };
        let mut applied = false;
        let mut adjustment_changed = false;
        for command in commands {
            applied = true;
            match command {
                Command::Enable(enabled) => {
                    let now = self.clock.elapsed_realtime_ms();
                    self.controller.set_light_sensor_enabled(enabled, now);
                }
                Command::AddUserPoint { lux, brightness } => {
                    self.controller
                        .set_screen_brightness_by_user_at(lux, brightness);
                }
                Command::ClearUserPoints => self.controller.reset_short_term_model(),
                Command::SetProfile(name) => info!("profile change requested: {name}"),
                Command::SetAdjustment(value) => {
                    self.controller
                        .mapper_mut()
                        .set_auto_brightness_adjustment(value);
                    adjustment_changed = true;
                }
            }
        }
        // A user-initiated change must take effect immediately, mirroring
        // AOSP's `configure(..., userChangedBrightness, ...)` behaviour.
        if applied {
            let now = self.clock.elapsed_realtime_ms();
            if let Some(b) = self.controller.refresh_after_user_change(now).brightness {
                self.target = b.clamp(0.0, 1.0);
            }
        }
        if adjustment_changed {
            let state = PersistedState {
                schema_version: PersistedState::SCHEMA_VERSION,
                adjustment: self.controller.get_auto_brightness_adjustment(),
            };
            if let Err(err) = state.save() {
                tracing::warn!("could not persist calibration state: {err:#}");
            }
        }
    }

    fn update_shared(&self) {
        let Some(shared) = &self.shared else {
            return;
        };
        if let Ok(mut s) = shared.lock() {
            s.lux = self.controller.ambient_lux();
            s.last_observed_lux = self.controller.last_observed_lux();
            s.slow_lux = self.controller.slow_ambient_lux();
            s.fast_lux = self.controller.fast_ambient_lux();
            s.ambient_brightening_threshold = self.controller.ambient_brightening_threshold();
            s.ambient_darkening_threshold = self.controller.ambient_darkening_threshold();
            s.screen_brightening_threshold = self.controller.screen_brightening_threshold();
            s.screen_darkening_threshold = self.controller.screen_darkening_threshold();
            s.controller_brightness = self.controller.get_automatic_screen_brightness();
            s.output_brightness = self.last_applied;
            s.adjustment = self.controller.get_auto_brightness_adjustment();
            if self.controller.has_user_data_points() {
                s.user_points = vec![(
                    self.controller.mapper().get_user_lux(),
                    self.controller.mapper().get_user_brightness(),
                )];
            } else {
                s.user_points.clear();
            }
        }
    }

    /// Apply an event from the desktop/session.
    async fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        let now = self.clock.elapsed_realtime_ms();
        match event {
            Event::UserBrightness(fraction) => {
                if !self.controller.light_sensor_enabled() {
                    return Ok(());
                }
                if self.controller.has_valid_ambient_lux() {
                    let lux = self.controller.ambient_lux();
                    self.controller
                        .set_screen_brightness_by_user_at(lux, fraction);
                    // AOSP samples the event after a debounce; the actual user
                    // point is applied immediately above.
                    self.controller.prepare_brightness_adjustment_sample(now);
                    if let Some(b) = self.controller.refresh_after_user_change(now).brightness {
                        self.target = b.clamp(0.0, 1.0);
                    }
                    info!(
                        "user brightness change: {:.1}% at {:.2} lx",
                        fraction * 100.0,
                        lux
                    );
                    self.ramp.reset(fraction.clamp(0.0, 1.0));
                    self.last_applied = fraction.clamp(0.0, 1.0);
                    self.update_shared();
                }
            }
            Event::Locked(locked) => {
                info!("session {}", if locked { "locked" } else { "unlocked" });
                self.controller.set_light_sensor_enabled(!locked, now);
            }
            Event::Suspend(preparing) => {
                info!(
                    "system {}",
                    if preparing { "suspending" } else { "resuming" }
                );
                self.controller.set_light_sensor_enabled(!preparing, now);
            }
        }
        Ok(())
    }

    /// Run until the input stream ends or Ctrl-C is received.
    pub async fn run(&mut self, als: &mut dyn AlsSource) -> anyhow::Result<()> {
        let now = self.clock.elapsed_realtime_ms();
        self.controller.set_light_sensor_enabled(true, now);
        info!("ambient source: {}", als.description());
        info!("backlight sink: {}", self.sink.description());

        let mut ctrl_c = Box::pin(tokio::signal::ctrl_c());
        let mut events = self.events.take();

        loop {
            self.apply_commands();
            let deadline = match (self.controller.next_wakeup_ms(), self.ramp_deadline) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            let clock = self.clock.clone();
            let sleep = async move {
                match deadline {
                    Some(t) => {
                        let delay = (t - clock.elapsed_realtime_ms()).max(0) as u64;
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                    None => std::future::pending::<()>().await,
                }
            };

            tokio::select! {
                _ = &mut ctrl_c => {
                    info!("received Ctrl-C, shutting down");
                    break;
                }
                event = recv_event(&mut events) => {
                    self.handle_event(event).await?;
                }
                sample = als.next() => {
                    let Some(sample) = sample? else {
                        info!("ambient source ended");
                        break;
                    };
                    let out = self.controller.handle_light_sensor_event(sample.time_ms, sample.lux);
                    self.advance(sample.time_ms, out).await?;
                }
                _ = sleep => {
                    let now = self.clock.elapsed_realtime_ms();
                    let out = self.controller.on_timer(now);
                    self.advance(now, out).await?;
                }
            }
        }
        Ok(())
    }

    async fn advance(&mut self, now_ms: i64, out: ControllerOutput) -> anyhow::Result<()> {
        if let Some(b) = out.brightness {
            self.target = b.clamp(0.0, 1.0);
        }
        if self.target.is_nan() {
            return Ok(());
        }
        let ramped = self.ramp.update(now_ms, self.target);
        if !float_equals(ramped, self.last_applied) {
            self.sink.set(ramped).await.context("setting backlight")?;
            self.last_applied = ramped;
            debug!("brightness -> {ramped:.4} (controller {:.4})", self.target);
        }
        self.ramp_deadline = if (self.ramp.current() - self.target).abs() > 1e-6 {
            Some(now_ms + self.ramp_min_interval_ms)
        } else {
            None
        };
        self.update_shared();
        Ok(())
    }
}

/// Deterministic, synchronous replay used by `abrightd --replay`.
///
/// Interleaves sensor samples with the controller's scheduled wake-ups exactly
/// as the real handler loop would.
pub fn replay(
    controller: &mut AutomaticBrightnessController,
    ramp: &mut Ramp,
    samples: &[Sample],
) -> Vec<ReplayPoint> {
    let mut points = Vec::new();
    if samples.is_empty() {
        return points;
    }
    controller.set_light_sensor_enabled(true, samples[0].time_ms);
    let last_sample_time = samples[samples.len() - 1].time_ms;
    let mut idx = 0usize;
    let mut target = f32::NAN;
    let mut last_output = f32::NAN;
    let mut last_now = i64::MIN;
    let max_steps = samples.len() * 1000 + 1000;

    for _ in 0..max_steps {
        let next_sample = samples.get(idx).map(|s| s.time_ms);
        let next_timer = controller.next_wakeup_ms();
        let now = match (next_sample, next_timer) {
            (Some(s), Some(t)) => s.min(t),
            (Some(s), None) => s,
            (None, Some(t)) => t,
            (None, None) => break,
        };
        // Once the trace is exhausted, let the controller settle for a couple of
        // minutes before stopping.
        if next_sample.is_none() && now > last_sample_time + 120_000 {
            break;
        }
        let take_sample = match (next_sample, next_timer) {
            (Some(s), Some(t)) => s <= t,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        let out = if take_sample {
            let s = samples[idx];
            idx += 1;
            controller.handle_light_sensor_event(s.time_ms, s.lux)
        } else {
            controller.on_timer(now)
        };

        if let Some(b) = out.brightness {
            target = b;
        }
        let output = if target.is_nan() {
            f32::NAN
        } else {
            let ramped = ramp.update(now, target);
            last_output = ramped;
            ramped
        };
        let lux = samples[idx.saturating_sub(1)].lux;
        points.push(ReplayPoint {
            time_ms: now,
            lux,
            controller_brightness: controller.get_automatic_screen_brightness(),
            output,
        });

        if now <= last_now && !take_sample {
            // Defensive: a timer fired without advancing time.
            break;
        }
        last_now = now;
    }

    let _ = last_output;
    points
}

/// Await the next desktop event, or never complete when there is no channel.
async fn recv_event(events: &mut Option<tokio::sync::mpsc::Receiver<Event>>) -> Event {
    match events {
        Some(rx) => match rx.recv().await {
            Some(event) => event,
            None => std::future::pending::<Event>().await,
        },
        None => std::future::pending::<Event>().await,
    }
}

fn float_equals(a: f32, b: f32) -> bool {
    a == b || (a - b).abs() <= f32::EPSILON * a.abs().max(b.abs()).max(1.0)
}
