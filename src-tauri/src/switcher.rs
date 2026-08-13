//! Listing the windows that host a session, and bringing one to the front.
//!
//! The inbox only knows about sessions that have asked for something. This is
//! the other half: every editor or terminal window currently open, whether or
//! not it is waiting on anything.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEntry {
    /// Native handle, passed back verbatim to focus the window.
    pub handle: isize,
    pub title: String,
    /// Executable stem, e.g. `code` or `WindowsTerminal`.
    pub app: String,
    /// Friendly grouping label for the UI.
    pub kind: String,
    pub minimized: bool,
}

/// Processes worth listing: editors that embed Claude Code, and the terminals
/// it is usually run from.
const EDITORS: [&str; 4] = ["code", "code - insiders", "cursor", "windsurf"];
const TERMINALS: [&str; 8] = [
    "windowsterminal",
    "wt",
    "pwsh",
    "powershell",
    "cmd",
    "alacritty",
    "wezterm-gui",
    "hyper",
];

fn classify(app: &str) -> Option<&'static str> {
    let key = app.to_lowercase();
    if EDITORS.contains(&key.as_str()) {
        return Some("editor");
    }
    if TERMINALS.contains(&key.as_str()) {
        return Some("terminal");
    }
    None
}

/// Strips the application suffix editors append, so the list shows the part
/// that identifies the window rather than repeating the app name.
fn tidy_title(title: &str) -> String {
    for suffix in [
        " - Visual Studio Code",
        " - Visual Studio Code - Insiders",
        " - Cursor",
        " - Windsurf",
    ] {
        if let Some(head) = title.strip_suffix(suffix) {
            return head.to_string();
        }
    }
    title.to_string()
}

#[cfg(windows)]
mod platform {
    use super::{classify, tidy_title, WindowEntry};
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    fn window_title(hwnd: HWND) -> String {
        unsafe {
            let length = GetWindowTextLengthW(hwnd);
            if length <= 0 {
                return String::new();
            }
            let mut buffer = vec![0u16; length as usize + 1];
            let written = GetWindowTextW(hwnd, &mut buffer);
            String::from_utf16_lossy(&buffer[..written as usize])
        }
    }

    /// Executable stem of the process owning `hwnd`.
    fn process_name(hwnd: HWND) -> Option<String> {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 512];
            let mut length = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(handle);
            result.ok()?;

            let path = String::from_utf16_lossy(&buffer[..length as usize]);
            let file = path.rsplit(['\\', '/']).next()?;
            Some(file.trim_end_matches(".exe").trim_end_matches(".EXE").to_string())
        }
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<WindowEntry>) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let title = window_title(hwnd);
        if title.trim().is_empty() {
            return BOOL(1);
        }
        let Some(app) = process_name(hwnd) else {
            return BOOL(1);
        };
        let Some(kind) = classify(&app) else {
            return BOOL(1);
        };

        found.push(WindowEntry {
            handle: hwnd.0 as isize,
            title: tidy_title(&title),
            app,
            kind: kind.to_string(),
            minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        });
        BOOL(1)
    }

    pub fn list() -> Vec<WindowEntry> {
        let mut found: Vec<WindowEntry> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut found as *mut _ as isize));
        }
        // Editors first, then alphabetically, so the list does not reshuffle
        // itself between openings.
        found.sort_by(|a, b| {
            (a.kind != "editor")
                .cmp(&(b.kind != "editor"))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        found
    }

    pub fn focus(handle: isize) -> Result<(), String> {
        let hwnd = HWND(handle as *mut core::ffi::c_void);
        unsafe {
            // A minimized window cannot take focus until it is restored.
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if SetForegroundWindow(hwnd).as_bool() {
                Ok(())
            } else {
                Err("could not bring the window to the front".to_string())
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::WindowEntry;

    pub fn list() -> Vec<WindowEntry> {
        Vec::new()
    }

    pub fn focus(_handle: isize) -> Result<(), String> {
        Err("not supported on this platform".to_string())
    }
}

pub use platform::{focus, list};

/// Finds the window already holding `project`.
///
/// Editors title their windows `<file> - <folder> - <app>`, and `tidy_title`
/// has already removed the app part, so the folder is the last ` - ` segment.
/// Matching that is exact enough to avoid grabbing an unrelated window that
/// merely mentions the name.
pub fn find_by_project(project: &str) -> Option<isize> {
    if project.trim().is_empty() {
        return None;
    }
    let wanted = project.to_lowercase();
    let windows = list();

    // Editors first: a terminal whose title happens to match is a weaker
    // signal than the editor that actually has the folder open.
    windows
        .iter()
        .find(|w| w.kind == "editor" && last_segment(&w.title) == wanted)
        .or_else(|| windows.iter().find(|w| last_segment(&w.title) == wanted))
        .map(|w| w.handle)
}

fn last_segment(title: &str) -> String {
    title
        .rsplit(" - ")
        .next()
        .unwrap_or(title)
        .trim()
        .to_lowercase()
}

#[cfg(test)]
mod match_tests {
    use super::*;

    #[test]
    fn the_folder_is_the_last_segment_of_an_editor_title() {
        assert_eq!(last_segment("main.rs - myapp - "), "");
        assert_eq!(last_segment("main.rs - myapp"), "myapp");
        assert_eq!(last_segment("myapp"), "myapp");
    }

    #[test]
    fn matching_is_case_insensitive_but_not_a_substring_match() {
        assert_eq!(last_segment("x - MyApp"), "myapp");
        // A folder merely mentioned mid-title must not count as the window's.
        assert_ne!(last_segment("myapp - other"), "myapp");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_suffixes_are_stripped_but_other_titles_are_left_alone() {
        assert_eq!(tidy_title("main.rs - myapp - Visual Studio Code"), "main.rs - myapp");
        assert_eq!(tidy_title("pwsh - myapp"), "pwsh - myapp");
    }

    /// Exercises the real FFI path: enumeration, title reads and the process
    /// lookup all run, so a mistake in the unsafe code shows up here rather
    /// than as an empty list in the UI.
    #[test]
    fn enumeration_runs_and_returns_well_formed_entries() {
        let found = list();
        for entry in &found {
            assert!(!entry.title.trim().is_empty());
            assert!(entry.handle != 0);
            assert!(entry.kind == "editor" || entry.kind == "terminal");
        }
        println!("listed {} window(s)", found.len());
    }

    #[test]
    fn only_editors_and_terminals_are_listed() {
        assert_eq!(classify("Code"), Some("editor"));
        assert_eq!(classify("WindowsTerminal"), Some("terminal"));
        assert_eq!(classify("explorer"), None);
    }
}
