//! Persisted calibration / learning state.
//!
//! Stores the *net effect* of calibration: the AOSP global adjustment plus the
//! user data point(s).  On startup these are re-applied to reproduce the same
//! curve (the point is inserted directly, without re-inferring the adjustment).
//!
//! Unknown/missing fields fall back to defaults so old state files keep loading
//! after upgrades.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A learned/calibrated user control point (lux, normalized brightness).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPoint {
    pub lux: f32,
    pub brightness: f32,
}

/// On-disk state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub schema_version: u32,
    /// Global auto-brightness adjustment in `[-1, 1]` (AOSP semantics).
    pub adjustment: f32,
    /// User control points.  `SimpleMappingStrategy` holds at most one today;
    /// the list is forward-looking (Tier 4 history).
    pub user_points: Vec<UserPoint>,
}

impl PersistedState {
    pub const SCHEMA_VERSION: u32 = 2;

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

    /// The single user point, if any (AOSP holds at most one).
    pub fn single_user_point(&self) -> Option<UserPoint> {
        self.user_points.first().copied()
    }

    /// An empty, uncalibrated state.
    pub fn cleared() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            adjustment: 0.0,
            user_points: Vec::new(),
        }
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
    fn round_trips_with_points() {
        let state = PersistedState {
            schema_version: PersistedState::SCHEMA_VERSION,
            adjustment: 0.25,
            user_points: vec![UserPoint {
                lux: 40.0,
                brightness: 0.2,
            }],
        };
        let text = toml::to_string_pretty(&state).unwrap();
        let back: PersistedState = toml::from_str(&text).unwrap();
        assert!((back.adjustment - 0.25).abs() < 1e-6);
        assert_eq!(back.single_user_point().unwrap().lux, 40.0);
        assert_eq!(back.single_user_point().unwrap().brightness, 0.2);
    }

    #[test]
    fn old_state_without_points_loads() {
        // v1 file: only adjustment.
        let back: PersistedState =
            toml::from_str("schema_version = 1\nadjustment = 0.5\n").unwrap();
        assert!((back.adjustment - 0.5).abs() < 1e-6);
        assert!(back.user_points.is_empty());
    }

    #[test]
    fn clamps_adjustment() {
        let state = PersistedState {
            adjustment: 5.0,
            ..PersistedState::default()
        };
        assert_eq!(state.adjustment_clamped(), 1.0);
    }

    #[test]
    fn cleared_is_uncalibrated() {
        let state = PersistedState::cleared();
        assert_eq!(state.adjustment, 0.0);
        assert!(state.user_points.is_empty());
    }
}
