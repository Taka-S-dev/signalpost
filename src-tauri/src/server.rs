//! The local endpoint Claude Code's HTTP hooks post to.
//!
//! `POST /hook/permission` is the interesting one: it deliberately does not
//! answer until the user clicks. Claude Code allows a hook 600s, so parking
//! the response there is what lets an approval be made from this app instead
//! of from the editor window.

use axum::{extract::State, routing::{get, post}, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::model::{
    now_ms, one_line, project_name, signature_of, summarize, Agent, CodexPayload, Decision, Item,
    ItemKind, NotificationPayload, PermissionPayload, SessionPayload,
};
use crate::sessions::SessionState;
use crate::state::AppState;

pub const DEFAULT_PORT: u16 = 8787;

/// Stay comfortably inside Claude Code's 600s hook timeout, so a request we
/// gave up on is one *we* released rather than one that was cut off.
const HOLD_TIMEOUT: Duration = Duration::from_secs(570);

/// A turn ending can be reported by both `Stop` and `agent_completed`; within
/// this window the second one is treated as the same event.
const COMPLETION_WINDOW_MS: u64 = 4000;

/// Builds the response Claude Code accepts as a permission verdict.
///
/// The shape is fussy and the published docs disagree with the implementation:
/// the verdict must sit at `hookSpecificOutput.decision` as an *object* with a
/// `behavior` field, and the denial text is `message`, not `reason`. A bare
/// top-level `decision` string is read as nothing at all, which looks exactly
/// like the app being ignored.
fn verdict(behavior: &str, message: Option<&str>) -> Json<Value> {
    let mut decision = json!({ "behavior": behavior });
    if let Some(message) = message {
        decision["message"] = json!(message);
    }
    Json(json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision,
        }
    }))
}

pub fn port() -> u16 {
    std::env::var("SIGNALPOST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Records that a hook arrived, so the setup screen can tell "configured"
/// from "configured and actually firing".
async fn note(state: &Arc<AppState>) {
    state.note_hook();
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/hook/permission", post(permission))
        .route("/hook/notification", post(notification))
        .route("/hook/tool-settled", post(tool_settled))
        .route("/hook/codex", post(codex))
        .route("/hook/turn-start", post(turn_start))
        .route("/hook/turn-end", post(turn_end))
        .route("/hook/session-end", post(session_end))
        .with_state(state)
}

/// Removes a parked row if its request is abandoned — the connection drops,
/// or the handler is cancelled. Approving in the editor leaves the row with
/// nothing to answer, and a row that cannot be acted on must not be shown.
struct Parked {
    state: Arc<AppState>,
    id: String,
    answered: bool,
}

impl Drop for Parked {
    fn drop(&mut self) {
        if !self.answered {
            self.state.dismiss(&self.id);
        }
    }
}

pub async fn serve(state: Arc<AppState>) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", port());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router(state)).await
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "app": "Signalpost" }))
}

async fn permission(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PermissionPayload>,
) -> Json<Value> {
    note(&state).await;
    let (summary, detail, detail_kind) = summarize(&payload.tool_name, &payload.tool_input);
    let mut item = Item {
        id: Uuid::new_v4().to_string(),
        kind: ItemKind::Permission,
        agent: Agent::Claude,
        session_id: payload.session_id,
        project: project_name(&payload.cwd),
        cwd: payload.cwd,
        signature: signature_of(&payload.tool_name, &payload.tool_input),
        tool_name: payload.tool_name,
        summary,
        detail,
        detail_kind: detail_kind.to_string(),
        risk: None,
        label: String::new(),
        color: String::new(),
        created_at: now_ms(),
    };
    state.decorate(&mut item);

    if state.auto_allows(&item) {
        return verdict("allow", Some("Signalpost auto-allow rule"));
    }

    let id = item.id.clone();
    let waiter = state.enqueue_permission(item);
    let mut parked = Parked {
        state: state.clone(),
        id,
        answered: false,
    };

    let outcome = tokio::time::timeout(HOLD_TIMEOUT, waiter).await;
    parked.answered = matches!(outcome, Ok(Ok(_)));

    match outcome {
        Ok(Ok(Decision::Allow)) => verdict("allow", None),
        Ok(Ok(Decision::Deny)) => verdict("deny", Some("Denied in Signalpost")),
        // Timed out, or the row vanished. Returning no decision hands the
        // prompt back to the editor rather than answering for the user.
        _ => Json(json!({})),
    }
}

/// `PostToolUse` / `PermissionDenied`: the call was settled without us, so the
/// row it is still blocking on is stale.
async fn tool_settled(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PermissionPayload>,
) -> Json<Value> {
    let signature = signature_of(&payload.tool_name, &payload.tool_input);
    state.drop_settled(&payload.session_id, &signature);
    Json(json!({}))
}

