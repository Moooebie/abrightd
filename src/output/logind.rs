//! `org.freedesktop.login1` backlight sink.
//!
//! Uses `Session.SetBrightness("backlight", name, raw)` which is available to
//! the session owner without root.

use async_trait::async_trait;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Proxy};

use super::BacklightSink;
use crate::brightness_to_raw;

pub struct LogindBacklight {
    conn: Connection,
    session_path: OwnedObjectPath,
    name: String,
    max_raw: u32,
}

impl LogindBacklight {
    pub async fn connect(device: Option<&str>) -> anyhow::Result<Self> {
        let conn = Connection::system().await?;
        let manager = Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        let session_path: OwnedObjectPath = manager.call("GetSession", &("auto",)).await?;

        let backlight = crate::output::sysfs::SysfsBacklight::discover(device)?;
        let max_raw = backlight.max_raw().await?;
        Ok(Self {
            conn,
            session_path,
            name: backlight.name().to_string(),
            max_raw,
        })
    }

    async fn set_raw(&self, raw: u32) -> anyhow::Result<()> {
        let session = Proxy::new(
            &self.conn,
            "org.freedesktop.login1",
            self.session_path.as_str(),
            "org.freedesktop.login1.Session",
        )
        .await?;
        session
            .call_method("SetBrightness", &("backlight", self.name.as_str(), raw))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl BacklightSink for LogindBacklight {
    async fn set(&self, fraction: f32) -> anyhow::Result<()> {
        self.set_raw(brightness_to_raw(fraction, self.max_raw))
            .await
    }

    async fn max_raw(&self) -> anyhow::Result<u32> {
        Ok(self.max_raw)
    }

    fn description(&self) -> String {
        format!(
            "logind:{} (max={}, session={})",
            self.name,
            self.max_raw,
            self.session_path.as_str()
        )
    }
}
