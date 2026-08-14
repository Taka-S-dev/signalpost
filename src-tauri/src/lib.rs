mod codex;
mod install;
mod model;
mod profiles;
mod risk;
mod rules;
mod server;
mod sessions;
mod settings;
mod state;
mod switcher;
mod token;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State};

use model::{Decision, Item};
use profiles::ResolvedProfile;
use risk::RiskRule;
use rules::{RuleView, Scope};
use settings::Settings;
use state::{AppState, Geometry};

pub const TRAY_ID: &str = "signalpost-tray";

/// What the app is called in any folder a user might open. Deliberately not
/// the bundle identifier: that has to be a globally unique reverse-DNS
/// string, which is not a name to leave lying in someone's AppData.
const PRODUCT_DIR: &str = "Signalpost";

/// Ships beside the executable in the portable download. Its presence is what
/// makes the build portable; the installer does not lay it down.
const PORTABLE_MARKER: &str = "portable.txt";

/// Where settings, rules and the webview's data go.
///
/// A build calling itself portable has to leave nothing behind, so the marker
/// moves everything next to the executable. An explicit file rather than a
/// guess about where the exe sits: "am I installed?" has no reliable answer,
/// and getting it wrong silently moves someone's rules somewhere they did not
/// look.
fn state_dir(exe_dir: Option<&Path>, roaming: Option<&Path>) -> PathBuf {
    if let Some(dir) = exe_dir {
        if dir.join(PORTABLE_MARKER).is_file() {
            return dir.join("data");
        }
    }
    match roaming {
        Some(dir) => dir.join(PRODUCT_DIR),
        None => std::env::temp_dir().join(PRODUCT_DIR),
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}
/// Long enough to get through a meeting, short enough to forget about safely.
const SNOOZE_MINUTES: u64 = 30;

type Shared<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
fn list_items(state: Shared) -> Vec<Item> {
    state.items()
}

#[tauri::command]
fn resolve(
    state: Shared,
    id: String,
    decision: Decision,
    remember: Option<Scope>,
) -> state::ResolveOutcome {
    state.resolve(&id, decision, remember)
}

/// What to prefill the "commands starting with" box with.
///
/// Derived here rather than in the UI so the suggestion and the matching it
/// feeds can never drift apart.
#[tauri::command]
fn suggest_prefix(signature: String) -> String {
    rules::suggested_prefix(&signature)
}

/// Takes back the standing rule the last answer created.
#[tauri::command]
fn undo_last_rule(state: Shared) -> Vec<RuleView> {
    state.undo_last_rule()
}

#[tauri::command]
fn dismiss(state: Shared, id: String) {
    state.dismiss(&id);
}

/// Clears every informational row. Blocked calls are left alone.
#[tauri::command]
fn dismiss_all(state: Shared) -> usize {
    state.dismiss_all_info()
}

/// Brings the session's window forward. The row stays in the inbox: jumping
/// over to read the code is not a decision.
#[tauri::command]
fn focus_editor(state: Shared, id: String) -> Result<(), String> {
    let item = state
        .items()
        .into_iter()
        .find(|i| i.id == id)
        .ok_or("that item is gone")?;
    go_to(&state, &item.cwd, &item.project)
}

/// Raises the window that already has the project open, and only launches
/// something when there is no such window.
///
/// `code <folder>` does *not* raise an existing window on Windows — it returns
/// having done nothing visible — so the CLI cannot be the way this works.
/// Focusing the window directly can, because the panel is the foreground
/// window at the moment the user clicks it.
fn go_to(state: &Arc<AppState>, cwd: &str, project: &str) -> Result<(), String> {
    let explicit = state.has_open_command(cwd);
    if !explicit {
        if let Some(handle) = switcher::find_by_project(project) {
            switcher::focus(handle)?;
            if let Some(window) = ui::panel(state.app()) {
                let _ = window.hide();
            }
            return Ok(());
        }
    }

    // Either the user configured a command for this project, or nothing has
    // the folder open and it has to be started.
    ui::run_open_command(&state.open_command(cwd))?;
    if let Some(window) = ui::panel(state.app()) {
        let _ = window.hide();
    }
    Ok(())
}

/// Gets the panel out of the way.
///
/// While anything is still queued it collapses rather than disappears, so
/// there is always something on screen to notice — hiding it completely is
/// what made rows easy to miss.
#[tauri::command]
fn hide_panel(state: Shared) {
    if state.items().is_empty() {
        if let Some(window) = ui::panel(state.app()) {
            let _ = window.hide();
        }
    } else {
        ui::show_pill(state.app());
    }
}

/// `peek` means the pointer opened it, so moving away closes it again. The
/// watching is done natively against the OS cursor rather than from DOM
/// events, which cannot tell a tooltip apart from a real departure.
#[tauri::command]
fn expand_panel(state: Shared, peek: Option<bool>) {
    // "Keep the list open" has to win over every automatic collapse, not just
    // the one that fires when the queue drains. Opening by hover would
    // otherwise still arm the watcher and close it on the way out.
    let peek = peek.unwrap_or(false) && !state.settings().keep_open;
    state.set_peeking(peek);
    ui::reveal(state.app());
    if peek {
        ui::watch_peek(state.app());
    }
}

/// Stops the peek entirely. Used when a text field takes focus: typing must
/// never be interrupted by the panel deciding the pointer has wandered off.
/// Clicking otherwise changes nothing — the pointer leaving is the signal,
/// and a second timing for "clicked first" only made it harder to predict.
#[tauri::command]
fn pin_panel(state: Shared) {
    state.set_peeking(false);
}

/// Collapses on request, whether or not anything is queued — staying as a bar
/// is a choice, not only what happens when there is little to show.
#[tauri::command]
fn collapse_panel(state: Shared) {
    ui::show_pill(state.app());
}

/// The webview reloads independently of the window — during development, and
/// whenever it is reattached — so it has to ask which shape it is in rather
/// than assume the full panel and render it into a collapsed window.
#[tauri::command]
fn get_mode(state: Shared) -> &'static str {
    if state.is_pill() {
        "pill"
    } else {
        "full"
    }
}

