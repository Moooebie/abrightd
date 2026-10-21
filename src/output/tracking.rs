//! A [`BacklightSink`] wrapper that remembers the last raw value it commanded.
//!
//! The user-change monitors in [`crate::desktop`] compare backlight readings
//! against this value to tell the daemon's own writes from the user's.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use super::BacklightSink;
use crate::brightness_to_raw;

pub struct TrackingSink {
    inner: Box<dyn BacklightSink>,
    last_commanded: Arc<AtomicI32>,
}

impl TrackingSink {
    /// Wrap `inner`; every `set` records the raw value in `last_commanded`
    /// (`-1` means nothing has been commanded yet).
    pub fn new(inner: Box<dyn BacklightSink>, last_commanded: Arc<AtomicI32>) -> Self {
        Self {
            inner,
            last_commanded,
        }
    }
}

#[async_trait]
impl BacklightSink for TrackingSink {
    async fn set(&self, fraction: f32) -> anyhow::Result<()> {
        let max_raw = self.inner.max_raw().await?;
        let raw = brightness_to_raw(fraction, max_raw);
        // Record before writing so a poll between the two sees our own value.
        self.last_commanded
            .store(i32::try_from(raw).unwrap_or(i32::MAX), Ordering::Relaxed);
        self.inner.set(fraction).await
    }

    async fn max_raw(&self) -> anyhow::Result<u32> {
        self.inner.max_raw().await
    }

    fn description(&self) -> String {
        self.inner.description()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingSink {
        max: u32,
        writes: Arc<Mutex<Vec<f32>>>,
    }

    #[async_trait]
    impl BacklightSink for RecordingSink {
        async fn set(&self, fraction: f32) -> anyhow::Result<()> {
            self.writes.lock().unwrap().push(fraction);
            Ok(())
        }

        async fn max_raw(&self) -> anyhow::Result<u32> {
            Ok(self.max)
        }

        fn description(&self) -> String {
            "recording".into()
        }
    }

    #[tokio::test]
    async fn records_raw_value_and_forwards_the_write() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let last = Arc::new(AtomicI32::new(-1));
        let sink = TrackingSink::new(
            Box::new(RecordingSink {
                max: 1000,
                writes: writes.clone(),
            }),
            last.clone(),
        );

        assert_eq!(last.load(Ordering::Relaxed), -1);
        sink.set(0.5).await.unwrap();
        assert_eq!(last.load(Ordering::Relaxed), 500);
        assert_eq!(*writes.lock().unwrap(), vec![0.5]);

        sink.set(1.0).await.unwrap();
        assert_eq!(last.load(Ordering::Relaxed), 1000);
        assert_eq!(sink.max_raw().await.unwrap(), 1000);
    }
}
