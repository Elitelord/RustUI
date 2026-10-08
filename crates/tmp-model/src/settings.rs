//! Lite's settings. The fields and JSON are the same as `lite/settings.rs`, but
//! the file lives in the lab's own folder, so experiments never overwrite the
//! installed app's settings.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LiteSettings {
    /// Telemetry poll cadence, milliseconds.
    pub refresh_ms: u64,
    /// Which page opens on launch, as a [`Page::key`].
    pub default_page: String,
    /// Keep the window above others (applied at launch).
    pub always_on_top: bool,
}

impl Default for LiteSettings {
    fn default() -> Self {
        Self {
            refresh_ms: 1000,
            default_page: Page::Processes.key().into(),
            always_on_top: false,
        }
    }
}

/// The refresh choices on lite's Settings page: (label, milliseconds).
pub const REFRESH_CHOICES: [(&str, u64); 4] =
    [("0.5 s", 500), ("1 s", 1000), ("2 s", 2000), ("5 s", 5000)];

impl LiteSettings {
    /// `%LOCALAPPDATA%\RustUI-lab\lite-settings.json`
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(base)
                .join("RustUI-lab")
                .join("lite-settings.json"),
        )
    }

    /// Saved settings, or the defaults when the file is missing or unreadable.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn page(&self) -> Page {
        Page::from_key(&self.default_page)
    }
}

/// The four lite pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Processes,
    Performance,
    Battery,
    Settings,
}

impl Page {
    /// Sidebar order.
    pub const ALL: [Page; 4] = [
        Page::Processes,
        Page::Performance,
        Page::Battery,
        Page::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Processes => "Processes",
            Page::Performance => "Performance",
            Page::Battery => "Battery",
            Page::Settings => "Settings",
        }
    }

    /// The value stored in `default_page`.
    pub fn key(self) -> &'static str {
        match self {
            Page::Processes => "processes",
            Page::Performance => "performance",
            Page::Battery => "battery",
            Page::Settings => "settings",
        }
    }

    /// Unknown values open Processes, as lite does.
    pub fn from_key(key: &str) -> Page {
        match key {
            "performance" => Page::Performance,
            "battery" => Page::Battery,
            "settings" => Page::Settings,
            _ => Page::Processes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lites_json_and_fills_missing_fields() {
        let s: LiteSettings =
            serde_json::from_str(r#"{ "refresh_ms": 2000, "default_page": "battery" }"#).unwrap();
        assert_eq!(s.refresh_ms, 2000);
        assert_eq!(s.page(), Page::Battery);
        assert!(!s.always_on_top);

        let round: LiteSettings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round, s);
    }

    #[test]
    fn page_keys_round_trip() {
        for p in Page::ALL {
            assert_eq!(Page::from_key(p.key()), p);
        }
        assert_eq!(Page::from_key("storage"), Page::Processes);
    }
}
