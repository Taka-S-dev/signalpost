//! User preferences for how loudly the panel behaves.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Audio cue when a row arrives.
    pub sound: bool,
    /// Brief highlight on arrival and on clearing the last row — the visible
    /// acknowledgement that replaces the panel vanishing.
    pub flash: bool,
    /// Hide the panel once nothing is left. Off by default: a window that
    /// disappears and reappears on its own reads as flicker.
    pub auto_hide: bool,
    /// Background opacity, 0.4–1.0. Text stays fully opaque either way.
    pub opacity: f64,
    /// `auto` follows the OS language; `ja` and `en` pin it.
    #[serde(default = "auto_lang")]
    pub lang: Lang,
    /// Colour for the panel's outline. Empty keeps the subtle default, which
    /// disappears against a dark background once the panel is translucent.
    #[serde(default)]
    pub border: String,
    /// Which arrivals are worth putting the panel in front of what you are
    /// doing.
    #[serde(default = "default_popup")]
    pub popup: PopupWhen,
    /// System notification for arrivals that do not raise the panel, so they
    /// are not silent as well as invisible.
    #[serde(default = "yes")]
    pub toast: bool,
    /// Expand the bar by pointing at it, and collapse again on the way out.
    #[serde(default = "yes")]
    pub hover_expand: bool,
    /// Keep the full list on screen instead of letting it collapse to the
    /// bar when the queue empties.
    #[serde(default)]
    pub keep_open: bool,
    /// Global shortcut that shows the panel, e.g. `Alt+Space`.
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
}

/// `Alt+Space` is the obvious choice and also what PowerToys' Command Palette
/// takes by default, so it cannot be assumed to be free.
pub const DEFAULT_SHORTCUT: &str = "Alt+Space";

/// Tried in order when the configured one is unavailable.
pub const FALLBACK_SHORTCUTS: [&str; 3] = ["Ctrl+Alt+Space", "Ctrl+Shift+Space", "Alt+Q"];

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT.to_string()
}

fn yes() -> bool {
    true
}

/// Not every row deserves the same interruption. A blocked call has something
/// waiting on it; a finished turn is only news.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PopupWhen {
    /// Blocked calls only.
    Permission,
    /// Anything that arrives, including finished turns.
    All,
    /// Never; the tray count and the sound still report.
    Never,
}

fn default_popup() -> PopupWhen {
    PopupWhen::Permission
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lang {
    Auto,
    Ja,
    En,
}

fn auto_lang() -> Lang {
    Lang::Auto
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sound: true,
            flash: true,
            auto_hide: false,
            opacity: 1.0,
            lang: Lang::Auto,
            border: String::new(),
            popup: PopupWhen::Permission,
            toast: true,
            hover_expand: true,
            keep_open: false,
            shortcut: DEFAULT_SHORTCUT.to_string(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, raw);
        }
    }

    /// Keeps the panel usable no matter what a hand-edited file contains.
    pub fn sanitized(mut self) -> Self {
        self.opacity = self.opacity.clamp(0.4, 1.0);
        self
    }
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}
