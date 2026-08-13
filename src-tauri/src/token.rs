//! The secret that has to appear in a hook URL for it to be answered.
//!
//! The server is bound to `127.0.0.1`, which keeps other machines out but not
//! other *processes* on this one. Without a secret, anything that can open a
//! local socket could post a row saying a session is about to run some
//! command, and the panel would show it exactly like a real one. Nothing is
//! executed as a result — answering a forged request only answers that
//! request — but the panel is a thing people trust at a glance, and it should
//! not be paintable by whatever else happens to be running.
//!
//! It is not a defence against code running as the same user: that code can
//! read this file. What it does close is the gap to *other* accounts on a
//! shared machine, which reach loopback but not each other's AppData.
//!
//! Browsers were checked separately and are already shut out: the endpoints
//! require `application/json`, which forces a CORS preflight that nothing here
//! answers.

use std::path::Path;
use std::sync::OnceLock;

static TOKEN: OnceLock<String> = OnceLock::new();

/// A v4 UUID with the hyphens removed: 32 hex characters from the system
/// generator, which is plenty for a value only ever compared, never guessed
/// at over a network.
fn generate() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn plausible(value: &str) -> bool {
    value.len() >= 16 && value.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Reads the token, writing a fresh one on first run.
///
/// Kept out of `settings.json` so it is never rendered into the settings
/// screen, and so hand-editing preferences cannot damage it.
pub fn init(config_dir: &Path) {
    let path = config_dir.join("hook-token");
    let existing = std::fs::read_to_string(&path)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| plausible(raw));

    let token = existing.unwrap_or_else(|| {
        let fresh = generate();
        let _ = std::fs::create_dir_all(config_dir);
        let _ = std::fs::write(&path, &fresh);
        fresh
    });
    let _ = TOKEN.set(token);
}

/// The token to put in a hook URL.
///
/// Falls back to a value that lives only in this process, so a build that
/// never called [`init`] — a test, or a run with an unreadable config
/// directory — still refuses forged requests rather than accepting every one.
pub fn current() -> &'static str {
    TOKEN.get_or_init(generate)
}

/// Whether a URL segment is the token.
///
/// Compared without stopping at the first difference. The length is not
/// secret — it is fixed and visible in `settings.json` — so returning early
/// on a length mismatch gives nothing away.
pub fn matches(candidate: &str) -> bool {
    let expected = current().as_bytes();
    let candidate = candidate.as_bytes();
    if candidate.len() != expected.len() {
        return false;
    }
    expected
        .iter()
        .zip(candidate)
        .fold(0u8, |differing, (a, b)| differing | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_it_hands_out_is_the_one_it_accepts() {
        assert!(matches(current()));
    }

    #[test]
    fn nothing_else_is_accepted() {
        assert!(!matches(""));
        assert!(!matches("0"));
        // Same length, one character out.
        let mut near = current().to_string();
        near.pop();
        near.push(if current().ends_with('a') { 'b' } else { 'a' });
        assert!(!matches(&near));
    }

    #[test]
    fn a_stored_token_is_reused_and_a_damaged_one_is_replaced() {
        let dir = std::env::temp_dir().join(format!("sp-token-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // A file that was truncated or hand-edited must not become the token.
        std::fs::write(dir.join("hook-token"), "short").unwrap();
        assert!(!plausible("short"));

        let kept = generate();
        std::fs::write(dir.join("hook-token"), &kept).unwrap();
        assert!(plausible(&kept));

        std::fs::remove_dir_all(&dir).ok();
    }
}
