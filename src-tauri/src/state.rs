//! The inbox queue.
//!
//! A permission item is not just a row in a list: the HTTP request that
//! created it is still open, parked on a [`oneshot`] channel. Resolving the
//! row is what finally answers Claude Code.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::model::{Decision, Item, ItemKind};
use crate::profiles::{profiles_path, Profiles, ResolvedProfile};
use crate::risk::{risk_path, RiskRule, RiskRules};
use crate::rules::{Rule, RuleView, Rules, Scope};
use crate::sessions::{Session, SessionState, Sessions};
use crate::settings::{settings_path, Settings};

/// Where the user last put the panel. Absent until they move it themselves,
/// which is what distinguishes "docked by default" from "placed on purpose".
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What answering a row did, beyond answering it.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveOutcome {
    pub resolved: bool,
    /// A standing rule was created, so the UI can offer to take it back.
    pub rule_added: bool,
    pub rule_label: Option<String>,
}

/// Matches `minWidth`/`minHeight` in tauri.conf.json.
const MIN_WIDTH: f64 = 320.0;
const MIN_HEIGHT: f64 = 260.0;
pub const DEFAULT_WIDTH: f64 = 400.0;
pub const DEFAULT_HEIGHT: f64 = 620.0;

impl Geometry {
    /// Dragging the panel between monitors with different scaling can produce
    /// a size measured against a scale factor that is still changing.
    ///
    /// The window cannot be resized below its minimum by hand, so a size under
    /// it is not a preference the user expressed — it is a corrupt reading,
    /// and the default is a better answer than clamping to a size nobody
    /// chose. The position is kept either way.
    pub fn sane(self) -> Self {
        let usable = (MIN_WIDTH..=1600.0).contains(&self.width)
            && (MIN_HEIGHT..=2400.0).contains(&self.height);
        if usable {
            return self;
        }
        Geometry {
            x: self.x,
            y: self.y,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

#[derive(Default)]
struct Inner {
    items: Vec<Item>,
    waiters: HashMap<String, oneshot::Sender<Decision>>,
}

pub struct AppState {
    inner: Mutex<Inner>,
    rules: Mutex<Rules>,
    rules_path: PathBuf,
    geometry_path: PathBuf,
    geometry: Mutex<Option<Geometry>>,
    suppress_until: Mutex<u64>,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    profiles_path: PathBuf,
    profiles: Mutex<Profiles>,
    risk_path: PathBuf,
    risk: Mutex<RiskRules>,
    tray_strings: Mutex<crate::ui::TrayStrings>,
    snooze_until: Mutex<u64>,
    tray_badge: Mutex<Option<[u8; 4]>>,
    pill: Mutex<bool>,
    peeking: Mutex<bool>,
    active_shortcut: Mutex<Option<String>>,
    last_hook_at: Mutex<Option<u64>>,
    sessions: Mutex<Sessions>,
    app: AppHandle,
}

/// Files carried over when the app identifier changes.
const CONFIG_FILES: [&str; 5] = [
    "auto-allow.json",
    "projects.json",
    "risk.json",
    "settings.json",
    "window.json",
];

/// Copies configuration left behind by a previous app identifier.
///
/// The config directory is derived from the identifier, so renaming the app
/// silently moves it — every auto-allow rule, project colour and preference
/// would look deleted. Runs once: anything already in the new directory wins.
fn migrate_config(config_dir: &Path) {
    let Some(parent) = config_dir.parent() else { return };
    let old = parent.join("com.claudenotify.app");
    if !old.is_dir() || config_dir.join("settings.json").exists() {
        return;
    }
    if std::fs::create_dir_all(config_dir).is_err() {
        return;
    }
    for name in CONFIG_FILES {
        let from = old.join(name);
        if from.is_file() {
            let _ = std::fs::copy(&from, config_dir.join(name));
        }
    }
}

impl AppState {
    pub fn new(app: AppHandle, config_dir: PathBuf) -> Self {
        migrate_config(&config_dir);
        let rules_path = crate::rules::rules_path(&config_dir);
        let geometry_path = config_dir.join("window.json");
        let rules = Rules::load(&rules_path);
        let geometry = std::fs::read_to_string(&geometry_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Geometry>(&raw).ok())
            .map(Geometry::sane);
        let settings_path = settings_path(&config_dir);
        let settings = Settings::load(&settings_path).sanitized();
        AppState {
            inner: Mutex::new(Inner::default()),
            rules: Mutex::new(rules),
            rules_path,
            geometry_path,
            geometry: Mutex::new(geometry),
            suppress_until: Mutex::new(0),
            settings_path,
            settings: Mutex::new(settings),
            profiles_path: profiles_path(&config_dir),
            profiles: Mutex::new(Profiles::load(&profiles_path(&config_dir))),
            risk_path: risk_path(&config_dir),
            risk: Mutex::new(RiskRules::load(&risk_path(&config_dir))),
            tray_strings: Mutex::new(crate::ui::TrayStrings::default()),
            snooze_until: Mutex::new(0),
            // None means "not drawn yet", so the first sync always paints.
            tray_badge: Mutex::new(Some([0, 0, 0, 0])),
            pill: Mutex::new(false),
            peeking: Mutex::new(false),
            active_shortcut: Mutex::new(None),
            last_hook_at: Mutex::new(None),
            sessions: Mutex::new(Sessions::default()),
            app,
        }
    }

    /// Records what a session is doing and pushes the new list to the UI.
    pub fn mark_session(&self, session_id: &str, cwd: &str, state: SessionState) {
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.mark(session_id, cwd, state);
            let profiles = self.profiles.lock().unwrap();
            for session in sessions.values_mut() {
                session.label = profiles.label(&session.cwd);
                session.color = profiles.color(&session.cwd);
            }
        }
        self.announce_sessions();
    }

    pub fn end_session(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
        self.announce_sessions();
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.sessions.lock().unwrap().list()
    }

    fn announce_sessions(&self) {
        let _ = self.app.emit("sessions:changed", self.sessions());
    }

    /// Stops new rows from raising the panel for a while.
    ///
    /// Deliberately time-boxed and not persisted: a suppression you can forget
    /// about is how a notifier ends up silently useless, and restarting the app
    /// should never leave one in force.
    pub fn snooze(&self, minutes: u64) -> u64 {
        let until = crate::model::now_ms() + minutes * 60_000;
        *self.snooze_until.lock().unwrap() = until;
        self.announce_snooze();
        until
    }

    pub fn clear_snooze(&self) {
        *self.snooze_until.lock().unwrap() = 0;
        self.announce_snooze();
    }

    /// When the suppression ends, or `None` once it has.
    pub fn snoozed_until(&self) -> Option<u64> {
        let until = *self.snooze_until.lock().unwrap();
        (until > crate::model::now_ms()).then_some(until)
    }

    fn announce_snooze(&self) {
        let _ = self.app.emit("snooze:changed", self.snoozed_until());
    }

    /// True when a finished-turn row for this session arrived moments ago.
    ///
    /// A turn ending can be reported twice — `Stop` always fires, and
    /// `agent_completed` sometimes does — and neither alone is reliable. Both
    /// are accepted and the duplicate is dropped, so a completion is never
    /// silent and never doubled.
    pub fn completed_recently(&self, session_id: &str, within_ms: u64) -> bool {
        let now = crate::model::now_ms();
        self.inner.lock().unwrap().items.iter().any(|i| {
            i.kind == ItemKind::Completed
                && i.session_id == session_id
                && now.saturating_sub(i.created_at) < within_ms
        })
    }

    /// Records that a hook reached us. Distinguishes "configured" from
    /// "configured and actually in effect".
    pub fn note_hook(&self) {
        *self.last_hook_at.lock().unwrap() = Some(crate::model::now_ms());
    }

    pub fn last_hook_at(&self) -> Option<u64> {
        *self.last_hook_at.lock().unwrap()
    }

    /// True while the panel is open only because the pointer is on it, so
    /// moving away should put it back.
    pub fn set_peeking(&self, peeking: bool) {
        *self.peeking.lock().unwrap() = peeking;
    }

    /// Which global shortcut actually registered, if any.
    pub fn set_active_shortcut(&self, shortcut: Option<String>) {
        *self.active_shortcut.lock().unwrap() = shortcut;
    }

    pub fn active_shortcut(&self) -> Option<String> {
        self.active_shortcut.lock().unwrap().clone()
    }

    pub fn is_peeking(&self) -> bool {
        *self.peeking.lock().unwrap()
    }


    pub fn set_pill(&self, pill: bool) {
        *self.pill.lock().unwrap() = pill;
    }

    /// While collapsed, the window's size is the app's choice rather than the
    /// user's, so it must never overwrite the remembered geometry.
    pub fn is_pill(&self) -> bool {
        *self.pill.lock().unwrap()
    }

    /// True when the tray badge differs from what is already drawn, so the
    /// icon is only rebuilt when it would actually look different.
    pub fn tray_badge_changed(&self, dot: Option<[u8; 4]>) -> bool {
        let mut current = self.tray_badge.lock().unwrap();
        if *current == dot {
            return false;
        }
        *current = dot;
        true
    }

    pub fn tray_strings(&self) -> crate::ui::TrayStrings {
        self.tray_strings.lock().unwrap().clone()
    }

    pub fn set_tray_strings(&self, strings: crate::ui::TrayStrings) {
        *self.tray_strings.lock().unwrap() = strings;
        crate::ui::sync(&self.app, &self.items(), None);
    }

    pub fn risk_rules(&self) -> Vec<RiskRule> {
        self.risk.lock().unwrap().list().to_vec()
    }

    pub fn set_risk_rules(&self, rules: Vec<RiskRule>) -> Vec<RiskRule> {
        let mut risk = self.risk.lock().unwrap();
        risk.replace(rules);
        risk.save(&self.risk_path);
        let listed = risk.list().to_vec();
        drop(risk);
        self.restamp();
        listed
    }

    pub fn restore_risk_defaults(&self) -> Vec<RiskRule> {
        let mut risk = self.risk.lock().unwrap();
        risk.restore_defaults();
        risk.save(&self.risk_path);
        let listed = risk.list().to_vec();
        drop(risk);
        self.restamp();
        listed
    }

    /// Stamps a row with its project's name and colour, and records that the
    /// project was seen so it can be named later without hunting for the path.
    pub fn decorate(&self, item: &mut Item) {
        let mut profiles = self.profiles.lock().unwrap();
        if profiles.touch(&item.cwd) {
            profiles.save(&self.profiles_path);
        }
        item.label = profiles.label(&item.cwd);
        item.color = profiles.color(&item.cwd);
        drop(profiles);

        // Match against the signature *and* the summary, so a risky argument
        // is caught whether or not it made it into the stable identity.
        let haystack = format!("{} {}", item.signature, item.summary);
        item.risk = self.risk.lock().unwrap().evaluate(&haystack);
    }

    pub fn projects(&self) -> Vec<ResolvedProfile> {
        self.profiles.lock().unwrap().resolved()
    }

    pub fn open_command(&self, cwd: &str) -> String {
        self.profiles.lock().unwrap().open_command(cwd)
    }

    /// True when the user chose a command for this project, which then wins
    /// over focusing whatever window happens to match the folder name.
    pub fn has_open_command(&self, cwd: &str) -> bool {
        self.profiles.lock().unwrap().has_open_command(cwd)
    }

    pub fn set_project(
        &self,
        cwd: &str,
        name: Option<String>,
        color: Option<String>,
        open_command: Option<String>,
    ) -> Vec<ResolvedProfile> {
        let mut profiles = self.profiles.lock().unwrap();
        profiles.set(cwd, name, color, open_command);
        profiles.save(&self.profiles_path);
        let resolved = profiles.resolved();
        drop(profiles);
        // Rows already on screen carry the old name, so refresh them too.
        self.restamp();
        resolved
    }

    /// Re-applies project identity and risk marks to every row, so edits to
    /// either take effect on what is already on screen.
    fn restamp(&self) {
        {
            let profiles = self.profiles.lock().unwrap();
            let risk = self.risk.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            for item in inner.items.iter_mut() {
                item.label = profiles.label(&item.cwd);
                item.color = profiles.color(&item.cwd);
                item.risk = risk.evaluate(&format!("{} {}", item.signature, item.summary));
            }
        }
        self.announce(None);
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, settings: Settings) -> Settings {
        let settings = settings.sanitized();
        settings.save(&self.settings_path);
        *self.settings.lock().unwrap() = settings.clone();
        settings
    }

    pub fn geometry(&self) -> Option<Geometry> {
        *self.geometry.lock().unwrap()
    }

    pub fn save_geometry(&self, geometry: Geometry) {
        let geometry = geometry.sane();
        *self.geometry.lock().unwrap() = Some(geometry);
        if let Some(dir) = self.geometry_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(raw) = serde_json::to_string_pretty(&geometry) {
            let _ = std::fs::write(&self.geometry_path, raw);
        }
    }

    /// Records where the window was moved to, keeping the remembered size.
    ///
    /// Used while collapsed: the bar's size is the app's choice, but where it
    /// sits is the user's, and dragging it used to be forgotten the moment it
    /// expanded again.
    pub fn save_position(&self, x: f64, y: f64) {
        let existing = self.geometry().unwrap_or(Geometry {
            x,
            y,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        });
        self.save_geometry(Geometry { x, y, ..existing });
    }

    /// Forgets the remembered placement, so the panel docks to the right edge
    /// again. The way out when a placement ends up somewhere unusable.
    pub fn reset_geometry(&self) {
        *self.geometry.lock().unwrap() = None;
        let _ = std::fs::remove_file(&self.geometry_path);
    }

    /// Suppresses geometry saves for a moment. During a monitor change Windows
    /// resizes the window itself, and those intermediate sizes are measured
    /// against a scale factor that is still changing.
    pub fn suppress_geometry_saves(&self, millis: u64) {
        *self.suppress_until.lock().unwrap() = crate::model::now_ms() + millis;
    }

    pub fn geometry_saves_suppressed(&self) -> bool {
        crate::model::now_ms() < *self.suppress_until.lock().unwrap()
    }

    /// True when a persisted rule already covers this call, in which case it
    /// must never be shown to the user.
    pub fn auto_allows(&self, item: &Item) -> bool {
        self.rules.lock().unwrap().allows(item)
    }

    /// Drops a session's finished-turn rows.
    ///
    /// "Completed" asserts that the session is done. The moment it asks for
    /// something the assertion is false, and leaving it up puts two rows on
    /// screen for one session where only the newer one is true.
    fn clear_completed_for(&self, session_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner
            .items
            .retain(|i| !(i.kind == ItemKind::Completed && i.session_id == session_id));
    }

    /// Called when a session shows any sign of being active again.
    pub fn session_active(&self, session_id: &str) {
        self.clear_completed_for(session_id);
        self.announce(None);
    }

    /// Parks a permission request. The returned receiver resolves when the
    /// user answers, or is dropped if the row disappears for any other reason.
    pub fn enqueue_permission(&self, item: Item) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        self.clear_completed_for(&item.session_id);
        {
            let mut inner = self.inner.lock().unwrap();
            inner.waiters.insert(item.id.clone(), tx);
            inner.items.push(item.clone());
        }
        self.mark_session(&item.session_id, &item.cwd, SessionState::Waiting);
        self.announce(Some(&item));
        rx
    }

    /// Adds an informational row.
    ///
    /// "Needs input" is a *state* — a session is either waiting or it is not,
    /// so a newer one replaces the old. A finished turn is an *event*: three
    /// completed turns are three things that happened, and collapsing them
    /// makes every turn after the first look like it never arrived.
    pub fn push_info(&self, item: Item) {
        // A question means the session is not finished after all.
        if item.kind != ItemKind::Completed {
            self.clear_completed_for(&item.session_id);
        }
        let mut item = item;
        {
            let mut inner = self.inner.lock().unwrap();

            // One row per session per kind, always. Three finished turns are
            // three events, but showing them as three rows buries the other
            // sessions — so the row is replaced and counts instead.
            let previous = inner
                .items
                .iter()
                .position(|i| i.session_id == item.session_id && i.kind == item.kind);
            if let Some(index) = previous {
                item.repeat = inner.items[index].repeat + 1;
                inner.items.remove(index);
            }
            inner.items.push(item.clone());
        }
        let state = match item.kind {
            ItemKind::Completed => SessionState::Idle,
            _ => SessionState::Waiting,
        };
        self.mark_session(&item.session_id, &item.cwd, state);
        self.announce(Some(&item));
    }

    /// Undoes the standing rule created by the most recent answer.
    pub fn undo_last_rule(&self) -> Vec<RuleView> {
        let mut rules = self.rules.lock().unwrap();
        rules.undo_last();
        rules.save(&self.rules_path);
        rules.list().iter().map(Rule::view).collect()
    }

    /// Answers a permission row, optionally remembering the decision.
    ///
    /// Reports whether a rule was created, so the UI can offer to take it back
    /// straight away rather than sending the user to look for it.
    pub fn resolve(&self, id: &str, decision: Decision, remember: Option<Scope>) -> ResolveOutcome {
        let (waiter, item) = {
            let mut inner = self.inner.lock().unwrap();
            let item = match inner.items.iter().position(|i| i.id == id) {
                Some(index) => inner.items.remove(index),
                None => return ResolveOutcome::default(),
            };
            (inner.waiters.remove(id), item)
        };

        let mut rule_added = false;
        if let (Some(scope), Decision::Allow) = (remember, decision) {
            let mut rules = self.rules.lock().unwrap();
            rule_added = rules.add(Rule::from_item(&item, scope));
            rules.save(&self.rules_path);
        }

        // A missing waiter means the request timed out already; the row is
        // gone either way, so the click is still worth acknowledging.
        if let Some(tx) = waiter {
            let _ = tx.send(decision);
        }
        // Answering unblocks the session, so it is working again.
        self.mark_session(&item.session_id, &item.cwd, SessionState::Running);
        self.announce(None);
        ResolveOutcome {
            resolved: true,
            rule_added,
            rule_label: rule_added.then(|| item.tool_name.clone()),
        }
    }

    /// Removes a row without answering it. Used for informational rows and
    /// for requests Claude Code has stopped waiting on.
    pub fn dismiss(&self, id: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.items.retain(|i| i.id != id);
            inner.waiters.remove(id);
        }
        self.announce(None);
    }