#[tauri::command]
fn list_rules(state: Shared) -> Vec<RuleView> {
    state.rule_views()
}

#[tauri::command]
fn remove_rule(state: Shared, index: usize) -> Vec<RuleView> {
    state.remove_rule(index);
    state.rule_views()
}

/// Whether the hooks are wired up *and* whether they have actually fired
/// since they were written.
///
/// "Installed" alone is misleading: hooks are read when a session starts, so
/// existing sessions keep running without them. The screen said "installed"
/// while nothing was arriving, and there was no way to tell that apart from
/// a broken setup.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HookStatus {
    installed: bool,
    /// Milliseconds since the config was written, or null if never.
    installed_at: Option<u64>,
    /// Milliseconds of the last hook we received, or null if none yet.
    last_hook_at: Option<u64>,
    /// How many hooks were refused for carrying another copy's token, and how
    /// long ago the last one was. Null while nothing has been misaddressed.
    misrouted: Option<u64>,
    misrouted_at: Option<u64>,
}

#[tauri::command]
fn hooks_status(state: Shared) -> HookStatus {
    let Ok(home) = state.app().path().home_dir() else {
        return HookStatus {
            installed: false,
            installed_at: None,
            last_hook_at: None,
            misrouted: None,
            misrouted_at: None,
        };
    };
    // The settings file's own timestamp survives restarts, so this does not
    // need remembering separately.
    let installed_at = std::fs::metadata(install::settings_path(&home))
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    let (misrouted, misrouted_at) = match token::misrouted() {
        Some((count, since)) => (Some(count), Some(since)),
        None => (None, None),
    };

    HookStatus {
        installed: install::is_installed(&home),
        installed_at,
        last_hook_at: state.last_hook_at(),
        misrouted,
        misrouted_at,
    }
}

#[tauri::command]
fn install_hooks(state: Shared) -> Result<String, String> {
    let home = state.app().path().home_dir().map_err(|e| e.to_string())?;
    let written = install::install(&home).map(|p| p.display().to_string())?;
    // The hooks now carry this copy's token, so whatever was refused before
    // says nothing about the setup from here on.
    token::forget_mismatches();
    Ok(written)
}

