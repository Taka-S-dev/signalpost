//! Wiring the Codex CLI's `notify` slot.
//!
//! Codex allows exactly one `notify` program, so installing cannot simply
//! overwrite it — whatever was there is chained behind the shim and restored
//! on uninstall. `toml_edit` is used so the rest of the user's config keeps
//! its formatting and comments.

use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Value};

const SHIM: &str = "signalpost-codex.exe";
/// Recognised so a config written under the old name can still be detected
/// and undone — otherwise renaming the app would strand a `notify` entry
/// pointing at a binary that no longer exists.
const FORMER_SHIM: &str = "claudenotify-codex.exe";
const CHAIN_FLAG: &str = "--chain";

pub fn config_path(home: &Path) -> PathBuf {
    home.join(".codex").join("config.toml")
}

/// The shim sits next to the app binary.
pub fn shim_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(SHIM))
}

fn is_shim(value: &str) -> bool {
    let value = value.to_lowercase().replace('\\', "/");
    value.ends_with(SHIM) || value.ends_with(FORMER_SHIM)
}

fn as_strings(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn array_of(values: &[String]) -> Item {
    let mut array = Array::new();
    for value in values {
        array.push(Value::from(value.clone()));
    }
    Item::Value(Value::Array(array))
}

/// Whether Codex is wired to a shim that actually exists.
///
/// Deliberately stricter than [`is_shim`]: an entry left by the previous name
/// points at a binary that is no longer there, and reporting that as
/// "configured" would be the same lie as saying hooks are installed while
/// nothing arrives. It has to read as not set up, so it gets set up.
pub fn is_installed(home: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(config_path(home)) else {
        return false;
    };
    let Ok(doc) = raw.parse::<DocumentMut>() else {
        return false;
    };
    as_strings(doc.get("notify")).first().is_some_and(|first| {
        first.to_lowercase().replace('\\', "/").ends_with(SHIM) && Path::new(first).exists()
    })
}

/// Prepends the shim.
///
/// `keep_existing` chains whatever program was configured behind ours;
/// without it the slot is taken outright, which is simpler and is the right
/// choice when the previous program is not wanted.
pub fn install(home: &Path, shim: &Path, keep_existing: bool) -> Result<PathBuf, String> {
    let path = config_path(home);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc = existing
        .parse::<DocumentMut>()
        .map_err(|e| format!("could not parse config.toml: {e}"))?;

    let current = as_strings(doc.get("notify"));
    // Re-installing must not chain the shim to itself.
    let chained: Vec<String> = if current.first().is_some_and(|f| is_shim(f)) {
        current
            .iter()
            .skip_while(|v| v.as_str() != CHAIN_FLAG)
            .skip(1)
            .cloned()
            .collect()
    } else {
        current
    };

    let chained = if keep_existing { chained } else { Vec::new() };

    let mut notify = vec![shim.display().to_string()];
    if !chained.is_empty() {
        notify.push(CHAIN_FLAG.to_string());
        notify.extend(chained);
    }
    doc["notify"] = array_of(&notify);

    let rendered = doc.to_string();
    // TOML puts root keys before tables; refuse to write anything that would
    // not parse back rather than leaving a broken config behind.
    rendered
        .parse::<DocumentMut>()
        .map_err(|e| format!("refusing to write an invalid config.toml: {e}"))?;

    if !existing.is_empty() {
        let _ = std::fs::write(path.with_extension("toml.bak"), &existing);
    }
    std::fs::write(&path, rendered).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Puts the chained program back in the `notify` slot, or removes the key.
pub fn uninstall(home: &Path) -> Result<(), String> {
    let path = config_path(home);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let mut doc = raw
        .parse::<DocumentMut>()
        .map_err(|e| format!("could not parse config.toml: {e}"))?;

    let current = as_strings(doc.get("notify"));
    if !current.first().is_some_and(|f| is_shim(f)) {
        return Ok(());
    }

    let chained: Vec<String> = current
        .iter()
        .skip_while(|v| v.as_str() != CHAIN_FLAG)
        .skip(1)
        .cloned()
        .collect();

    if chained.is_empty() {
        doc.remove("notify");
    } else {
        doc["notify"] = array_of(&chained);
    }

    std::fs::write(&path, doc.to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cn-codex-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".codex")).unwrap();
        if !contents.is_empty() {
            std::fs::write(config_path(&dir), contents).unwrap();
        }
        dir
    }

    fn notify_of(home: &Path) -> Vec<String> {
        let raw = std::fs::read_to_string(config_path(home)).unwrap();
        as_strings(raw.parse::<DocumentMut>().unwrap().get("notify"))
    }

    #[test]
    fn an_existing_notify_program_is_chained_not_replaced() {
        let home = scratch("notify = [ \"orig.exe\", \"turn-ended\" ]\nmodel = \"gpt\"\n");
        install(&home, Path::new("C:/app/signalpost-codex.exe"), true).unwrap();

        assert_eq!(
            notify_of(&home),
            vec![
                "C:/app/signalpost-codex.exe",
                "--chain",
                "orig.exe",
                "turn-ended"
            ]
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn installing_twice_does_not_chain_the_shim_to_itself() {
        let home = scratch("notify = [ \"orig.exe\" ]\n");
        let shim = Path::new("C:/app/signalpost-codex.exe");
        install(&home, shim, true).unwrap();
        install(&home, shim, true).unwrap();

        assert_eq!(
            notify_of(&home),
            vec!["C:/app/signalpost-codex.exe", "--chain", "orig.exe"]
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn an_entry_pointing_at_a_missing_shim_does_not_count_as_installed() {
        let home = scratch("notify = [ \"C:/gone/signalpost-codex.exe\" ]\n");
        assert!(!is_installed(&home));

        let former = scratch("notify = [ \"C:/gone/claudenotify-codex.exe\" ]\n");
        assert!(!is_installed(&former));

        std::fs::remove_dir_all(&home).ok();
        std::fs::remove_dir_all(&former).ok();
    }

    #[test]
    fn uninstall_restores_the_original_program() {
        let home = scratch("notify = [ \"orig.exe\", \"turn-ended\" ]\n");
        // A real path is needed: is_installed checks the shim is on disk.
        let shim = std::env::current_exe().unwrap();
        let shim = shim.with_file_name(SHIM);
        std::fs::write(&shim, b"").unwrap();
        install(&home, &shim, true).unwrap();
        assert!(is_installed(&home));

        uninstall(&home).unwrap();
        assert_eq!(notify_of(&home), vec!["orig.exe", "turn-ended"]);
        assert!(!is_installed(&home));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn unrelated_settings_and_tables_survive_the_edit() {
        let home = scratch(
            "notify = [ \"orig.exe\" ]\nmodel = \"gpt-5.6\"\n\n[windows]\nsandbox = \"elevated\"\n",
        );
        install(&home, Path::new("C:/app/signalpost-codex.exe"), true).unwrap();

        let raw = std::fs::read_to_string(config_path(&home)).unwrap();
        let doc = raw.parse::<DocumentMut>().unwrap();
        assert_eq!(doc["model"].as_str(), Some("gpt-5.6"));
        assert_eq!(doc["windows"]["sandbox"].as_str(), Some("elevated"));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn overriding_drops_the_existing_program_instead_of_chaining_it() {
        let home = scratch("notify = [ \"orig.exe\", \"turn-ended\" ]\n");
        install(&home, Path::new("C:/app/signalpost-codex.exe"), false).unwrap();

        assert_eq!(notify_of(&home), vec!["C:/app/signalpost-codex.exe"]);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_config_left_by_the_former_name_is_replaced_rather_than_chained() {
        let home =
            scratch("notify = [ \"C:/old/claudenotify-codex.exe\", \"--chain\", \"orig.exe\" ]\n");
        install(&home, Path::new("C:/app/signalpost-codex.exe"), true).unwrap();

        assert_eq!(
            notify_of(&home),
            vec!["C:/app/signalpost-codex.exe", "--chain", "orig.exe"]
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_config_without_notify_gets_a_valid_one() {
        let home = scratch("model = \"gpt\"\n\n[windows]\nsandbox = \"elevated\"\n");
        install(&home, Path::new("C:/app/signalpost-codex.exe"), true).unwrap();

        assert_eq!(notify_of(&home), vec!["C:/app/signalpost-codex.exe"]);
        std::fs::remove_dir_all(&home).ok();
    }
}
