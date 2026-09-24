//! `/sys/class/backlight/*/brightness` sink.
//!
//! Access is expected to be granted by the shipped udev rule (or by running
//! with the appropriate group), so this does not require root.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::BacklightSink;
use crate::brightness_to_raw;

const BACKLIGHT_CLASS: &str = "/sys/class/backlight";

pub struct SysfsBacklight {
    dir: PathBuf,
    name: String,
    max_raw: u32,
}

impl SysfsBacklight {
    /// `device` may be a name (`intel_backlight`), an absolute path, or `None`
    /// for auto-discovery of the first backlight device.
    pub fn discover(device: Option<&str>) -> anyhow::Result<Self> {
        let dir = match device {
            Some(d) if Path::new(d).is_absolute() => PathBuf::from(d),
            Some(name) => Path::new(BACKLIGHT_CLASS).join(name),
            None => find_device()?,
        };
        anyhow::ensure!(
            dir.is_dir(),
            "backlight device directory not found: {}",
            dir.display()
        );
        let max_raw: u32 = std::fs::read_to_string(dir.join("max_brightness"))?
            .trim()
            .parse()?;
        anyhow::ensure!(max_raw > 0, "max_brightness is zero");
        let name = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        Ok(Self { dir, name, max_raw })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The device's maximum raw brightness (sync accessor).
    pub fn max_raw_sync(&self) -> u32 {
        self.max_raw
    }

    /// Read the current raw brightness, e.g. to detect an external writer.
    pub fn read_raw(&self) -> anyhow::Result<u32> {
        let raw = std::fs::read_to_string(self.dir.join("brightness"))?;
        Ok(raw.trim().parse()?)
    }
}

#[async_trait]
impl BacklightSink for SysfsBacklight {
    async fn set(&self, fraction: f32) -> anyhow::Result<()> {
        let raw = brightness_to_raw(fraction, self.max_raw);
        let path = self.dir.join("brightness");
        tokio::fs::write(&path, format!("{raw}\n")).await?;
        Ok(())
    }

    async fn max_raw(&self) -> anyhow::Result<u32> {
        Ok(self.max_raw)
    }

    fn description(&self) -> String {
        format!("sysfs:{} (max={})", self.dir.display(), self.max_raw)
    }
}

fn find_device() -> anyhow::Result<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(BACKLIGHT_CLASS)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("brightness").exists() && p.join("max_brightness").exists())
        .collect();
    entries.sort();
    entries
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no backlight device found under {BACKLIGHT_CLASS}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_device(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("abrightd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn writes_scaled_raw_value() {
        let dir = fake_device("sysfs");
        std::fs::write(dir.join("max_brightness"), "1000\n").unwrap();
        std::fs::write(dir.join("brightness"), "0\n").unwrap();

        let sink = SysfsBacklight::discover(Some(dir.to_str().unwrap())).unwrap();
        assert_eq!(sink.max_raw().await.unwrap(), 1000);
        sink.set(0.5).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("brightness"))
                .unwrap()
                .trim(),
            "500"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
