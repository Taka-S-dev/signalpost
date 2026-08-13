//! Panel, tray and editor-focus behaviour.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::model::{Item, ItemKind};
use crate::settings::PopupWhen;
use crate::state::AppState;

pub const PANEL_LABEL: &str = "panel";
const EDGE_MARGIN: f64 = 16.0;
/// Big enough to read across a desk, small enough to leave up.
///
/// Larger than the visible bar on purpose: the window is transparent, and the
/// spare margin is where the arrival glow is drawn. Sized to the bar exactly,
/// the glow would be clipped at the window edge and the flash would be little
/// more than a colour change on a dark desktop.
const PILL_WIDTH: f64 = 232.0;
const PILL_HEIGHT: f64 = 58.0;

/// Collapses the panel to a small always-visible strip.
///
/// The tray is not an answer on Windows 11, where icons are hidden in the
/// overflow by default — a badge nobody can see reports nothing. Something
/// that stays on screen has to be the app's own window, so the panel shrinks
/// instead of disappearing.
pub fn show_pill(app: &AppHandle) {
    let Some(window) = panel(app) else { return };
    let anchor = app
        .try_state::<Arc<AppState>>()
        .and_then(|s| s.geometry())
        .map(|g| (g.x, g.y));

    // Tell the webview first so it is already laying out the bar while the
    // window shrinks. Emitting afterwards leaves a frame of the full panel
    // crammed into a small window, which is what reads as stutter.
    let _ = app.emit("mode:changed", "pill");

    // Set before resizing: the resulting Moved/Resized events must not record
    // the pill's dimensions as the panel's remembered size, nor the collapse
    // itself as a move the user made.
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.set_pill(true);
        state.suppress_geometry_saves(600);
    }
    resize(&window, PILL_WIDTH, PILL_HEIGHT);
    if let Some((x, y)) = anchor {
        reposition(&window, x, y);
    } else {
        dock(&window);
    }
    if on_a_monitor(&window) {
        keep_on_screen(&window);
    } else {
        dock(&window);
    }
    let _ = window.show();
}

/// Collapses the panel once the pointer really has left it.
///
/// DOM `mouseleave` cannot be trusted for this: a native tooltip is a window
/// of its own, and when one opens under the cursor the webview loses the
/// pointer and reports a leave. Hovering a button with a `title` therefore
/// closed the panel. The OS cursor position has no such ambiguity.
pub fn watch_peek(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut outside = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let Some(state) = app.try_state::<Arc<AppState>>() else {
                return;
            };
            if !state.is_peeking() {
                return;
            }
            let Some(window) = panel(&app) else { return };

            match pointer_inside(&window) {
                // Unable to tell: leave the panel alone rather than guess.
                None => return,
                Some(true) => outside = 0,
                Some(false) => {
                    outside += 1;
                    // Two checks, so brushing past an edge is not a leave.
                    // The same delay whether or not anything was clicked: the
                    // pointer having left is the whole signal, and a second
                    // timing would only make the behaviour harder to predict.
                    if outside >= 2 {
                        state.set_peeking(false);
                        show_pill(&app);
                        return;
                    }
                }
            }
        }
    });
}

/// True while a mouse button is held.
///
/// Dragging the window's edge to resize it moves the cursor ahead of the
/// window, so it reads as "outside" for a moment and the panel collapsed
/// mid-resize — losing the new size. Holding a button is proof the user is
/// still working on it.
#[cfg(windows)]
fn button_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    unsafe {
        (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0
    }
}

#[cfg(not(windows))]
fn button_held() -> bool {
    false
}

fn pointer_inside(window: &WebviewWindow) -> Option<bool> {
    if button_held() {
        return Some(true);
    }
    let cursor = window.cursor_position().ok()?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(
        cursor.x >= position.x as f64
            && cursor.x < (position.x + size.width as i32) as f64
            && cursor.y >= position.y as f64
            && cursor.y < (position.y + size.height as i32) as f64,
    )
}

/// Skips the call when the window is already that size.
///
/// Every set_size is a native resize plus a repaint, so a redundant one is a
/// visible hitch rather than a no-op.
fn resize(window: &WebviewWindow, width: f64, height: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    if let Ok(size) = window.outer_size() {
        let current = size.to_logical::<f64>(scale);
        if (current.width - width).abs() < 1.0 && (current.height - height).abs() < 1.0 {
            return;
        }
    }
    let _ = window.set_size(LogicalSize::new(width, height));
}

