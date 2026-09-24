//! `org.abrightd` D-Bus control/status service.
//!
//! Enabled with the `dbus` feature.  The service methods only queue commands or
//! read the shared snapshot; the daemon loop owns the controller and applies
//! queued commands between steps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::connection::Builder;
use zbus::interface;

use crate::status::{Command, SharedState};

pub struct Abrightd {
    state: Arc<Mutex<SharedState>>,
}

impl Abrightd {
    fn push(&self, command: Command) {
        if let Ok(mut state) = self.state.lock() {
            state.commands.push_back(command);
        }
    }
}

#[interface(name = "org.abrightd")]
impl Abrightd {
    /// Enable or disable automatic brightness.
    async fn enable(&self, enabled: bool) {
        self.push(Command::Enable(enabled));
    }

    /// Select a named profile (applied by the daemon on next reload).
    async fn set_profile(&self, name: String) {
        if let Ok(mut state) = self.state.lock() {
            state.profile = name.clone();
        }
        self.push(Command::SetProfile(name));
    }

    /// Add a learned user data point.
    async fn add_user_point(&self, lux: f64, brightness: f64) {
        self.push(Command::AddUserPoint {
            lux: lux as f32,
            brightness: brightness as f32,
        });
    }

    /// Clear all learned user data points.
    async fn clear_user_points(&self) {
        self.push(Command::ClearUserPoints);
    }

    /// Set the global auto-brightness adjustment (`[-1, 1]`).
    async fn set_adjustment(&self, value: f64) {
        self.push(Command::SetAdjustment(value as f32));
    }

    /// Return the current pipeline snapshot.
    async fn status(&self) -> HashMap<String, String> {
        self.state.lock().map(|s| s.as_map()).unwrap_or_default()
    }
}

/// Publish the service on the session bus under `org.abrightd`.
pub async fn serve(state: Arc<Mutex<SharedState>>) -> anyhow::Result<zbus::Connection> {
    let connection = Builder::session()?
        .name("org.abrightd")?
        .serve_at("/org/abrightd", Abrightd { state })?
        .build()
        .await?;
    Ok(connection)
}

/// Fetch the running daemon's status snapshot, if it is on the session bus.
pub async fn fetch_status() -> anyhow::Result<HashMap<String, String>> {
    let connection = zbus::Connection::session().await?;
    let proxy =
        zbus::Proxy::new(&connection, "org.abrightd", "/org/abrightd", "org.abrightd").await?;
    let status: HashMap<String, String> = proxy.call("Status", &()).await?;
    Ok(status)
}

/// Ask the running daemon to set the global adjustment.
pub async fn set_adjustment(value: f32) -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy =
        zbus::Proxy::new(&connection, "org.abrightd", "/org/abrightd", "org.abrightd").await?;
    proxy.call_method("SetAdjustment", &(value as f64)).await?;
    Ok(())
}
