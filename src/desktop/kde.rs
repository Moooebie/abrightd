//! KDE Plasma integration via PowerDevil.
//!
//! PowerDevil owns the brightness keys and the Plasma brightness UI, and
//! exposes a `0..brightnessMax` scale (10000 on the reference machine).  We use
//! it as an output path so the desktop UI stays in sync, and we listen to its
//! `brightnessChanged` signal to learn from user key presses.
//!
//! Writes use `setBrightnessSilent` so auto-brightness does not spam the OSD;
//! the monitor additionally ignores values matching our own last write.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::Proxy;

use super::Event;
use crate::output::sysfs::SysfsBacklight;
use crate::output::BacklightSink;

pub const SERVICE: &str = "org.kde.Solid.PowerManagement";
pub const BRIGHTNESS_PATH: &str = "/org/kde/Solid/PowerManagement/Actions/BrightnessControl";
pub const BRIGHTNESS_IFACE: &str = "org.kde.Solid.PowerManagement.Actions.BrightnessControl";

const DEFAULT_MAX: i32 = 10_000;

/// A [`BacklightSink`] that drives brightness through PowerDevil.
pub struct KdeSink {
    proxy: Proxy<'static>,
    max: i32,
    last_commanded: Arc<AtomicI32>,
}

impl KdeSink {
    pub async fn connect(last_commanded: Arc<AtomicI32>) -> anyhow::Result<Self> {
        let connection = zbus::Connection::session().await?;
        let proxy = Proxy::new(&connection, SERVICE, BRIGHTNESS_PATH, BRIGHTNESS_IFACE).await?;
        let max: i32 = proxy
            .call("brightnessMax", &())
            .await
            .unwrap_or(DEFAULT_MAX)
            .max(1);
        Ok(Self {
            proxy,
            max,
            last_commanded,
        })
    }

    pub fn max(&self) -> i32 {
        self.max
    }
}

#[async_trait]
impl BacklightSink for KdeSink {
    async fn set(&self, fraction: f32) -> anyhow::Result<()> {
        let value = (fraction.clamp(0.0, 1.0) * self.max as f32).round() as i32;
        self.last_commanded.store(value, Ordering::Relaxed);
        self.proxy
            .call_method("setBrightnessSilent", &(value,))
            .await?;
        Ok(())
    }

    async fn max_raw(&self) -> anyhow::Result<u32> {
        Ok(self.max as u32)
    }

    fn description(&self) -> String {
        format!("kde:PowerDevil (max={})", self.max)
    }
}

/// Watch PowerDevil's `brightnessChanged` and forward user changes as
/// [`Event::UserBrightness`].  Our own `setBrightnessSilent` writes are ignored
/// by comparing against `last_commanded`.
pub fn spawn_brightness_monitor(
    tx: tokio::sync::mpsc::Sender<Event>,
    last_commanded: Arc<AtomicI32>,
) {
    tokio::spawn(async move {
        if let Err(err) = brightness_monitor(tx, last_commanded).await {
            tracing::warn!("PowerDevil brightness monitor stopped: {err:#}");
        }
    });
}

async fn brightness_monitor(
    tx: tokio::sync::mpsc::Sender<Event>,
    last_commanded: Arc<AtomicI32>,
) -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(&connection, SERVICE, BRIGHTNESS_PATH, BRIGHTNESS_IFACE).await?;
    let max: i32 = proxy
        .call("brightnessMax", &())
        .await
        .unwrap_or(DEFAULT_MAX)
        .max(1);
    let mut stream = proxy.receive_signal("brightnessChanged").await?;

    while let Some(msg) = stream.next().await {
        let value: i32 = match msg.body().deserialize() {
            Ok(value) => value,
            Err(_) => continue,
        };
        // Ignore our own writes (and no-op changes).
        if (value - last_commanded.load(Ordering::Relaxed)).abs() <= 1 {
            continue;
        }
        let fraction = (value as f32 / max as f32).clamp(0.0, 1.0);
        if tx.send(Event::UserBrightness(fraction)).await.is_err() {
            break;
        }
    }
    Ok(())
}

/// A snapshot of PowerDevil/backlight state for `abrightd integrate detect`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Diagnostics {
    /// PowerDevil's brightness on its own `0..max` scale, if reachable.
    pub brightness: Option<i32>,
    pub max: Option<i32>,
    /// The actual sysfs backlight `(raw, max)`, if readable.
    pub sysfs: Option<(u32, u32)>,
    /// PowerDevil's ambient-light auto-brightness setting, if the key exists.
    pub ambient_auto_brightness: Option<bool>,
}

impl Diagnostics {
    /// Whether PowerDevil's view disagrees with the real panel by >2%.
    pub fn is_stale(&self) -> bool {
        match (self.brightness, self.max, self.sysfs) {
            (Some(b), Some(m), Some((raw, raw_max))) if m > 0 && raw_max > 0 => {
                let kde = b as f32 / m as f32;
                let sys = raw as f32 / raw_max as f32;
                (kde - sys).abs() > 0.02
            }
            _ => false,
        }
    }
}

/// Query PowerDevil and sysfs for diagnostics.  Never fails hard.
pub async fn diagnose() -> Diagnostics {
    let mut diag = Diagnostics::default();

    if let Ok(connection) = zbus::Connection::session().await {
        if let Ok(proxy) = Proxy::new(&connection, SERVICE, BRIGHTNESS_PATH, BRIGHTNESS_IFACE).await
        {
            diag.brightness = proxy.call("brightness", &()).await.ok();
            diag.max = proxy.call("brightnessMax", &()).await.ok();
        }
    }

    if let Ok(backlight) = SysfsBacklight::discover(None) {
        if let Ok(raw) = backlight.read_raw() {
            diag.sysfs = Some((raw, backlight.max_raw_sync()));
        }
    }

    diag.ambient_auto_brightness = read_ambient_auto_brightness();
    diag
}

/// Read PowerDevil's ambient-light auto-brightness flag from `powerdevilrc`.
///
/// Returns `None` when the key is absent, which (together with the absence of
/// the relevant code in the PowerDevil binaries) indicates the feature is not
/// available in this build.
fn read_ambient_auto_brightness() -> Option<bool> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".config/powerdevilrc");
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.to_ascii_lowercase().contains("ambient") {
                let value = value.trim().to_ascii_lowercase();
                return Some(value == "true" || value == "1");
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_detection() {
        let diag = Diagnostics {
            brightness: Some(3500),
            max: Some(10_000),
            sysfs: Some((2338, 15360)),
            ambient_auto_brightness: None,
        };
        assert!(diag.is_stale());

        let diag = Diagnostics {
            brightness: Some(5380),
            max: Some(10_000),
            sysfs: Some((8263, 15360)),
            ambient_auto_brightness: None,
        };
        assert!(!diag.is_stale());
    }
}
