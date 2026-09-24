//! Desktop-environment integration.
//!
//! The DE owns the user's brightness keys and (on GNOME) its own ALS
//! auto-brightness.  This module detects the desktop and bridges its events
//! into the daemon as [`Event`]s: user brightness changes and lock/suspend
//! transitions.  The D-Bus monitors are gated behind the `dbus` feature; the
//! types themselves are always available.

/// Which desktop session we are running under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desktop {
    Kde,
    Gnome,
    Other,
}

impl Desktop {
    /// Detect from `XDG_CURRENT_DESKTOP` / `DESKTOP_SESSION`.
    pub fn detect() -> Self {
        let haystack = format!(
            "{} {}",
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            std::env::var("DESKTOP_SESSION").unwrap_or_default()
        )
        .to_ascii_lowercase();
        if haystack.contains("kde") || haystack.contains("plasma") {
            Desktop::Kde
        } else if haystack.contains("gnome") || haystack.contains("ubuntu") {
            Desktop::Gnome
        } else {
            Desktop::Other
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Desktop::Kde => "KDE Plasma",
            Desktop::Gnome => "GNOME",
            Desktop::Other => "other",
        }
    }
}

/// An event from the desktop/session to apply to the controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// The user changed the screen brightness (normalized `[0, 1]`).
    UserBrightness(f32),
    /// The session was locked (`true`) or unlocked (`false`).
    Locked(bool),
    /// The machine is about to suspend (`true`) or just resumed (`false`).
    Suspend(bool),
}

#[cfg(feature = "dbus")]
pub mod kde;

/// Watch the logind session for lock/unlock and suspend/resume and forward
/// them as [`Event`]s.  Runs until the channel is closed.
#[cfg(feature = "dbus")]
pub fn spawn_session_monitor(tx: tokio::sync::mpsc::Sender<Event>) {
    tokio::spawn(async move {
        if let Err(err) = session_monitor(tx).await {
            tracing::warn!("session monitor stopped: {err:#}");
        }
    });
}

#[cfg(feature = "dbus")]
async fn session_monitor(tx: tokio::sync::mpsc::Sender<Event>) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use zbus::zvariant::OwnedObjectPath;

    let conn = zbus::Connection::system().await?;
    let manager = zbus::Proxy::new(
        &conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let session_path: OwnedObjectPath = manager.call("GetSession", &("auto",)).await?;
    let session = zbus::Proxy::new(
        &conn,
        "org.freedesktop.login1",
        session_path.as_str(),
        "org.freedesktop.login1.Session",
    )
    .await?;

    let mut sleep = manager.receive_signal("PrepareForSleep").await?;
    let mut lock = session.receive_signal("Lock").await?;
    let mut unlock = session.receive_signal("Unlock").await?;

    loop {
        tokio::select! {
            Some(msg) = sleep.next() => {
                let preparing: bool = msg.body().deserialize().unwrap_or(false);
                let _ = tx.send(Event::Suspend(preparing)).await;
            }
            Some(_) = lock.next() => {
                let _ = tx.send(Event::Locked(true)).await;
            }
            Some(_) = unlock.next() => {
                let _ = tx.send(Event::Locked(false)).await;
            }
            else => break,
        }
    }
    Ok(())
}
