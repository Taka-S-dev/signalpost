//! Auto-allow rules.
//!
//! Every rule the user creates from the inbox is persisted here, so a call
//! that has already been approved once never reaches the queue again. This is
//! the mechanism that makes the inbox quieter the longer it runs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::model::Item;

/// How wide an "always allow" click should reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// This exact call, in this project only.
    ExactCall,
    /// Any call of this tool, in this project only.
    ToolInProject,
    /// Any call of this tool, anywhere.
    ToolEverywhere,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub tool_name: String,
    /// `None` matches any call of the tool.
    pub signature: Option<String>,
    /// `None` matches any project.
    pub cwd: Option<String>,
    pub created_at: u64,
    /// How many calls this rule has answered without asking. A rule is only
    /// worth keeping if it earns its place, and a rule firing far more often
    /// than expected is the first sign it was scoped too wide.
    #[serde(default)]
    pub hits: u32,
    /// `None` until it fires for the first time.
    #[serde(default)]
    pub last_hit_at: Option<u64>,
}

impl Rule {
    pub fn from_item(item: &Item, scope: Scope) -> Self {
        let (signature, cwd) = match scope {
            Scope::ExactCall => (Some(item.signature.clone()), Some(item.cwd.clone())),
            Scope::ToolInProject => (None, Some(item.cwd.clone())),
            Scope::ToolEverywhere => (None, None),
        };
        Rule {
            tool_name: item.tool_name.clone(),
            signature,
            cwd,
            created_at: crate::model::now_ms(),
            hits: 0,
            last_hit_at: None,
        }
    }

    fn matches(&self, item: &Item) -> bool {
        if self.tool_name != item.tool_name {
            return false;
        }
        if let Some(sig) = &self.signature {
            if sig != &item.signature {
                return false;
            }
        }
        if let Some(cwd) = &self.cwd {
            if !same_path(cwd, &item.cwd) {
                return false;
            }
        }
        true
    }

