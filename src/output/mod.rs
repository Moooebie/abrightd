//! Backlight outputs.

#[cfg(feature = "dbus")]
pub mod logind;
pub mod sysfs;

use async_trait::async_trait;

/// The output boundary: something that can accept a normalized brightness.
#[async_trait]
pub trait BacklightSink: Send + Sync {
    /// Command a normalized brightness in `[0, 1]`.
    async fn set(&self, fraction: f32) -> anyhow::Result<()>;
    /// The device's maximum raw backlight value.
    async fn max_raw(&self) -> anyhow::Result<u32>;
    /// Human-readable description for logs / status.
    fn description(&self) -> String;
}
