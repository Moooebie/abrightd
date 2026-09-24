//! CSV replay source: `timestamp_ms,lux` per line.
//!
//! Used by the deterministic replay harness (`abrightd --replay`) and tests.
//! Lines starting with `#` and a leading `timestamp_ms,lux` header are ignored.

use std::collections::VecDeque;
use std::path::Path;

use async_trait::async_trait;

use super::{AlsSource, Sample};

#[derive(Debug, Clone, Default)]
pub struct Replay {
    samples: VecDeque<Sample>,
}

impl Replay {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_csv(&text)
    }

    pub fn from_csv(text: &str) -> anyhow::Result<Self> {
        let mut samples = VecDeque::new();
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split(',');
            let time = parts.next().unwrap_or("").trim();
            let lux = parts.next().unwrap_or("").trim();
            let (Ok(time_ms), Ok(lux)) = (time.parse::<i64>(), lux.parse::<f32>()) else {
                // Tolerate a header line anywhere in the file.
                if time.eq_ignore_ascii_case("timestamp_ms") {
                    continue;
                }
                anyhow::bail!("invalid replay line {}: {raw:?}", lineno + 1);
            };
            samples.push_back(Sample { time_ms, lux });
        }
        Ok(Self { samples })
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Drain all samples synchronously (used by the replay harness).
    pub fn into_vec(self) -> Vec<Sample> {
        self.samples.into_iter().collect()
    }
}

#[async_trait]
impl AlsSource for Replay {
    async fn next(&mut self) -> anyhow::Result<Option<Sample>> {
        Ok(self.samples.pop_front())
    }

    fn description(&self) -> String {
        format!("replay ({} samples)", self.samples.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_with_comments_and_header() {
        let r =
            Replay::from_csv("# a comment\ntimestamp_ms,lux\n0,10\n200,20.5\n400,30\n").unwrap();
        assert_eq!(r.len(), 3);
        let v = r.into_vec();
        assert_eq!(
            v[1],
            Sample {
                time_ms: 200,
                lux: 20.5
            }
        );
    }

    #[test]
    fn rejects_bad_line() {
        assert!(Replay::from_csv("0,10\nnot,a,line\n").is_err());
    }
}