fn reposition(window: &WebviewWindow, x: f64, y: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    if let Ok(position) = window.outer_position() {
        let current = position.to_logical::<f64>(scale);
        if (current.x - x).abs() < 1.0 && (current.y - y).abs() < 1.0 {
            return;
        }
    }
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Restores the full panel at the size and place the user chose.
pub fn expand(app: &AppHandle) {
    let Some(window) = panel(app) else { return };
    // Same reason as the collapse: the layout should already be the panel's
    // by the time the window is the panel's size.
    let _ = app.emit("mode:changed", "full");
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.set_pill(false);
    }
    place(app, &window);
    let _ = window.show();
}

/// Tray text, supplied by the frontend so every translation lives in one
/// place. The English values here are only what shows before the UI has
/// loaded and told us which language to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStrings {
    pub show: String,
    pub bar: String,
    /// Reachable from the tray because the panel is usually hidden at the
    /// moment someone wants it to stop appearing.
    pub snooze: String,
    pub unsnooze: String,
    /// The way back when the panel has ended up somewhere invisible — the
    /// in-panel reset button is unreachable exactly when it is needed.
    pub reset: String,
    pub quit: String,
    pub idle: String,
    /// Contains `{n}`, replaced with the number of blocked calls.
    pub pending: String,
}

impl Default for TrayStrings {
    fn default() -> Self {
        TrayStrings {
            show: "Open panel".into(),
            bar: "Collapse to bar".into(),
            snooze: "Stop popping up (30 min)".into(),
            unsnooze: "Pop up again".into(),
            reset: "Reset position".into(),
            quit: "Quit".into(),
            idle: "Signalpost — nothing waiting".into(),
            pending: "Signalpost — {n} waiting".into(),
        }
    }
}

pub fn panel(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(PANEL_LABEL)
}

/// Reflects the queue in the tray and the panel.
///
/// A new row shows the panel but deliberately does **not** take keyboard
/// focus: interrupting whatever the user is typing is exactly the cost this
/// app exists to remove. `Alt+Space` is how focus is claimed, on purpose.
pub fn sync(app: &AppHandle, items: &[Item], arrived: Option<ItemKind>) {
    // A question is as much "stopped, waiting on you" as a blocked call is;
    // only where it gets answered differs.
    let pending = items
        .iter()
        .filter(|i| i.kind != ItemKind::Completed)
        .count();

    badge_tray(app, pending, items.len() - pending);

    if let Some(tray) = app.tray_by_id(crate::TRAY_ID) {
        let strings = app
            .try_state::<Arc<AppState>>()
            .map(|s| s.tray_strings())
            .unwrap_or_default();
        let tip = if pending > 0 {
            strings.pending.replace("{n}", &pending.to_string())
        } else {
            strings.idle
        };
        let _ = tray.set_tooltip(Some(tip));
    }

    let state = app.try_state::<Arc<AppState>>();
    let settings = state.as_ref().map(|s| s.settings()).unwrap_or_default();
    // Rows still arrive and the tray still counts them; only the window
    // coming forward is suppressed.
    let snoozed = state.as_ref().and_then(|s| s.snoozed_until()).is_some();
    let worth_interrupting = match (arrived, settings.popup) {
        (None, _) | (_, PopupWhen::Never) => false,
        (Some(_), PopupWhen::All) => true,
        (Some(kind), PopupWhen::Permission) => kind == ItemKind::Permission,
    };

    // What does not interrupt still has to reach the user somehow. Without
    // this, a finished turn would be silent and invisible until the panel is
    // opened by hand.
    if let (Some(kind), true) = (arrived, !worth_interrupting && settings.toast) {
        if let Some(item) = items.iter().rev().find(|i| i.kind == kind) {
            toast(app, item);
        }
    }

    let Some(window) = panel(app) else { return };
    if items.is_empty() {
        // Collapsing when the queue empties is the frontend's call: only it
        // knows whether the user is looking at the inbox or is halfway
        // through the settings.
        if settings.auto_hide && !settings.keep_open {
            let _ = window.hide();
        }
    } else if worth_interrupting && !snoozed {
        expand(app);
    } else if arrived.is_some() && !window.is_visible().unwrap_or(false) {
        // Not worth taking over the screen, but too easy to miss if nothing
        // is left behind.
        show_pill(app);
    }
}

