//! Data shapes shared between the hook server, the inbox queue and the UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the user is being asked to do about an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    /// A tool call is blocked waiting for allow/deny. The HTTP request that
    /// created it is still open.
    Permission,
    /// The session asked a question and is idle until the user answers it in
    /// the editor.
    NeedsInput,
    /// The session finished its turn. Informational only.
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    Deny,
}

/// One row in the inbox.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub kind: ItemKind,
    pub agent: Agent,
    pub session_id: String,
    pub cwd: String,
    /// Last path segment of `cwd`, used as the human-facing session name.
    pub project: String,
    /// Display name for the project — the override if one is set.
    pub label: String,
    /// Colour the project is recognised by in the list.
    pub color: String,
    pub tool_name: String,
    /// One-line description, e.g. `npm test` or `src/main.rs`.
    pub summary: String,
    /// Full payload for the expanded view.
    pub detail: Option<String>,
    /// How the UI should render `detail`: `diff` colours `-`/`+` lines.
    pub detail_kind: String,
    /// Stable identity of the request, used for "always allow this exact call".
    pub signature: String,
    /// Set when a risk rule matched, so the row can be made to stand out.
    pub risk: Option<crate::risk::RiskMark>,
    /// How many times this has happened for the session. Repeats replace the
    /// row rather than adding one, so a chatty session stays one line.
    pub repeat: u32,
    /// Epoch milliseconds, so the UI can render elapsed time.
    pub created_at: u64,
}

/// Payload of a `PermissionRequest` hook.
#[derive(Debug, Deserialize)]
pub struct PermissionPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
}

/// Which agent a row came from. Claude Code can have its approvals answered
/// here; Codex only reports that a turn finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Agent {
    Claude,
    Codex,
}

/// Payload of Codex CLI's `notify` program.
///
/// Field names are hyphenated in Codex's JSON, and only `agent-turn-complete`
/// is ever sent — there is no approval event to hook.
#[derive(Debug, Deserialize)]
pub struct CodexPayload {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(rename = "thread-id", default)]
    pub thread_id: String,
    #[serde(rename = "turn-id", default)]
    pub turn_id: String,
    #[serde(rename = "last-assistant-message", default)]
    pub last_message: String,
    #[serde(rename = "input-messages", default)]
    pub input_messages: Vec<String>,
}

/// Payload of a `Notification` hook.
#[derive(Debug, Deserialize)]
pub struct NotificationPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub notification_type: String,
    #[serde(default)]
    pub message: String,
}

/// Payload of session lifecycle hooks.
#[derive(Debug, Deserialize)]
pub struct SessionPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn project_name(cwd: &str) -> String {
    cwd.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or("(unknown)")
        .to_string()
}

/// Collapses a tool call into a stable one-line identity.
///
/// Two calls with the same signature are the same decision as far as the
/// auto-allow rules are concerned, so this deliberately ignores volatile
/// fields such as `Edit`'s replacement text.
pub fn signature_of(tool_name: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str);

    let body = match tool_name {
        "Bash" | "PowerShell" => field("command"),
        "Read" | "Write" | "Edit" | "NotebookEdit" => field("file_path"),
        "WebFetch" => field("url"),
        "Glob" | "Grep" => field("pattern"),
        _ => None,
    };

    match body {
        Some(b) => format!("{tool_name}:{b}"),
        // Unknown tools fall back to the whole input so a rule can never be
        // broader than the call the user actually approved.
        None => format!("{tool_name}:{input}"),
    }
}

/// First line of `text`, truncated to `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= max {
        return line.to_string();
    }
    let head: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

const DETAIL_LIMIT: usize = 4000;
/// Enough to see the shape of a change without the row swallowing the panel.
const DIFF_LINE_LIMIT: usize = 40;

/// Renders an edit the way a diff reads, instead of as the raw JSON the hook
/// delivers. The question being answered is "what changes?", and escaped
/// newlines inside a one-line JSON blob do not answer it.
fn as_diff(before: &str, after: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut push = |text: &str, marker: char| {
        for line in text.lines().take(DIFF_LINE_LIMIT) {
            lines.push(format!("{marker} {line}"));
        }
        if text.lines().count() > DIFF_LINE_LIMIT {
            lines.push(format!(
                "{marker} … ({} more lines)",
                text.lines().count() - DIFF_LINE_LIMIT
            ));
        }
    };
    if !before.is_empty() {
        push(before, '-');
    }
    if !after.is_empty() {
        push(after, '+');
    }
    lines.join("\n")
}

pub fn summarize(tool_name: &str, input: &Value) -> (String, Option<String>, &'static str) {
    let field = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("");

    let summary = match tool_name {
        "Bash" | "PowerShell" => one_line(field("command"), 90),
        "Read" | "Write" | "Edit" | "NotebookEdit" => {
            let path = field("file_path");
            one_line(path.rsplit(['/', '\\']).next().unwrap_or(path), 90)
        }
        "WebFetch" | "WebSearch" => one_line(
            if tool_name == "WebFetch" {
                field("url")
            } else {
                field("query")
            },
            90,
        ),
        _ => one_line(&input.to_string(), 90),
    };

    let (detail, kind) = match tool_name {
        "Bash" | "PowerShell" => (field("command").to_string(), "text"),
        "Edit" => (as_diff(field("old_string"), field("new_string")), "diff"),
        "Write" => (as_diff("", field("content")), "diff"),
        "NotebookEdit" => (as_diff("", field("new_source")), "diff"),
        _ => (
            serde_json::to_string_pretty(input).unwrap_or_default(),
            "text",
        ),
    };

    let detail = if detail.chars().count() > DETAIL_LIMIT {
        let head: String = detail.chars().take(DETAIL_LIMIT).collect();
        Some(format!("{head}\n… (truncated)"))
    } else if detail.trim().is_empty() || detail == "null" {
        None
    } else {
        Some(detail)
    };

    (summary, detail, kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_edit_reads_as_a_diff_with_real_line_breaks() {
        let (summary, detail, kind) = summarize(
            "Edit",
            &json!({
                "file_path": "C:/work/app/src/screen.py",
                "old_string": "a = 1\nb = 2",
                "new_string": "a = 3\nb = 2",
            }),
        );

        assert_eq!(summary, "screen.py");
        assert_eq!(kind, "diff");
        assert_eq!(detail.unwrap(), "- a = 1\n- b = 2\n+ a = 3\n+ b = 2");
    }

    #[test]
    fn a_command_is_shown_verbatim_rather_than_as_json() {
        let (_, detail, kind) = summarize("Bash", &json!({ "command": "git push --force" }));
        assert_eq!(kind, "text");
        assert_eq!(detail.unwrap(), "git push --force");
    }
}
