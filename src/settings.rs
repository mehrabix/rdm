//! User preferences, persisted to `~/.config/rdm/config.toml`.
//!
//! One struct owns every user-facing default so the rest of the app never
//! hard-codes a theme, language, folder, connection count, or queue.

use std::path::PathBuf;
use std::sync::OnceLock;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl Theme {
    /// i18n key for the theme's display name.
    pub fn label_key(self) -> &'static str {
        match self {
            Theme::Light => "theme.light",
            Theme::Dark => "theme.dark",
            Theme::System => "theme.system",
        }
    }

    /// Value handed to the CSS/JS layer.
    pub fn as_js(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }
}

/// A named queue (IDM-style): downloads assigned to it share a concurrency cap.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueDef {
    pub name: String,
    /// Concurrent downloads allowed in this queue.
    pub limit: u8,
}

impl Default for QueueDef {
    fn default() -> Self {
        Self {
            name: "Main".to_string(),
            limit: 3,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub language: String,
    pub download_dir: PathBuf,
    pub connections: u8,
    pub max_concurrent: u8,
    pub start_minimized: bool,
    pub confirm_exit: bool,
    pub launch_on_complete: bool,
    /// 0 = unlimited.
    pub speed_limit_kbps: u32,
    /// Put downloads into `Compressed/`, `Video/`, … subfolders.
    pub organize_by_category: bool,
    /// Only transfer inside a daily window (IDM's scheduler).
    pub schedule_enabled: bool,
    pub schedule_start: String,
    pub schedule_stop: String,
    /// Named queues and their concurrency limits.
    pub queues: Vec<QueueDef>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            language: "en".to_string(),
            download_dir: rdm_core::util::default_download_dir(),
            connections: 8,
            max_concurrent: 3,
            start_minimized: false,
            confirm_exit: true,
            launch_on_complete: false,
            speed_limit_kbps: 0,
            organize_by_category: false,
            schedule_enabled: false,
            schedule_start: "01:00".to_string(),
            schedule_stop: "07:00".to_string(),
            queues: vec![QueueDef::default()],
        }
    }
}

impl Settings {
    /// The names of the configured queues, in order.
    pub fn queue_names(&self) -> Vec<String> {
        self.queues.iter().map(|q| q.name.clone()).collect()
    }

    /// Queue name -> concurrency limit, for the engine.
    pub fn queue_limits(&self) -> std::collections::HashMap<String, usize> {
        self.queues
            .iter()
            .map(|q| (q.name.clone(), q.limit.max(1) as usize))
            .collect()
    }
}

/// `$XDG_CONFIG_HOME/rdm/config.toml`, else `~/.config/rdm/config.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("rdm").join("config.toml")
}

/// Read settings from disk. Any error (missing, unreadable, malformed) yields
/// defaults — the app must always be able to start.
pub fn load() -> Settings {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string_pretty(settings) {
        let _ = std::fs::write(path, text);
    }
}

/// Settings loaded once at process start. `state::engine()` reads these when it
/// builds the engine, so the boot settings must exist before the first task.
static BOOT: OnceLock<Settings> = OnceLock::new();

pub fn boot() -> &'static Settings {
    BOOT.get_or_init(load)
}

/// Reactive handle to the settings, shared through Dioxus context. Writes are
/// persisted immediately.
#[derive(Clone, Copy, PartialEq)]
pub struct SettingsCtx {
    pub settings: Signal<Settings>,
}

impl SettingsCtx {
    pub fn get(&self) -> Settings {
        self.settings.read().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) {
        let mut signal = self.settings;
        signal.with_mut(f);
        save(&self.get());
    }
}

/// Context lookup for components.
pub fn use_settings() -> SettingsCtx {
    use_context::<SettingsCtx>()
}

/// Whether `now` (minutes since midnight) is inside the scheduled window.
/// Handles windows that wrap past midnight.
pub fn in_schedule(settings: &Settings, now_minutes: u32) -> bool {
    if !settings.schedule_enabled {
        return true;
    }
    let parse = |s: &str| -> u32 {
        let mut parts = s.split(':');
        let h: u32 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let m: u32 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        (h.min(23)) * 60 + m.min(59)
    };
    let start = parse(&settings.schedule_start);
    let stop = parse(&settings.schedule_stop);
    if start <= stop {
        now_minutes >= start && now_minutes < stop
    } else {
        now_minutes >= start || now_minutes < stop
    }
}

/// Current local time as minutes since midnight, via `date` (no extra crate).
pub fn now_minutes() -> u32 {
    let output = std::process::Command::new("date").arg("+%H %M").output();
    if let Ok(output) = output {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut parts = text.split_whitespace();
        let h: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let m: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        return h * 60 + m;
    }
    0
}