#[tauri::command]
fn uninstall_hooks(state: Shared) -> Result<(), String> {
    let home = state.app().path().home_dir().map_err(|e| e.to_string())?;
    install::uninstall(&home)
}

#[tauri::command]
fn codex_installed(state: Shared) -> bool {
    match state.app().path().home_dir() {
        Ok(home) => codex::is_installed(&home),
        Err(_) => false,
    }
}

#[tauri::command]
fn install_codex(state: Shared, keep_existing: bool) -> Result<String, String> {
    let home = state.app().path().home_dir().map_err(|e| e.to_string())?;
    let shim = codex::shim_path().ok_or("could not locate the Codex shim")?;
    if !shim.exists() {
        return Err(format!("{} is missing", shim.display()));
    }
    codex::install(&home, &shim, keep_existing).map(|p| p.display().to_string())
}

#[tauri::command]
fn uninstall_codex(state: Shared) -> Result<(), String> {
    let home = state.app().path().home_dir().map_err(|e| e.to_string())?;
    codex::uninstall(&home)
}

#[tauri::command]
fn server_port() -> u16 {
    server::port()
}

#[tauri::command]
fn list_projects(state: Shared) -> Vec<ResolvedProfile> {
    state.projects()
}

#[tauri::command]
fn set_project(
    state: Shared,
    cwd: String,
    name: Option<String>,
    color: Option<String>,
    open_command: Option<String>,
) -> Vec<ResolvedProfile> {
    state.set_project(&cwd, name, color, open_command)
}

#[tauri::command]
fn default_open_command() -> &'static str {
    profiles::DEFAULT_OPEN_COMMAND
}

