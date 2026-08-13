//! What every known session is doing right now.
//!
//! The inbox only knows about sessions that have asked for something. A
//! session that has been grinding away for twenty minutes says nothing, and
//! is therefore invisible — which is exactly when you want to know about it.

use serde::Serialize;
use std::collections::HashMap;

use crate::model::now_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    /// Working on a turn.
    Running,
    /// Blocked on the user — an approval or a question.
    Waiting,
    /// Turn finished; nothing to do.
    Idle,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    pub cwd: String,
    pub label: String,
    pub color: String,
    pub state: SessionState,
    /// Epoch milliseconds the current state began, so the UI can show how
    /// long it has been in it.
    pub since: u64,
}

#[derive(Debug, Default)]
pub struct Sessions {
    known: HashMap<String, Session>,
}

impl Sessions {
    /// Records a state, keeping `since` when the state has not changed so the
    /// elapsed time keeps counting from when the session actually entered it.
    pub fn mark(&mut self, session_id: &str, cwd: &str, state: SessionState) {
        if session_id.is_empty() {
            return;
        }
        match self.known.get_mut(session_id) {
            Some(existing) => {
                if existing.state != state {
                    existing.state = state;
                    existing.since = now_ms();
                }
                if !cwd.is_empty() {
                    existing.cwd = cwd.to_string();
                }
            }
            None => {
                self.known.insert(
                    session_id.to_string(),
                    Session {
                        session_id: session_id.to_string(),
                        cwd: cwd.to_string(),
                        label: String::new(),
                        color: String::new(),
                        state,
                        since: now_ms(),
                    },
                );
            }
        }
    }

    pub fn remove(&mut self, session_id: &str) {
        self.known.remove(session_id);
    }

    /// Busiest first: what is blocked matters more than what is running, and
    /// what is running matters more than what has finished. Within a state,
    /// the one that has been there longest is on top.
    pub fn list(&self) -> Vec<Session> {
        let mut list: Vec<Session> = self.known.values().cloned().collect();
        list.sort_by_key(|s| {
            let rank = match s.state {
                SessionState::Waiting => 0,
                SessionState::Running => 1,
                SessionState::Idle => 2,
            };
            (rank, s.since)
        });
        list
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        self.known.values_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_marking_the_same_state_does_not_restart_the_clock() {
        let mut sessions = Sessions::default();
        sessions.mark("a", "C:/work/app", SessionState::Running);
        let first = sessions.list()[0].since;

        sessions.mark("a", "C:/work/app", SessionState::Running);
        assert_eq!(sessions.list()[0].since, first);
    }

    #[test]
    fn blocked_sessions_sort_above_running_ones() {
        let mut sessions = Sessions::default();
        sessions.mark("busy", "C:/a", SessionState::Running);
        sessions.mark("done", "C:/b", SessionState::Idle);
        sessions.mark("blocked", "C:/c", SessionState::Waiting);

        let listed = sessions.list();
        let order: Vec<&str> = listed.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(order, vec!["blocked", "busy", "done"]);
    }

    #[test]
    fn a_session_without_an_id_is_ignored() {
        let mut sessions = Sessions::default();
        sessions.mark("", "C:/a", SessionState::Running);
        assert!(sessions.list().is_empty());
    }
}