/// Codex CLI's `notify`, forwarded by the shim.
///
/// Codex has no hook that can answer an approval, so these rows are purely
/// informational — the value is having Claude and Codex sessions in one list
/// rather than two places to watch.
async fn codex(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CodexPayload>,
) -> Json<Value> {
    if payload.kind != "agent-turn-complete" {
        return Json(json!({}));
    }

    // Prefer what was asked over what was answered: the prompt identifies the
    // turn, while the reply is usually a wall of prose.
    let summary = payload
        .input_messages
        .first()
        .filter(|m| !m.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| payload.last_message.clone());

    let session_id = if payload.thread_id.is_empty() {
        payload.turn_id.clone()
    } else {
        payload.thread_id.clone()
    };

    let mut item = Item {
        id: Uuid::new_v4().to_string(),
        kind: ItemKind::Completed,
        agent: Agent::Codex,
        session_id: format!("codex:{session_id}"),
        project: project_name(&payload.cwd),
        cwd: payload.cwd.clone(),
        tool_name: String::new(),
        summary: one_line(&summary, 120),
        detail: (!payload.last_message.trim().is_empty()).then(|| payload.last_message.clone()),
        detail_kind: "text".to_string(),
        risk: None,
        signature: String::new(),
        label: String::new(),
        color: String::new(),
        created_at: now_ms(),
    };
    state.decorate(&mut item);
    let session_id = item.session_id.clone();
    let cwd = item.cwd.clone();
    state.push_info(item);
    state.mark_session(&session_id, &cwd, SessionState::Idle);

    Json(json!({}))
}

/// `UserPromptSubmit`: the session started working. This is the only signal
/// that a busy session exists at all — it says nothing else until it needs
/// something.
async fn turn_start(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SessionPayload>,
) -> Json<Value> {
    note(&state).await;
    // A new turn makes any "finished" row for this session out of date.
    state.session_active(&payload.session_id);
    state.mark_session(&payload.session_id, &payload.cwd, SessionState::Running);
    Json(json!({}))
}

/// `Stop`: the turn is over, so nothing still parked can be acted on.
///
/// This is also the only guaranteed "finished" signal. `agent_completed` is a
/// notification that may or may not fire, so relying on it alone left a
/// finished turn with no row, no sound and no toast — silent.
async fn turn_end(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SessionPayload>,
) -> Json<Value> {
    note(&state).await;
    state.drop_pending_for_session(&payload.session_id);
    state.mark_session(&payload.session_id, &payload.cwd, SessionState::Idle);

    if !state.completed_recently(&payload.session_id, COMPLETION_WINDOW_MS) {
        let mut item = Item {
            id: Uuid::new_v4().to_string(),
            kind: ItemKind::Completed,
            agent: Agent::Claude,
            session_id: payload.session_id.clone(),
            project: project_name(&payload.cwd),
            cwd: payload.cwd.clone(),
            tool_name: String::new(),
            summary: String::new(),
            detail: None,
            detail_kind: "text".to_string(),
            risk: None,
            signature: String::new(),
            label: String::new(),
            color: String::new(),
            created_at: now_ms(),
        };
        state.decorate(&mut item);
        state.push_info(item);
    }
    Json(json!({}))
}

async fn notification(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NotificationPayload>,
) -> Json<Value> {
    note(&state).await;
    let kind = match payload.notification_type.as_str() {
        "agent_needs_input" | "idle_prompt" => ItemKind::NeedsInput,
        "agent_completed" => ItemKind::Completed,
        // permission_prompt fires alongside PermissionRequest for the same
        // tool call. Showing it too would put an unanswerable copy of every
        // approval in the list, right next to the real one.
        _ => return Json(json!({})),
    };

    // `Stop` may have reported the same turn a moment ago.
    if kind == ItemKind::Completed
        && state.completed_recently(&payload.session_id, COMPLETION_WINDOW_MS)
    {
        return Json(json!({}));
    }

    let mut item = Item {
        id: Uuid::new_v4().to_string(),
        kind,
        agent: Agent::Claude,
        session_id: payload.session_id,
        project: project_name(&payload.cwd),
        cwd: payload.cwd,
        tool_name: String::new(),
        summary: crate::model::one_line(&payload.message, 120),
        detail: None,
        detail_kind: "text".to_string(),
        risk: None,
        signature: String::new(),
        label: String::new(),
        color: String::new(),
        created_at: now_ms(),
    };
    state.decorate(&mut item);
    state.push_info(item);

    Json(json!({}))
}

async fn session_end(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SessionPayload>,
) -> Json<Value> {
    state.drop_session(&payload.session_id);
    state.end_session(&payload.session_id);
    Json(json!({}))
}
