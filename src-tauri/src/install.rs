//! One-click installation of the hooks into `~/.claude/settings.json`.
//!
//! Hooks are registered without a matcher and filtered server-side, so a
//! future notification type cannot silently stop reaching the inbox.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::server;

pub fn settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// The token sits in the path so that a process which cannot read the config
/// directory cannot post to these endpoints at all.
fn endpoint(path: &str) -> String {
    format!(
        "http://127.0.0.1:{}/hook/{}/{}",
        server::port(),
        crate::token::current(),
        path
    )
}

fn hook_entry(path: &str, timeout: u64) -> Value {
    json!({
        "hooks": [{
            "type": "http",
            "url": endpoint(path),
            "timeout": timeout,
        }]
    })
}

/// The three events the app registers, and the endpoint each one posts to.
fn wiring() -> Vec<(&'static str, &'static str, u64)> {
    vec![
        // 600s is the maximum Claude Code allows; the request is parked for
        // almost all of it while the row waits in the inbox.
        ("PermissionRequest", "permission", 600),
        ("Notification", "notification", 10),
        // These three exist to retire rows the app is still blocking on when
        // the decision was made in the editor instead.
        ("PostToolUse", "tool-settled", 10),
        ("PermissionDenied", "tool-settled", 10),
        // A session says nothing while it works, so this is what tells us a
        // busy one exists.
        ("UserPromptSubmit", "turn-start", 10),
        ("Stop", "turn-end", 10),
        ("SessionEnd", "session-end", 10),
    ]
}

fn urls(entry: &Value) -> Vec<&str> {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks
                .iter()
                .filter_map(|h| h.get("url").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

/// True when an entry points at one of our endpoints, whatever the port or
/// token. Deliberately loose: this is what finds entries to replace or
/// remove, including ones written by an older build.
fn is_ours(entry: &Value) -> bool {
    urls(entry)
        .iter()
        .any(|u| u.contains("127.0.0.1") && u.contains("/hook/"))
}

/// True only when an entry would actually reach this build.
///
/// An entry written before the token existed, or under a token since
/// replaced, still looks like ours but is answered with 404. Reporting that
/// as installed would show a green setup screen for hooks that cannot
/// deliver anything.
fn is_current(entry: &Value) -> bool {
    let prefix = format!("/hook/{}/", crate::token::current());
    urls(entry)
        .iter()
        .any(|u| u.contains("127.0.0.1") && u.contains(&prefix))
}

/// True when hooks are wired up, but to a different copy of the app.
///
/// Each copy writes its own token beside its own settings, so an installed
/// build and a portable one never accept each other's URLs. Reporting that as
/// "not set up" invites the fix that overwrites the other copy's hooks, and
/// it is answerable from the file alone — waiting for a refused request means
/// waiting for the other copy to be running too.
pub fn points_elsewhere(home: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(settings_path(home)) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&raw) else {
        return false;
    };
    let entries: Vec<&Value> = wiring()
        .iter()
        .filter_map(|(event, _, _)| {
            settings
                .get("hooks")
                .and_then(|h| h.get(event))
                .and_then(Value::as_array)
        })
        .flatten()
        .collect();

    entries.iter().any(|e| is_ours(e)) && !entries.iter().any(|e| is_current(e))
}

pub fn is_installed(home: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(settings_path(home)) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&raw) else {
        return false;
    };
    wiring().iter().all(|(event, _, _)| {
        settings
            .get("hooks")
            .and_then(|h| h.get(event))
            .and_then(Value::as_array)
            .map(|entries| entries.iter().any(is_current))
            .unwrap_or(false)
    })
}

/// Merges the hooks into the user's settings, preserving everything else and
/// leaving a `.bak` copy of the previous file.
pub fn install(home: &Path) -> Result<PathBuf, String> {
    let path = settings_path(home);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let existing = std::fs::read_to_string(&path).ok();
    let mut settings: Value = match &existing {
        Some(raw) if !raw.trim().is_empty() => {
            serde_json::from_str(raw).map_err(|e| format!("could not parse settings.json: {e}"))?
        }
        _ => Value::Object(Map::new()),
    };

    if !settings.is_object() {
        return Err("settings.json is not a JSON object".into());
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or("settings.json hooks is not an object")?;

    for (event, endpoint_path, timeout) in wiring() {
        let entries = hooks.entry(event).or_insert_with(|| Value::Array(vec![]));
        let entries = entries
            .as_array_mut()
            .ok_or_else(|| format!("settings.json hooks.{event} is not an array"))?;
        entries.retain(|e| !is_ours(e));
        entries.push(hook_entry(endpoint_path, timeout));
    }

    if let Some(previous) = existing {
        let _ = std::fs::write(path.with_extension("json.bak"), previous);
    }
    let rendered = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, rendered).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Removes the hooks again, leaving the rest of the file untouched.
pub fn uninstall(home: &Path) -> Result<(), String> {
    let path = settings_path(home);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let mut settings: Value =
        serde_json::from_str(&raw).map_err(|e| format!("could not parse settings.json: {e}"))?;

    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for (_, entries) in hooks.iter_mut() {
            if let Some(entries) = entries.as_array_mut() {
                entries.retain(|e| !is_ours(e));
            }
        }
    }

    let rendered = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, rendered).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_preserves_unrelated_settings_and_hooks() {
        let dir = std::env::temp_dir().join(format!("cn-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(
            settings_path(&dir),
            r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .unwrap();

        install(&dir).unwrap();
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(settings_path(&dir)).unwrap()).unwrap();

        assert_eq!(after["model"], "opus");
        // The app registers a Stop hook of its own, so the user's must survive
        // alongside it rather than be replaced by it.
        let stop = after["hooks"]["Stop"].as_array().unwrap();
        assert!(stop.iter().any(|e| e["hooks"][0]["command"] == "echo hi"));
        assert!(stop.iter().any(is_ours));
        assert!(is_installed(&dir));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The state that used to read as "not set up at all", which invites the
    /// button that overwrites the other copy's hooks.
    #[test]
    fn hooks_written_by_another_copy_are_told_apart_from_no_hooks() {
        let dir = std::env::temp_dir().join(format!("cn-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".claude")).unwrap();

        // Nothing at all.
        std::fs::write(settings_path(&dir), r#"{"hooks":{}}"#).unwrap();
        assert!(!is_installed(&dir));
        assert!(!points_elsewhere(&dir));

        // Our shape, someone else's token.
        std::fs::write(
            settings_path(&dir),
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
               "url":"http://127.0.0.1:8787/hook/ffffffffffffffffffffffffffffffff/turn-end"}]}]}}"#,
        )
        .unwrap();
        assert!(!is_installed(&dir));
        assert!(points_elsewhere(&dir));

        // Ours, current.
        install(&dir).unwrap();
        assert!(is_installed(&dir));
        assert!(!points_elsewhere(&dir));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn installing_twice_does_not_duplicate_entries() {
        let dir = std::env::temp_dir().join(format!("cn-test-{}", uuid::Uuid::new_v4()));
        install(&dir).unwrap();
        install(&dir).unwrap();

        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(settings_path(&dir)).unwrap()).unwrap();
        assert_eq!(
            after["hooks"]["PermissionRequest"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        uninstall(&dir).unwrap();
        assert!(!is_installed(&dir));

        std::fs::remove_dir_all(&dir).ok();
    }
}