    /// The pieces the UI needs to describe this rule.
    ///
    /// Deliberately not a finished sentence: word order and wording differ per
    /// language, so the phrasing belongs to the frontend.
    pub fn view(&self) -> RuleView {
        RuleView {
            tool_name: self.tool_name.clone(),
            signature: self.signature.clone(),
            project: self.cwd.as_deref().map(crate::model::project_name),
            hits: self.hits,
            last_hit_at: self.last_hit_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleView {
    pub tool_name: String,
    /// `None` means the rule covers every call of the tool.
    pub signature: Option<String>,
    /// `None` means the rule applies in every project.
    pub project: Option<String>,
    pub hits: u32,
    pub last_hit_at: Option<u64>,
}

/// Windows paths differ in separator and case between hook payloads and the
/// values we stored, so compare them leniently.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Rules {
    #[serde(default)]
    rules: Vec<Rule>,
}

impl Rules {
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

    /// Answers whether a rule covers this call, and records the fact on the
    /// rule that did. Counting here rather than at the call site means a hit
    /// can never be missed by a caller that forgets to report it.
    pub fn allows(&mut self, item: &Item) -> bool {
        let Some(rule) = self.rules.iter_mut().find(|r| r.matches(item)) else {
            return false;
        };
        rule.hits += 1;
        rule.last_hit_at = Some(crate::model::now_ms());
        true
    }

    /// Returns whether a rule was actually added, so the UI only offers to
    /// undo something that happened.
    pub fn add(&mut self, rule: Rule) -> bool {
        let duplicate = self
            .rules
            .iter()
            .any(|r| r.tool_name == rule.tool_name && r.signature == rule.signature && r.cwd == rule.cwd);
        if duplicate {
            return false;
        }
        self.rules.push(rule);
        true
    }

    /// Removes the most recently added rule.
    pub fn undo_last(&mut self) -> bool {
        self.rules.pop().is_some()
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.rules.len() {
            self.rules.remove(index);
        }
    }

    pub fn list(&self) -> &[Rule] {
        &self.rules
    }
}

pub fn rules_path(config_dir: &Path) -> PathBuf {
    config_dir.join("auto-allow.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ItemKind;

    fn item(tool: &str, sig: &str, cwd: &str) -> Item {
        Item {
            id: "1".into(),
            kind: ItemKind::Permission,
            agent: crate::model::Agent::Claude,
            session_id: "s".into(),
            cwd: cwd.into(),
            project: crate::model::project_name(cwd),
            label: crate::model::project_name(cwd),
            color: "#7aa2f7".into(),
            tool_name: tool.into(),
            summary: String::new(),
            detail: None,
            detail_kind: "text".into(),
            risk: None,
            repeat: 1,
            signature: sig.into(),
            created_at: 0,
        }
    }

    #[test]
    fn exact_call_rule_is_scoped_to_project_and_signature() {
        let mut rules = Rules::default();
        rules.add(Rule::from_item(
            &item("Bash", "Bash:npm test", "C:/work/app"),
            Scope::ExactCall,
        ));

        assert!(rules.allows(&item("Bash", "Bash:npm test", "C:/work/app")));
        assert!(!rules.allows(&item("Bash", "Bash:npm run build", "C:/work/app")));
        assert!(!rules.allows(&item("Bash", "Bash:npm test", "C:/work/other")));
    }

    #[test]
    fn tool_in_project_rule_ignores_signature_but_not_project() {
        let mut rules = Rules::default();
        rules.add(Rule::from_item(
            &item("Read", "Read:a.rs", "C:/work/app"),
            Scope::ToolInProject,
        ));

        assert!(rules.allows(&item("Read", "Read:b.rs", "C:/work/app")));
        assert!(!rules.allows(&item("Read", "Read:b.rs", "C:/work/other")));
        assert!(!rules.allows(&item("Write", "Write:b.rs", "C:/work/app")));
    }

    #[test]
    fn project_match_tolerates_separator_and_case_differences() {
        let mut rules = Rules::default();
        rules.add(Rule::from_item(
            &item("Read", "Read:a.rs", "C:/work/App"),
            Scope::ToolInProject,
        ));

        assert!(rules.allows(&item("Read", "Read:a.rs", "c:\\work\\app")));
    }

    #[test]
    fn each_silent_approval_is_counted_against_the_rule_that_made_it() {
        let mut rules = Rules::default();
        let src = item("Read", "Read:a.rs", "C:/work/app");
        rules.add(Rule::from_item(&src, Scope::ToolInProject));

        assert_eq!(rules.list()[0].hits, 0);
        assert!(rules.allows(&item("Read", "Read:a.rs", "C:/work/app")));
        assert!(rules.allows(&item("Read", "Read:b.rs", "C:/work/app")));
        assert!(!rules.allows(&item("Read", "Read:b.rs", "C:/work/other")));

        assert_eq!(rules.list()[0].hits, 2);
        assert!(rules.list()[0].last_hit_at.is_some());
    }

    #[test]
    fn only_the_first_matching_rule_takes_the_credit() {
        let mut rules = Rules::default();
        let src = item("Read", "Read:a.rs", "C:/work/app");
        rules.add(Rule::from_item(&src, Scope::ExactCall));
        rules.add(Rule::from_item(&src, Scope::ToolInProject));

        assert!(rules.allows(&src));
        assert_eq!(rules.list()[0].hits, 1);
        assert_eq!(rules.list()[1].hits, 0);
    }

    #[test]
    fn adding_the_same_rule_twice_is_a_no_op() {
        let mut rules = Rules::default();
        let src = item("Bash", "Bash:ls", "C:/work/app");
        rules.add(Rule::from_item(&src, Scope::ExactCall));
        rules.add(Rule::from_item(&src, Scope::ExactCall));
        assert_eq!(rules.list().len(), 1);
    }
}
