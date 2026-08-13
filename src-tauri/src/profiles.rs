//! Per-project identity: the colour and name a session is recognised by.
//!
//! Keyed on `cwd` rather than session id, because that is what survives a
//! restart and what maps to an editor window.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::model::{now_ms, project_name};

/// Distinguishable at a glance on the dark panel, and still distinct for the
/// most common forms of colour blindness.
///
/// Muted on purpose. A project's colour only has to say *which* session a row
/// belongs to; at the saturation these were, a stripe naming a project pulled
/// harder than the amber that means something is waiting. None of these may
/// be mistaken for that amber, the red, or the green.
const PALETTE: [&str; 8] = [
    "#8ea3c4", // slate
    "#8fb392", // sage
    "#c9ad72", // sand
    "#c68d8d", // clay
    "#a798c4", // lilac
    "#79a8ad", // teal
    "#c4926a", // ochre
    "#a8b06e", // olive
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub cwd: String,
    /// Overridden display name; falls back to the folder name.
    #[serde(default)]
    pub name: Option<String>,
    /// Overridden colour; falls back to one derived from the path.
    #[serde(default)]
    pub color: Option<String>,
    /// What `Enter` runs to bring this project's window forward. `{cwd}` is
    /// substituted. Sessions run from a terminal need something other than the
    /// editor default, and nothing in the hook payload says which is which.
    #[serde(default)]
    pub open_command: Option<String>,
    #[serde(default)]
    pub last_seen: u64,
}

/// Reuses the window already holding the folder, so this focuses rather than
/// opening a second one.
pub const DEFAULT_OPEN_COMMAND: &str = "code -r \"{cwd}\"";

/// What the UI actually renders for a project.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedProfile {
    pub cwd: String,
    pub label: String,
    pub color: String,
    /// Empty when the project uses the default, so the field can show a
    /// placeholder rather than pretend the default was chosen.
    pub open_command: String,
    /// True when the user set these by hand, so the editor can offer a reset.
    pub customized: bool,
    pub last_seen: u64,
}

fn normalize(cwd: &str) -> String {
    cwd.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// Stable across restarts, unlike a hash whose seed changes per process.
fn auto_color(cwd: &str) -> &'static str {
    let sum = normalize(cwd)
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(sum as usize) % PALETTE.len()]
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Profiles {
    #[serde(default)]
    projects: Vec<Profile>,
}

impl Profiles {
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

    fn find(&self, cwd: &str) -> Option<&Profile> {
        let key = normalize(cwd);
        self.projects.iter().find(|p| normalize(&p.cwd) == key)
    }

    pub fn label(&self, cwd: &str) -> String {
        self.find(cwd)
            .and_then(|p| p.name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| project_name(cwd))
    }

    pub fn color(&self, cwd: &str) -> String {
        self.find(cwd)
            .and_then(|p| p.color.clone())
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| auto_color(cwd).to_string())
    }

    /// The command `Enter` should run for this project, with `{cwd}` already
    /// substituted.
    pub fn open_command(&self, cwd: &str) -> String {
        let template = self
            .find(cwd)
            .and_then(|p| p.open_command.clone())
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OPEN_COMMAND.to_string());
        template.replace("{cwd}", cwd)
    }

    pub fn has_open_command(&self, cwd: &str) -> bool {
        self.find(cwd)
            .and_then(|p| p.open_command.as_ref())
            .is_some_and(|c| !c.trim().is_empty())
    }

    /// Records that a project was seen. Returns true when something changed,
    /// so callers only write to disk when there is a reason to.
    pub fn touch(&mut self, cwd: &str) -> bool {
        if cwd.is_empty() {
            return false;
        }
        let key = normalize(cwd);
        if let Some(existing) = self.projects.iter_mut().find(|p| normalize(&p.cwd) == key) {
            existing.last_seen = now_ms();
            return true;
        }
        self.projects.push(Profile {
            cwd: cwd.to_string(),
            name: None,
            color: None,
            open_command: None,
            last_seen: now_ms(),
        });
        true
    }

    /// Empty strings clear the override and restore the derived value.
    pub fn set(
        &mut self,
        cwd: &str,
        name: Option<String>,
        color: Option<String>,
        open_command: Option<String>,
    ) {
        let clean = |v: Option<String>| {
            v.filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
        };
        let key = normalize(cwd);
        match self.projects.iter_mut().find(|p| normalize(&p.cwd) == key) {
            Some(existing) => {
                existing.name = clean(name);
                existing.color = clean(color);
                existing.open_command = clean(open_command);
            }
            None => self.projects.push(Profile {
                cwd: cwd.to_string(),
                name: clean(name),
                color: clean(color),
                open_command: clean(open_command),
                last_seen: now_ms(),
            }),
        }
    }

    /// Most recently active first — the projects being worked on right now are
    /// the ones worth naming.
    pub fn resolved(&self) -> Vec<ResolvedProfile> {
        let mut list: Vec<ResolvedProfile> = self
            .projects
            .iter()
            .map(|p| ResolvedProfile {
                label: self.label(&p.cwd),
                color: self.color(&p.cwd),
                open_command: p.open_command.clone().unwrap_or_default(),
                customized: p.name.is_some() || p.color.is_some() || p.open_command.is_some(),
                cwd: p.cwd.clone(),
                last_seen: p.last_seen,
            })
            .collect();
        list.sort_by_key(|p| std::cmp::Reverse(p.last_seen));
        list
    }

    pub fn palette() -> Vec<String> {
        PALETTE.iter().map(|c| c.to_string()).collect()
    }
}

pub fn profiles_path(config_dir: &Path) -> PathBuf {
    config_dir.join("projects.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_is_stable_and_ignores_path_spelling() {
        assert_eq!(auto_color("C:/work/app"), auto_color("c:\\work\\app\\"));
    }

    #[test]
    fn overrides_replace_the_derived_values_and_clear_back() {
        let mut profiles = Profiles::default();
        profiles.touch("C:/work/app");
        assert_eq!(profiles.label("C:/work/app"), "app");

        profiles.set(
            "C:/work/app",
            Some("prod-api".into()),
            Some("#ffffff".into()),
            None,
        );
        assert_eq!(profiles.label("c:\\work\\app"), "prod-api");
        assert_eq!(profiles.color("c:\\work\\app"), "#ffffff");

        profiles.set("C:/work/app", Some("  ".into()), None, None);
        assert_eq!(profiles.label("C:/work/app"), "app");
        assert_eq!(profiles.color("C:/work/app"), auto_color("C:/work/app"));
    }

    #[test]
    fn open_command_defaults_to_the_editor_and_substitutes_the_path() {
        let mut profiles = Profiles::default();
        profiles.touch("C:/work/app");
        assert_eq!(
            profiles.open_command("C:/work/app"),
            "code -r \"C:/work/app\""
        );

        profiles.set("C:/work/app", None, None, Some("wt -d \"{cwd}\"".into()));
        assert_eq!(
            profiles.open_command("C:/work/app"),
            "wt -d \"C:/work/app\""
        );

        profiles.set("C:/work/app", None, None, Some(String::new()));
        assert_eq!(
            profiles.open_command("C:/work/app"),
            "code -r \"C:/work/app\""
        );
    }

    #[test]
    fn touching_a_known_project_does_not_duplicate_it() {
        let mut profiles = Profiles::default();
        profiles.touch("C:/work/app");
        profiles.touch("c:\\work\\app");
        assert_eq!(profiles.resolved().len(), 1);
    }
}
