//! IIO sysfs ambient-light source.
//!
//! Discovers a device exposing `in_illuminance0_input`, `in_illuminance_input`
//! or `in_illuminance_raw` under `/sys/bus/iio/devices`, applies
//! `in_illuminance_scale` / `_offset` and an optional calibration multiplier,
//! and polls at the controller-selected rate.
//!
//! Blocking sysfs reads are moved off the async reactor with `spawn_blocking`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use super::{AlsSource, Sample};
use crate::clock::Clock;

const IIO_DEVICES: &str = "/sys/bus/iio/devices";
const INPUT_NAMES: [&str; 3] = [
    "in_illuminance0_input",
    "in_illuminance_input",
    "in_illuminance_raw",
];

pub struct IioSysfs {
    device_dir: PathBuf,
    input: PathBuf,
    scale: f32,
    offset: f32,
    lux_multiplier: f32,
    poll_rate_ms: i64,
    clock: std::sync::Arc<dyn Clock>,
}

/// A discovered IIO ambient-light device.
#[derive(Debug, Clone)]
pub struct IioDevice {
    /// Device directory name, e.g. `iio:device0`.
    pub name: String,
    pub device_dir: PathBuf,
    pub input: PathBuf,
    pub scale: f32,
    pub offset: f32,
}

impl IioDevice {
    /// Read and scale one lux sample (blocking).
    pub fn read_lux(&self) -> anyhow::Result<f32> {
        let raw: f32 = std::fs::read_to_string(&self.input)?.trim().parse()?;
        Ok(raw * self.scale + self.offset)
    }
}

/// List every IIO device that exposes an illuminance channel, sorted by name.
///
/// Used by `abrightd init` to offer the user a choice of sensors.
pub fn list_illuminance_devices() -> anyhow::Result<Vec<IioDevice>> {
    let mut devices = Vec::new();
    for entry in std::fs::read_dir(IIO_DEVICES)? {
        let entry = entry?;
        if let Some(device) = device_from_dir(&entry.path()) {
            devices.push(device);
        }
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

fn device_from_dir(dir: &Path) -> Option<IioDevice> {
    if !dir.is_dir() {
        return None;
    }
    let input = INPUT_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())?;
    Some(IioDevice {
        name: dir.file_name()?.to_string_lossy().into_owned(),
        device_dir: dir.to_path_buf(),
        input,
        scale: read_attr_f32(&dir.join("in_illuminance_scale")).unwrap_or(1.0),
        offset: read_attr_f32(&dir.join("in_illuminance_offset")).unwrap_or(0.0),
    })
}

impl IioSysfs {
    /// Find an IIO ambient-light device.  `device` may be a name
    /// (`iio:device0`), an absolute path, or `None` for auto-discovery.
    pub fn discover(
        device: Option<&str>,
        poll_rate_ms: i64,
        clock: std::sync::Arc<dyn Clock>,
    ) -> anyhow::Result<Self> {
        let device_dir = match device {
            Some(d) if Path::new(d).is_absolute() => PathBuf::from(d),
            Some(name) => Path::new(IIO_DEVICES).join(name),
            None => find_device()?,
        };

        anyhow::ensure!(
            device_dir.is_dir(),
            "IIO device directory not found: {}",
            device_dir.display()
        );

        let input = INPUT_NAMES
            .iter()
            .map(|n| device_dir.join(n))
            .find(|p| p.exists())
            .ok_or_else(|| {
                anyhow::anyhow!("no illuminance input under {}", device_dir.display())
            })?;

        let scale = read_attr_f32(&device_dir.join("in_illuminance_scale")).unwrap_or(1.0);
        let offset = read_attr_f32(&device_dir.join("in_illuminance_offset")).unwrap_or(0.0);

        Ok(Self {
            device_dir,
            input,
            scale,
            offset,
            lux_multiplier: 1.0,
            poll_rate_ms: poll_rate_ms.max(1),
            clock,
        })
    }

    /// Apply a per-device calibration multiplier to the computed lux.
    pub fn with_lux_multiplier(mut self, multiplier: f32) -> Self {
        self.lux_multiplier = multiplier;
        self
    }
}

#[async_trait]
impl AlsSource for IioSysfs {
    async fn next(&mut self) -> anyhow::Result<Option<Sample>> {
        tokio::time::sleep(Duration::from_millis(self.poll_rate_ms as u64)).await;
        let input = self.input.clone();
        let scale = self.scale;
        let offset = self.offset;
        let multiplier = self.lux_multiplier;
        // Blocking file I/O must not run on the reactor.
        let lux = tokio::task::spawn_blocking(move || -> anyhow::Result<f32> {
            let raw: f32 = std::fs::read_to_string(&input)?.trim().parse()?;
            Ok((raw * scale + offset) * multiplier)
        })
        .await??;
        Ok(Some(Sample {
            time_ms: self.clock.elapsed_realtime_ms(),
            lux,
        }))
    }

    fn description(&self) -> String {
        format!(
            "iio:{} (scale={}, offset={})",
            self.device_dir.display(),
            self.scale,
            self.offset
        )
    }
}

fn find_device() -> anyhow::Result<PathBuf> {
    list_illuminance_devices()?
        .into_iter()
        .next()
        .map(|device| device.device_dir)
        .ok_or_else(|| anyhow::anyhow!("no IIO device with an illuminance channel found"))
}

fn read_attr_f32(path: &Path) -> Option<f32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use std::sync::Arc;

    #[tokio::test]
    async fn reads_scaled_lux() {
        let dir = std::env::temp_dir().join(format!("abrightd-iio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("in_illuminance0_input"), "123\n").unwrap();
        std::fs::write(dir.join("in_illuminance_scale"), "2.0\n").unwrap();

        let clock: Arc<dyn Clock> = Arc::new(TestClock::new(4242));
        let mut src = IioSysfs::discover(Some(dir.to_str().unwrap()), 1, clock).unwrap();
        let sample = src.next().await.unwrap().unwrap();
        assert!((sample.lux - 246.0).abs() < 1e-3, "got {}", sample.lux);
        assert_eq!(sample.time_ms, 4242);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