/// Puts the panel back at its default dock, for when a remembered placement
/// ends up somewhere unusable.
#[tauri::command]
fn reset_panel_position(state: Shared) {
    state.reset_geometry();
    if let Some(window) = ui::panel(state.app()) {
        let _ = window.set_size(tauri::LogicalSize::new(
            state::DEFAULT_WIDTH,
            state::DEFAULT_HEIGHT,
        ));
        ui::place(state.app(), &window);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn list_risk_rules(state: Shared) -> Vec<RiskRule> {
    state.risk_rules()
}

#[tauri::command]
fn set_risk_rules(state: Shared, rules: Vec<RiskRule>) -> Vec<RiskRule> {
    state.set_risk_rules(rules)
}

#[tauri::command]
fn restore_risk_defaults(state: Shared) -> Vec<RiskRule> {
    state.restore_risk_defaults()
}

#[tauri::command]
fn list_sessions(state: Shared) -> Vec<sessions::Session> {
    state.sessions()
}

/// Brings a session's window forward.
#[tauri::command]
fn focus_session(state: Shared, cwd: String) -> Result<(), String> {
    let project = model::project_name(&cwd);
    go_to(&state, &cwd, &project)
}

#[tauri::command]
fn list_windows() -> Vec<switcher::WindowEntry> {
    switcher::list()
}

/// Focuses another application's window, then gets out of the way — leaving
/// the panel on top of what the user just asked to look at defeats the point.
#[tauri::command]
fn focus_window(state: Shared, handle: isize) -> Result<(), String> {
    switcher::focus(handle)?;
    if let Some(window) = ui::panel(state.app()) {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
fn palette() -> Vec<String> {
    profiles::Profiles::palette()
}

/// The tray lives outside the web view, so the frontend hands it the strings
/// for the active language rather than Rust keeping a second translation.
/// Epoch milliseconds the suppression ends, or null when it is not in force.
#[tauri::command]
fn get_snooze(state: Shared) -> Option<u64> {
    state.snoozed_until()
}

#[tauri::command]
fn toggle_snooze_command(state: Shared) -> Option<u64> {
    toggle_snooze(state.app());
    state.snoozed_until()
}

#[tauri::command]
fn set_tray_strings(state: Shared, strings: ui::TrayStrings) -> tauri::Result<()> {
    let app = state.app().clone();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(tray_menu(&app, &strings)?))?;
    }
    state.set_tray_strings(strings);
    Ok(())
}

#[tauri::command]
fn get_settings(state: Shared) -> Settings {
    state.settings()
}

#[tauri::command]
fn set_settings(state: Shared, settings: Settings) -> Settings {
    state.set_settings(settings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second copy could not bind the port, and would leave the user
        // with a panel that never fills up. Surface the running one instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            ui::reveal(app);
        }))
        .invoke_handler(tauri::generate_handler![
            list_items,
            resolve,
            dismiss,
            dismiss_all,
            focus_editor,
            hide_panel,
            expand_panel,
            pin_panel,
            collapse_panel,
            get_mode,
            list_rules,
            remove_rule,
            undo_last_rule,
            suggest_prefix,
            hooks_status,
            install_hooks,
            uninstall_hooks,
            codex_installed,
            install_codex,
            uninstall_codex,
            server_port,
            get_settings,
            set_settings,
            list_projects,
            set_project,
            default_open_command,
            palette,
            list_sessions,
            focus_session,
            list_windows,
            focus_window,
            reset_panel_position,
            list_risk_rules,
            set_risk_rules,
            restore_risk_defaults,
            set_tray_strings,
            active_shortcut,
            set_shortcut,
            get_snooze,
            toggle_snooze_command,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Losing the config directory only costs rule persistence; dying
            // here would take the whole approval path down with it.
            let roaming = app.path().config_dir().ok();
            if roaming.is_none() {
                eprintln!("Signalpost: could not resolve the config directory; falling back to a temporary one");
            }
            let config_dir = state_dir(exe_dir().as_deref(), roaming.as_deref());
            // Before anything reads a hook URL: both the server and the
            // installer need the same token, and the installer is reachable
            // from the setup screen as soon as the window exists.
            token::init(&config_dir);
            let shared = Arc::new(AppState::new(handle.clone(), config_dir));
            app.manage(shared.clone());

            build_panel(app)?;
            setup_tray(app)?;
            setup_shortcut(app);

            if let Some(window) = ui::panel(&handle) {
                ui::place(&handle, &window);
            }
            // Asking for the list to stay open means it should be open now,
            // not from the next arrival onwards.
            if shared.settings().keep_open {
                ui::expand(&handle);
            }

            tauri::async_runtime::spawn(async move {
                if let Err(error) = server::serve(shared).await {
                    eprintln!("Signalpost: could not bind port {}: {error}", server::port());
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // The panel lives in the tray; closing it should not end the app,
            // because the hooks would then start timing out silently.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Once the panel has been placed by hand, that placement wins over
            // the default docking for every later appearance.
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                remember_geometry(window);
            }
            // Crossing to a monitor with different scaling makes Windows
            // resize the panel. Those intermediate sizes are measured against
            // a scale factor mid-change, and saving one shrinks the panel.
            tauri::WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = window.app_handle().try_state::<Arc<AppState>>() {
                    state.suppress_geometry_saves(1200);
                    if let Some(saved) = state.geometry() {
                        let _ = window.set_size(tauri::LogicalSize::new(saved.width, saved.height));
                    }
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Persists the panel's placement. Hidden windows report stale geometry on
/// Windows, so a hide is not allowed to overwrite a real placement.
fn remember_geometry(window: &tauri::Window) {
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let Some(state) = window.app_handle().try_state::<Arc<AppState>>() else {
        return;
    };
    if state.geometry_saves_suppressed() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);

    // While collapsed the size belongs to the app, but the position is still
    // the user's: dragging the bar somewhere has to survive expanding it.
    if state.is_pill() {
        state.save_position(position.x, position.y);
        return;
    }
    state.save_geometry(Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    });
}

/// The panel is built here rather than declared in `tauri.conf.json` for one
/// reason: only the builder can be given an absolute data directory.
///
/// Left to itself the webview writes to a folder named after the bundle
/// identifier, so the identifier ends up carved into `%LOCALAPPDATA%` on
/// every machine the app is installed on. The identifier has to be a
/// reverse-DNS string to be unique; a folder someone else has to look at
/// should be the product's name. This is the only way to have both.
fn build_panel(app: &tauri::App) -> tauri::Result<()> {
    // Through the same decision as the settings: a portable copy that still
    // left an EBWebView folder in %LOCALAPPDATA% would not be portable, and
    // the cache is the larger of the two.
    let local = app
        .path()
        .app_local_data_dir()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let data_dir = state_dir(exe_dir().as_deref(), local.as_deref());

    tauri::WebviewWindowBuilder::new(app, "panel", tauri::WebviewUrl::default())
        .title(PRODUCT_DIR)
        .inner_size(400.0, 620.0)
        .min_inner_size(320.0, 260.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // Placed and revealed once the queue has something in it.
        .visible(false)
        .shadow(false)
        .data_directory(data_dir)
        .build()?;
    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let defaults = ui::TrayStrings::default();
    let menu = tray_menu(app.handle(), &defaults)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip(&defaults.idle)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "mode" => {
                if in_panel_mode(app) {
                    ui::show_pill(app)
                } else {
                    ui::reveal(app)
                }
                refresh_tray_menu(app);
            }
            "snooze" => toggle_snooze(app),
            "reset" => {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    reset_panel_position(state);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                ui::toggle(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn tray_menu(app: &tauri::AppHandle, strings: &ui::TrayStrings) -> tauri::Result<Menu<tauri::Wry>> {
    let snoozed = app
        .try_state::<Arc<AppState>>()
        .and_then(|s| s.snoozed_until())
        .is_some();
    let snooze_label = if snoozed {
        &strings.unsnooze
    } else {
        &strings.snooze
    };

    // One item, not two. Listing both directions meant one of them was always
    // a no-op — "collapse to bar" while already a bar — and neither label said
    // which state you were in. This reads as the thing it will do next, the
    // same way the snooze item below it does.
    let mode_label = if in_panel_mode(app) {
        &strings.bar
    } else {
        &strings.show
    };

    let mode = MenuItem::with_id(app, "mode", mode_label, true, None::<&str>)?;
    let snooze = MenuItem::with_id(app, "snooze", snooze_label, true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", &strings.reset, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &strings.quit, true, None::<&str>)?;
    Menu::with_items(app, &[&mode, &snooze, &reset, &quit])
}

/// True only when the panel is on screen at full size. A hidden window is not
/// something to collapse, so from the tray it offers to open instead.
fn in_panel_mode(app: &tauri::AppHandle) -> bool {
    let visible = ui::panel(app)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let pill = app
        .try_state::<Arc<AppState>>()
        .map(|s| s.is_pill())
        .unwrap_or(false);
    showing_panel(visible, pill)
}

/// There are three states, not two — hidden, bar, panel — so the tray item
/// cannot simply invert. Split out from the window it reads so the third state
/// is pinned by a test rather than by whichever one was in front of me.
fn showing_panel(visible: bool, pill: bool) -> bool {
    visible && !pill
}

/// Rebuilds the menu so its single mode item matches the state it describes.
pub(crate) fn refresh_tray_menu(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = tray_menu(app, &state.tray_strings()) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Flips the suppression and rebuilds the menu so its label matches.
fn toggle_snooze(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    if state.snoozed_until().is_some() {
        state.clear_snooze();
    } else {
        state.snooze(SNOOZE_MINUTES);
    }
    refresh_tray_menu(app);
}

/// Registers the global shortcut, and carries on without it if the key is
/// taken.
///
/// Another application owning `Alt+Space` used to abort startup entirely,
/// which meant no panel, no server, and every approval falling back to the
/// editor — a total outage caused by a convenience key being unavailable.
fn setup_shortcut(app: &tauri::App) {
    use tauri_plugin_global_shortcut::ShortcutState;

    // The plugin itself is registered once, with no shortcut; the actual key
    // is chosen afterwards so it can be changed without restarting.
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                ui::toggle(app);
            }
        })
        .build();
    if let Err(error) = app.handle().plugin(plugin) {
        eprintln!("Signalpost: no global shortcut ({error}); use the tray instead");
        return;
    }

    let configured = app
        .try_state::<Arc<AppState>>()
        .map(|s| s.settings().shortcut)
        .unwrap_or_else(|| settings::DEFAULT_SHORTCUT.to_string());
    apply_shortcut(app.handle(), &configured);
}

/// Registers the first shortcut that is actually free.
///
/// `Alt+Space` is what PowerToys' Command Palette takes by default, so the
/// obvious choice is frequently unavailable. Falling back silently to nothing
/// would leave the only keyboard way in dead with no explanation, so the one
/// that worked is recorded and shown in the UI.
fn apply_shortcut(app: &tauri::AppHandle, wanted: &str) -> Option<String> {
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    let candidates = std::iter::once(wanted)
        .chain(settings::FALLBACK_SHORTCUTS.iter().copied())
        .collect::<Vec<_>>();

    for candidate in candidates {
        let Ok(shortcut) = Shortcut::from_str(candidate) else {
            continue;
        };
        if manager.register(shortcut).is_ok() {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                state.set_active_shortcut(Some(candidate.to_string()));
            }
            if candidate != wanted {
                eprintln!("Signalpost: {wanted} was taken; using {candidate}");
            }
            return Some(candidate.to_string());
        }
    }

    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.set_active_shortcut(None);
    }
    eprintln!("Signalpost: no global shortcut is available; use the tray instead");
    None
}

/// The shortcut that is actually in force, which may not be the configured
/// one.
#[tauri::command]
fn active_shortcut(state: Shared) -> Option<String> {
    state.active_shortcut()
}

#[tauri::command]
fn set_shortcut(state: Shared, shortcut: String) -> Option<String> {
    apply_shortcut(state.app(), &shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-state-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_marker_next_to_the_executable_moves_the_state_beside_it() {
        let exe = scratch();
        std::fs::write(exe.join(PORTABLE_MARKER), "").unwrap();
        let roaming = scratch();

        assert_eq!(
            state_dir(Some(&exe), Some(&roaming)),
            exe.join("data"),
            "a portable copy must keep its state with it"
        );
        std::fs::remove_dir_all(&exe).ok();
        std::fs::remove_dir_all(&roaming).ok();
    }

    /// The installed build must never write beside its executable: that is
    /// Program Files, and the marker is the only thing that says otherwise.
    #[test]
    fn without_the_marker_the_state_stays_in_the_profile() {
        let exe = scratch();
        let roaming = scratch();

        assert_eq!(
            state_dir(Some(&exe), Some(&roaming)),
            roaming.join(PRODUCT_DIR)
        );
        std::fs::remove_dir_all(&exe).ok();
        std::fs::remove_dir_all(&roaming).ok();
    }

    /// A directory of that name is not the marker; only a file is. Otherwise
    /// someone's `portable.txt/` folder would silently relocate everything.
    #[test]
    fn a_directory_named_like_the_marker_does_not_count() {
        let exe = scratch();
        std::fs::create_dir_all(exe.join(PORTABLE_MARKER)).unwrap();
        let roaming = scratch();

        assert_eq!(
            state_dir(Some(&exe), Some(&roaming)),
            roaming.join(PRODUCT_DIR)
        );
        std::fs::remove_dir_all(&exe).ok();
        std::fs::remove_dir_all(&roaming).ok();
    }

    #[test]
    fn with_no_profile_to_write_to_it_falls_back_rather_than_failing() {
        let exe = scratch();
        assert_eq!(
            state_dir(Some(&exe), None),
            std::env::temp_dir().join(PRODUCT_DIR)
        );
        std::fs::remove_dir_all(&exe).ok();
    }

    #[test]
    fn the_tray_offers_to_collapse_only_the_full_panel() {
        assert!(showing_panel(true, false));
        assert!(!showing_panel(true, true), "a bar is already collapsed");
    }

    /// The state that a two-way toggle gets wrong: nothing on screen. Offering
    /// "collapse to bar" there does nothing anyone can see.
    #[test]
    fn a_hidden_window_is_offered_open_rather_than_collapse() {
        assert!(!showing_panel(false, false));
        assert!(!showing_panel(false, true));
    }
}
