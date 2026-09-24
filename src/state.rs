//! Persisted calibration / learning state.
//!
//! Kept deliberately small for now: the only persisted value is the AOSP
//! auto-brightness adjustment (a global gamma applied to the whole curve).
//! Later tiers will extend this with sensor-calibration knots and a learned
//! correction curve.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// On-disk state.  Unknown/missing fields fall back to defaults so old state
/// files keep loading after upgrades.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub schema_version: u32,
    /// Global auto-brightness adjustment in `[-1, 1]` (AOSP semantics).
    pub adjustment: f32,
}

impl PersistedState {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Load state from disk, returning defaults if absent or malformed.
    pub fn load() -> Self {
        match std::fs::read_to_string(state_path()) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Atomically write state to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let dir = state_dir();
        std::fs::create_dir_all(&dir)?;
        let text = toml::to_string_pretty(self)?;
        let tmp = dir.join("state.toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, dir.join("state.toml"))?;
        Ok(())
    }

    pub fn adjustment_clamped(&self) -> f32 {
        self.adjustment.clamp(-1.0, 1.0)
    }
}

/// Resolve the state directory.
///
/// Prefers `$STATE_DIRECTORY` (set by systemd `StateDirectory=`), then
/// `$XDG_STATE_HOME`, then `$HOME/.local/state`.
pub fn state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("STATE_DIRECTORY") {
        // systemd may provide a colon-separated list; take the first entry.
        let value = dir.to_string_lossy();
        if let Some(first) = value.split(':').find(|s| !s.is_empty()) {
            return PathBuf::from(first);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(xdg).join("abrightd");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/state/abrightd")
}

pub fn state_path() -> PathBuf {
    state_dir().join("state.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let state = PersistedState {
            schema_version: PersistedState::SCHEMA_VERSION,
            adjustment: 0.25,
        };
        let text = toml::to_string_pretty(&state).unwrap();
        let back: PersistedState = toml::from_str(&text).unwrap();
        assert!((back.adjustment - 0.25).abs() < 1e-6);
    }

    #[test]
    fn clamps_adjustment() {
        let state = PersistedState {
            schema_version: 1,
            adjustment: 5.0,
        };
        assert_eq!(state.adjustment_clamped(), 1.0);
    }
}