    /// Drops the blocked row for a call that was decided somewhere else —
    /// usually because the user answered the prompt in the editor instead.
    /// Without this the row would sit there until the hold expires.
    pub fn drop_settled(&self, session_id: &str, signature: &str) {
        let stale: Option<String> = {
            let inner = self.inner.lock().unwrap();
            inner
                .items
                .iter()
                .find(|i| {
                    i.kind == ItemKind::Permission
                        && i.session_id == session_id
                        && i.signature == signature
                })
                .map(|i| i.id.clone())
        };
        if let Some(id) = stale {
            self.dismiss(&id);
        }
    }

    /// Drops every blocked row of a session whose turn is over. Anything still
    /// waiting at that point can no longer be acted on.
    pub fn drop_pending_for_session(&self, session_id: &str) {
        let stale: Vec<String> = {
            let inner = self.inner.lock().unwrap();
            inner
                .items
                .iter()
                .filter(|i| i.kind == ItemKind::Permission && i.session_id == session_id)
                .map(|i| i.id.clone())
                .collect()
        };
        for id in stale {
            self.dismiss(&id);
        }
    }

    /// Clears the finished-turn rows in one go.
    ///
    /// Only those. A blocked call and an unanswered question are both things
    /// a session is stopped on — the fact that one is answered in the editor
    /// rather than here does not make it news, and a single key must never
    /// discard either in bulk.
    pub fn dismiss_all_info(&self) -> usize {
        let cleared = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.items.len();
            inner.items.retain(|i| i.kind != ItemKind::Completed);
            before - inner.items.len()
        };
        if cleared > 0 {
            self.announce(None);
        }
        cleared
    }

    /// Drops every row belonging to a session that has ended.
    pub fn drop_session(&self, session_id: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            let stale: Vec<String> = inner
                .items
                .iter()
                .filter(|i| i.session_id == session_id)
                .map(|i| i.id.clone())
                .collect();
            inner.items.retain(|i| i.session_id != session_id);
            for id in stale {
                inner.waiters.remove(&id);
            }
        }
        self.announce(None);
    }

    /// Rows in the order the UI renders them: blocked calls first, oldest at
    /// the top, so nothing can sit forgotten at the bottom of the list.
    pub fn items(&self) -> Vec<Item> {
        let mut items = self.inner.lock().unwrap().items.clone();
        // Blocked first, then unanswered questions — both are sessions that
        // have stopped — and finished turns last. Within a group, oldest on
        // top so nothing can sit forgotten at the bottom.
        items.sort_by_key(|i| {
            let rank = match i.kind {
                ItemKind::Permission => 0,
                ItemKind::NeedsInput => 1,
                ItemKind::Completed => 2,
            };
            (rank, i.created_at)
        });
        items
    }

    pub fn rule_views(&self) -> Vec<RuleView> {
        self.rules.lock().unwrap().list().iter().map(Rule::view).collect()
    }

    pub fn remove_rule(&self, index: usize) {
        let mut rules = self.rules.lock().unwrap();
        rules.remove(index);
        rules.save(&self.rules_path);
    }

    pub fn app(&self) -> &AppHandle {
        &self.app
    }

    /// Pushes the current snapshot to the UI and keeps the tray and the panel
    /// in sync with it.
    fn announce(&self, arrived: Option<&Item>) {
        let items = self.items();
        let _ = self.app.emit("inbox:changed", &items);
        if let Some(item) = arrived {
            let _ = self.app.emit("inbox:arrived", item);
        }
        crate::ui::sync(&self.app, &items, arrived.map(|i| i.kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(width: f64, height: f64) -> Geometry {
        Geometry { x: 100.0, y: 50.0, width, height }
    }

    #[test]
    fn a_size_the_user_could_have_chosen_is_left_alone() {
        let kept = geometry(520.0, 900.0).sane();
        assert_eq!((kept.width, kept.height), (520.0, 900.0));
    }

    #[test]
    fn a_size_below_the_minimum_falls_back_to_the_default_but_keeps_the_position() {
        // The exact reading a monitor change produced in practice.
        let healed = geometry(259.0, 344.0).sane();
        assert_eq!((healed.width, healed.height), (DEFAULT_WIDTH, DEFAULT_HEIGHT));
        assert_eq!((healed.x, healed.y), (100.0, 50.0));
    }

    #[test]
    fn an_absurdly_large_size_is_treated_as_corrupt_too() {
        let healed = geometry(9000.0, 620.0).sane();
        assert_eq!(healed.width, DEFAULT_WIDTH);
    }
}
