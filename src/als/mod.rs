//! Ambient-light sources.
//!
//! [`AlsSource`] is the input boundary that keeps the controller testable.

pub mod iio;
pub mod replay;

use async_trait::async_trait;

/// One ambient-light reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Timestamp in milliseconds on the sensor clock (`CLOCK_BOOTTIME`).
    pub time_ms: i64,
    pub lux: f32,
}

/// A stream of ambient-light samples.
#[async_trait]
pub trait AlsSource: Send {
    /// Yield the next sample, or `None` at end of stream (replay only).
    async fn next(&mut self) -> anyhow::Result<Option<Sample>>;

    /// Human-readable description for logs / status.
    fn description(&self) -> String;
}