/// Restores where the user put the panel, or docks it if they never have.
///
/// A remembered position can end up off every monitor — a display gets
/// disconnected, or the position was recorded against a different scale
/// factor. The panel would then be "shown" somewhere nobody can see, and the
/// reset button lives inside it, so the placement is verified rather than
/// trusted.
pub fn place(app: &AppHandle, window: &WebviewWindow) {
    // Everything below moves the window on the app's initiative. The Moved
    // events that follow must not be mistaken for the user choosing a new
    // spot — otherwise nudging the panel on-screen quietly overwrites where
    // the bar was put, and it creeps up the display every time it opens.
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.suppress_geometry_saves(600);
    }

    if let Some(saved) = app.try_state::<Arc<AppState>>().and_then(|s| s.geometry()) {
        resize(window, saved.width, saved.height);
        reposition(window, saved.x, saved.y);
        if on_a_monitor(window) {
            keep_on_screen(window);
            return;
        }
    }
    dock(window);
}

/// Slides the window back inside its monitor when it would hang off an edge.
///
/// The panel keeps the bar's top-left corner when it expands, so a bar parked
/// near the bottom of the screen would grow straight off it — the natural
/// place to leave a status bar is exactly where this breaks. Growing upwards
/// instead is what the geometry has to say.
fn keep_on_screen(window: &WebviewWindow) {
    let (Ok(Some(monitor)), Ok(position), Ok(size)) = (
        window.current_monitor(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        return;
    };

    let scale = monitor.scale_factor();
    // The work area, not the monitor: clamping to the full screen tucks the
    // bar under the taskbar, which is where it is most likely to be parked.
    let work = monitor.work_area();
    let origin = work.position.to_logical::<f64>(scale);
    let area = work.size.to_logical::<f64>(scale);
    let here = position.to_logical::<f64>(scale);
    let extent = size.to_logical::<f64>(scale);

    // A window taller or wider than the screen can only be aligned to the
    // near edge; clamping both would fight itself.
    let fit = |pos: f64, start: f64, span: f64, size: f64| {
        let low = start + EDGE_MARGIN;
        let high = start + span - size - EDGE_MARGIN;
        if high < low {
            low
        } else {
            pos.clamp(low, high)
        }
    };

    reposition(
        window,
        fit(here.x, origin.x, area.width, extent.width),
        fit(here.y, origin.y, area.height, extent.height),
    );
}

/// True when enough of the panel's top edge sits inside some monitor to grab.
fn on_a_monitor(window: &WebviewWindow) -> bool {
    let (Ok(monitors), Ok(position), Ok(size)) = (
        window.available_monitors(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        // Without an answer, assume it is fine rather than fighting the user's
        // placement on every appearance.
        return true;
    };
    if monitors.is_empty() {
        return true;
    }

    let grab_x = position.x + (size.width as i32 / 2);
    let grab_y = position.y + 16;
    monitors.iter().any(|monitor| {
        let origin = monitor.position();
        let extent = monitor.size();
        grab_x >= origin.x
            && grab_x < origin.x + extent.width as i32
            && grab_y >= origin.y
            && grab_y < origin.y + extent.height as i32
    })
}

/// Marks the tray icon with a dot for what is waiting.
///
/// A toast and a sound both pass: if you were looking elsewhere, they are
/// gone. The tray is the only thing on screen the whole time, so it is the
/// only place a signal can wait for you to notice it.
fn badge_tray(app: &AppHandle, pending: usize, info: usize) {
    // Amber reads as "something is blocked on you"; blue as "there is news".
    let dot = match (pending, info) {
        (0, 0) => None,
        (0, _) => Some([122u8, 162, 247, 255]),
        _ => Some([232u8, 192, 125, 255]),
    };

    // Repainting on every event would be wasted work; only a change matters.
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        if !state.tray_badge_changed(dot) {
            return;
        }
    }

    let Some(tray) = app.tray_by_id(crate::TRAY_ID) else {
        return;
    };
    let Some(base) = app.default_window_icon() else {
        return;
    };
    match dot {
        None => {
            let _ = tray.set_icon(Some(base.clone()));
        }
        Some(color) => {
            let (w, h) = (base.width(), base.height());
            let mut rgba = base.rgba().to_vec();
            draw_dot(&mut rgba, w, h, color);
            let _ = tray.set_icon(Some(tauri::image::Image::new_owned(rgba, w, h)));
        }
    }
}

/// Fills a circle in the lower-right corner, ringed in near-black so it stays
/// legible on both light and dark taskbars.
fn draw_dot(rgba: &mut [u8], width: u32, height: u32, color: [u8; 4]) {
    let radius = (width.min(height) as f32) * 0.30;
    let cx = width as f32 - radius - 1.0;
    let cy = height as f32 - radius - 1.0;
    let ring = radius * 0.78;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius {
                continue;
            }
            let pixel = ((y * width + x) * 4) as usize;
            let paint = if distance > ring {
                [10, 12, 16, 255]
            } else {
                color
            };
            rgba[pixel..pixel + 4].copy_from_slice(&paint);
        }
    }
}

