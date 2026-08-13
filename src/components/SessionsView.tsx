import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Session, type WindowEntry } from "../api";
import { useT } from "../i18n";
import { elapsed, useTick } from "../useInbox";

const MARK: Record<Session["state"], string> = {
  running: "●",
  waiting: "⏳",
  idle: "✓",
};

/**
 * Where everything is and what it is doing.
 *
 * The inbox only shows sessions that have asked for something, so a session
 * working quietly for twenty minutes is invisible there. This is the view
 * that answers "is that one still going?" — and the plain window list below
 * it covers whatever the hooks do not know about.
 */
export function SessionsView() {
  const t = useT();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [windows, setWindows] = useState<WindowEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  useTick();

  const refresh = useCallback(() => {
    void api.listSessions().then(setSessions);
    void api.listWindows().then(setWindows);
  }, []);

  useEffect(() => {
    refresh();
    const changed = listen<Session[]>("sessions:changed", (e) => setSessions(e.payload));
    // Windows open and close while the panel is up, so a stale list would
    // send clicks to handles that no longer exist.
    const timer = setInterval(refresh, 3000);
    return () => {
      clearInterval(timer);
      void changed.then((un) => un());
    };
  }, [refresh]);

  const focusWindow = (entry: WindowEntry) => {
    api.focusWindow(entry.handle).catch((e) => {
      setError(String(e));
      refresh();
    });
  };

  return (
    <div className="windows">
      <div className="windows-head">
        <h2>{t.sessions.title}</h2>
        <button onClick={refresh}>{t.windows.refresh}</button>
      </div>
      {error && <p className="note status warn">{error}</p>}

      {sessions.length === 0 ? (
        <p className="note">{t.sessions.empty}</p>
      ) : (
        <ul>
          {sessions.map((session) => (
            <li key={session.sessionId}>
              <button
                className={`window-row session-${session.state}`}
                style={{ borderLeftColor: session.color }}
                onClick={() => api.focusSession(session.cwd)}
              >
                <span className="mark">{MARK[session.state]}</span>
                <span className="window-title" style={{ color: session.color }}>
                  {session.label}
                </span>
                <span className="state">{t.sessions[session.state]}</span>
                <span className="elapsed">{elapsed(session.since, t.time)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <h2 className="secondary">{t.windows.title}</h2>
      {windows.length === 0 ? (
        <p className="note">{t.windows.empty}</p>
      ) : (
        <ul>
          {windows.map((entry) => (
            <li key={entry.handle}>
              <button className="window-row" onClick={() => focusWindow(entry)}>
                <span className={`tag tag-${entry.kind}`}>{t.windows[entry.kind]}</span>
                <span className="window-title">{entry.title}</span>
                {entry.minimized && <span className="badge">{t.windows.minimized}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
