//! Marking approvals that deserve a second look.
//!
//! Every row currently looks the same, so `git push --force` reads exactly
//! like `ls`. These rules give the loud ones a colour, an icon and a label,
//! and are editable because what counts as risky is per-person.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    /// Irreversible or outward-facing. Red.
    Danger,
    /// Worth reading before approving. Amber.
    Caution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskRule {
    /// Case-insensitive substring matched against `<tool>:<target>`.
    pub pattern: String,
    pub level: Level,
    pub icon: String,
    /// Free text the user typed. Empty on the seeded rules, which are named
    /// by `key` instead so they can be shown in either language.
    #[serde(default)]
    pub label: String,
    /// Translation key for a seeded rule. Absent on user-created ones.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// What the UI paints on a row. Carries both, and the frontend prefers the
/// translated `key` when there is one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskMark {
    pub level: Level,
    pub icon: String,
    pub label: String,
    pub key: Option<String>,
}

/// Seeded on first run. Deliberately conservative: these are the operations
/// that are hard or impossible to walk back, not merely unusual ones.
fn defaults() -> Vec<RiskRule> {
    let rule = |pattern: &str, level: Level, icon: &str, key: &str| RiskRule {
        pattern: pattern.to_string(),
        level,
        icon: icon.to_string(),
        label: String::new(),
        key: Some(key.to_string()),
        enabled: true,
    };
    vec![
        rule("push --force", Level::Danger, "⚠", "forcePush"),
        rule("push -f", Level::Danger, "⚠", "forcePush"),
        rule("git push", Level::Danger, "↑", "push"),
        rule("reset --hard", Level::Danger, "⚠", "historyLoss"),
        rule("rm -rf", Level::Danger, "⚠", "recursiveDelete"),
        rule("remove-item -recurse", Level::Danger, "⚠", "recursiveDelete"),
        rule("drop table", Level::Danger, "⚠", "dropTable"),
        rule("npm publish", Level::Danger, "📦", "publish"),
        rule("cargo publish", Level::Danger, "📦", "publish"),
        rule("gh release create", Level::Danger, "📦", "release"),
        rule("deploy", Level::Danger, "🚀", "deploy"),
        rule("terraform apply", Level::Danger, "🚀", "infraChange"),
        rule("git commit", Level::Caution, "●", "commit"),
        rule("curl", Level::Caution, "🌐", "network"),
        rule("Invoke-WebRequest", Level::Caution, "🌐", "network"),
    ]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskRules {
    #[serde(default)]
    rules: Vec<RiskRule>,
}

impl Default for RiskRules {
    fn default() -> Self {
        RiskRules { rules: defaults() }
    }
}

impl RiskRules {
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

    /// The strongest match wins, so a `git push --force` is flagged as a
    /// force push rather than as an ordinary push.
    pub fn evaluate(&self, haystack: &str) -> Option<RiskMark> {
        let hay = haystack.to_lowercase();
        let mut best: Option<&RiskRule> = None;
        for rule in self.rules.iter().filter(|r| r.enabled) {
            if rule.pattern.trim().is_empty() || !hay.contains(&rule.pattern.to_lowercase()) {
                continue;
            }
            let better = match best {
                None => true,
                Some(current) => {
                    (rule.level == Level::Danger && current.level == Level::Caution)
                        || (rule.level == current.level
                            && rule.pattern.len() > current.pattern.len())
                }
            };
            if better {
                best = Some(rule);
            }
        }
        best.map(|r| RiskMark {
            level: r.level,
            icon: r.icon.clone(),
            label: r.label.clone(),
            key: r.key.clone(),
        })
    }

    pub fn list(&self) -> &[RiskRule] {
        &self.rules
    }

    pub fn replace(&mut self, rules: Vec<RiskRule>) {
        self.rules = rules;
    }

    pub fn restore_defaults(&mut self) {
        self.rules = defaults();
    }
}

pub fn risk_path(config_dir: &Path) -> PathBuf {
    config_dir.join("risk.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_force_push_beats_the_plain_push_rule() {
        let rules = RiskRules::default();
        let mark = rules.evaluate("Bash:git push --force origin main").unwrap();
        assert_eq!(mark.level, Level::Danger);
        assert_eq!(mark.key.as_deref(), Some("forcePush"));
    }

    #[test]
    fn danger_outranks_caution_regardless_of_order() {
        let rules = RiskRules::default();
        let mark = rules.evaluate("Bash:git commit -m x && git push").unwrap();
        assert_eq!(mark.level, Level::Danger);
    }

    #[test]
    fn ordinary_calls_are_not_marked() {
        let rules = RiskRules::default();
        assert!(rules.evaluate("Bash:ls -la").is_none());
        assert!(rules.evaluate("Read:src/main.rs").is_none());
    }

    #[test]
    fn matching_ignores_case_and_disabled_rules_are_skipped() {
        let mut rules = RiskRules::default();
        assert!(rules.evaluate("Bash:GIT PUSH origin").is_some());

        let disabled: Vec<RiskRule> = rules
            .list()
            .iter()
            .cloned()
            .map(|mut r| {
                r.enabled = false;
                r
            })
            .collect();
        rules.replace(disabled);
        assert!(rules.evaluate("Bash:git push origin").is_none());
    }
}