/// A system notification for a row that is not worth raising the panel for.
///
/// The project name leads, because when several sessions are running the
/// first question is always which one this is. Clicking it goes to that
/// session's window — a notification you cannot act on is just a smaller
/// interruption, not a lesser one.
#[cfg(windows)]
fn toast(app: &AppHandle, item: &Item) {
    use tauri_winrt_notification::{Duration, Toast};

    let label = if item.label.is_empty() {
        &item.project
    } else {
        &item.label
    };
    let handle = app.clone();
    let cwd = item.cwd.clone();
    let project = item.project.clone();

    let build = |app_id: &str| {
        let handle = handle.clone();
        let cwd = cwd.clone();
        let project = project.clone();
        Toast::new(app_id)
            .title(label)
            .text1(&item.summary)
            .duration(Duration::Short)
            .on_activated(move |_| {
                if let Some(state) = handle.try_state::<Arc<AppState>>() {
                    let _ = crate::go_to(&state, &cwd, &project);
                }
                Ok(())
            })
    };

    // An unregistered app id silently fails, which is what happens while
    // running unpackaged. PowerShell's id always resolves, so a dev build
    // still shows something rather than nothing.
    if build(&app.config().identifier).show().is_err() {
        let _ = build(Toast::POWERSHELL_APP_ID).show();
    }
}

#[cfg(not(windows))]
fn toast(_app: &AppHandle, _item: &Item) {}

/// Docks the panel to the right edge of the monitor it is on.
fn dock(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    // Same reason as the clamp: the taskbar is not usable space.
    let work = monitor.work_area();
    let screen = work.size.to_logical::<f64>(scale);
    let origin = work.position.to_logical::<f64>(scale);
    let Ok(size) = window.outer_size() else {
        return;
    };
    let size = size.to_logical::<f64>(scale);

    let x = origin.x + screen.width - size.width - EDGE_MARGIN;
    let y = origin.y + (screen.height - size.height).max(0.0) / 2.0;
    reposition(window, x, y);
}

/// `Alt+Space` and the tray both land here.
pub fn toggle(app: &AppHandle) {
    let Some(window) = panel(app) else { return };
    if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
        let _ = window.hide();
    } else {
        reveal(app);
    }
}

pub fn reveal(app: &AppHandle) {
    expand(app);
    let Some(window) = panel(app) else { return };
    let _ = window.set_focus();
}

/// Runs the project's "bring this session's window forward" command.
///
/// It goes through a shell because the useful commands are shell shims
/// (`code`) or take quoted arguments (`wt -d "..."`).
pub fn run_open_command(command_line: &str) -> Result<(), String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command_line]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command_line]);
        c
    };
    // Going through a shell means spawning always succeeds, even when the
    // program does not exist. Its complaint is the only evidence of that, so
    // keep it instead of letting it vanish into a hidden console.
    command.stderr(Stdio::piped()).stdout(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("could not run `{command_line}`: {e}"))?;

    // Launchers return immediately. Anything still alive after this is doing
    // real work and is not worth blocking the UI on.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut message);
                }
                let message = message.trim();
                let detail = if message.is_empty() {
                    "the command failed".to_string()
                } else {
                    message.to_string()
                };
                return Err(format!("`{command_line}`: {detail}"));
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => return Ok(()),
        }
    }
}
