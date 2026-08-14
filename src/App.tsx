import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  DEFAULT_SETTINGS,
  type Decision,
  type HookStatus,
  type Scope,
  type Settings,
} from "./api";
import { dictionary, I18nContext, type Dictionary } from "./i18n";
import { ItemRow } from "./components/ItemRow";
import { Pill } from "./components/Pill";
import { ProjectsView } from "./components/ProjectsView";
import { RulesView } from "./components/RulesView";
import { SessionsView } from "./components/SessionsView";
import { Setup } from "./components/Setup";
import { useInbox, useTick } from "./useInbox";
import "./styles.css";

type View = "inbox" | "windows" | "projects" | "rules" | "setup";

// Every tab carries an icon, because narrow panels hide the labels and a
// row of bare shortcut letters names nothing. The inbox has no key of its
// own: it is where Esc and answering a row already put you.
const TABS: { view: View; name: keyof Dictionary["nav"]; icon: string; key?: string }[] = [
  { view: "inbox", name: "inbox", icon: "▤" },
  { view: "windows", name: "windows", icon: "⧉", key: "W" },
  { view: "projects", name: "projects", icon: "◈", key: "P" },
  { view: "rules", name: "rules", icon: "⚑", key: "R" },
  { view: "setup", name: "settings", icon: "⚙", key: "S" },
];

// Every key the panel answers to, in the order they come up: move, decide,
// clear, leave. The global shortcut is listed separately because the one in
// force is not always the one configured.
const KEYS: [string, keyof Dictionary["hints"]][] = [
  ["J / K", "move"],
  ["Y", "allow"],
  ["N", "deny"],
  ["↵", "openEditor"],
  ["D", "dismiss"],
  ["⇧D", "dismissAll"],
  ["W P R S", "views"],
  ["M", "snooze"],
  ["C", "bar"],
  // One key, two meanings depending on where you are, so it says both.
  ["Esc", "escape"],
];

/// How often the hook setup is re-read. Nothing else prompts a re-check when
/// the failure is that no events arrive.
const STATUS_POLL_MS = 15_000;

// Hooks are loaded when a session starts, so a hook arriving *after* the
// config was written is the only proof they are in effect.
function isLive(status: HookStatus): boolean {
  if (!status.installed || status.lastHookAt === null) return false;
  return status.installedAt === null || status.lastHookAt >= status.installedAt;
}

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const { items, selected, selectedId, setSelectedId, move, pulse } = useInbox(settings);
  const [view, setView] = useState<View>("inbox");
  const [installed, setInstalled] = useState(true);
  const [live, setLive] = useState(true);
  const [misrouted, setMisrouted] = useState(0);
  const [autoSetup, setAutoSetup] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [snoozeUntil, setSnoozeUntil] = useState<number | null>(null);
  const [mode, setMode] = useState<"full" | "pill">("full");
  const [port, setPort] = useState(8787);
  const [shortcut, setShortcut] = useState<string | null>(null);
  useTick();

  useEffect(() => {
    void api.getSettings().then(setSettings);
    void api.getSnooze().then(setSnoozeUntil);
    void api.getMode().then(setMode);
    void api.activeShortcut().then(setShortcut);
    const snooze = listen<number | null>("snooze:changed", (e) => setSnoozeUntil(e.payload));
    // The window is resized by Rust, so the layout follows rather than leads.
    const modeEvent = listen<"full" | "pill">("mode:changed", (e) => setMode(e.payload));
    void api.serverPort().then(setPort);
    // Polled, not read once: a hook pointed at another copy of the app is
    // refused at some later moment, and the whole failure is that nothing
    // arrives to prompt a re-check.
    const readStatus = () =>
      void api.hooksStatus().then((status) => {
        setInstalled(status.installed);
        setLive(isLive(status));
        setMisrouted(status.misrouted ?? 0);
        if (!status.installed) {
          setView("setup");
          setAutoSetup(true);
        }
      });
    readStatus();
    const poll = setInterval(readStatus, STATUS_POLL_MS);
    return () => {
      clearInterval(poll);
      void snooze.then((un) => un());
      void modeEvent.then((un) => un());
    };
  }, []);

  // The check reads `~/.claude/settings.json`, which the user may have wired
  // up by hand or from another home. Rows actually arriving is the stronger
  // evidence, and they must never be hidden behind a setup screen.
  useEffect(() => {
    if (autoSetup && items.length > 0) {
      setView("inbox");
      setAutoSetup(false);
    }
  }, [autoSetup, items.length]);

  // Which row, if any, the user ticked "remember this" on. Cleared as soon as
  // the row is answered, so the choice can never leak onto the next call.
  const [remember, setRemember] = useState<{ id: string; scope: Scope } | null>(null);
  const [undo, setUndo] = useState<string | null>(null);
  // The key list, on demand. It used to be a strip along the bottom, which
  // spent height on every session to answer a question asked in the first.
  const [keys, setKeys] = useState(false);
  // Re-read on every snapshot: a call approved by a rule never appears in
  // the queue, so an arrival is the only hint that the tally may have moved.
  const [autoAllowed, setAutoAllowed] = useState(0);
  useEffect(() => {
    void api
      .listRules()
      .then((rules) => setAutoAllowed(rules.reduce((sum, rule) => sum + rule.hits, 0)));
  }, [items]);

  const resolve = useCallback(
    (decision: Decision, scope?: Scope) => {
      if (selected?.kind !== "permission") return;
      const applied =
        scope ?? (remember?.id === selected.id ? remember.scope : undefined);
      void api.resolve(selected.id, decision, applied).then((outcome) => {
        // A standing rule made with no acknowledgement and no way back reads
        // as irreversible, so it is offered back immediately.
        if (outcome.ruleAdded) {
          setUndo(outcome.ruleLabel ?? "");
          window.setTimeout(() => setUndo(null), 10000);
        }
      });
      setRemember(null);
    },
    [selected, remember],
  );

  const dismiss = useCallback(() => {
    if (selected) void api.dismiss(selected.id);
  }, [selected]);

  const openEditor = useCallback(() => {
    if (!selected) return;
    const item = selected;
    setError(null);
    api
      .focusEditor(item.id)
      .then(() => {
        // A blocked call still needs an answer after the detour; an
        // informational row is done the moment the window is in front.
        if (item.kind !== "permission") void api.dismiss(item.id);
      })
      // Swallowing this made a failed jump indistinguishable from a working
      // one that focused nothing.
      .catch((e) => setError(String(e)));
  }, [selected]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      // Renaming a project means typing letters that are otherwise shortcuts.
      if (event.target instanceof HTMLInputElement) return;

      if (event.key === "Escape") {
        // The overlay is the shallowest thing on screen, so it closes first.
        if (keys) setKeys(false);
        else if (view === "inbox") void api.hidePanel();
        else setView("inbox");
        return;
      }
      if (event.key === "?") {
        setKeys((open) => !open);
        return;
      }
      if (event.key === "r") {
        setView((v) => (v === "rules" ? "inbox" : "rules"));
        return;
      }
      if (event.key === "s") {
        setView((v) => (v === "setup" ? "inbox" : "setup"));
        return;
      }
      if (event.key === "p") {
        setView((v) => (v === "projects" ? "inbox" : "projects"));
        return;
      }
      if (event.key === "w") {
        setView((v) => (v === "windows" ? "inbox" : "windows"));
        return;
      }
      if (event.key === "m") {
        void api.toggleSnooze().then(setSnoozeUntil);
        return;
      }
      if (event.key === "c") {
        void api.collapsePanel();
        return;
      }
      if (view !== "inbox" || !selected) return;

      switch (event.key) {
        case "j":
        case "ArrowDown":
          move(1);
          break;
        case "k":
        case "ArrowUp":
          move(-1);
          break;
        case "y":
          resolve("allow");
          break;
        case "n":
          resolve("deny");
          break;
        // No key creates a rule. `A` next to `Y` meant one slip could make a
        // call permanent; the checkbox on the row is the only way now.
        case "d":
          dismiss();
          break;
        case "D":
          void api.dismissAll();
          break;
        case "Enter":
          openEditor();
          break;
        default:
          return;
      }
      event.preventDefault();
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, selected, keys, move, resolve, dismiss, openEditor]);

  const t = useMemo(() => dictionary(settings.lang), [settings.lang]);

  // Pointing at the bar opens it. Closing again is watched natively against
  // the OS cursor: a DOM leave also fires when a native tooltip opens over a
  // button, which was closing the panel on hover.
  const hoverTimer = useRef<number | undefined>(undefined);

  const cancelPeek = useCallback(() => {
    window.clearTimeout(hoverTimer.current);
  }, []);

  const peek = useCallback(() => {
    cancelPeek();
    // Long enough that a cursor merely crossing the bar does not open it.
    hoverTimer.current = window.setTimeout(() => void api.expandPanel(true), 350);
  }, [cancelPeek]);

  // Typing is the one case where it must not close on its own. Clicking
  // anything else changes nothing: the pointer leaving is the whole signal,
  // and pinning turned a window opened by hovering into one that had to be
  // closed by hand.
  useEffect(() => {
    const onFocus = (event: FocusEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches?.("input, select, textarea")) void api.pinPanel();
    };
    document.addEventListener("focusin", onFocus);
    return () => document.removeEventListener("focusin", onFocus);
  }, []);

  // It opens, you deal with it, it gets out of the way.
  //
  // The trigger is the queue *draining*, not the queue *being empty*: opening
  // an already-empty panel on purpose was being collapsed straight back out
  // from under the user. Only clearing the last row earns the collapse.
  const previousCount = useRef(items.length);
  useEffect(() => {
    const drained = previousCount.current > 0 && items.length === 0;
    previousCount.current = items.length;

    if (!drained || mode !== "full" || view !== "inbox") return;
    // Keeping the list up is a choice about how the app sits on the desktop,
    // so nothing the queue does may override it.
    if (settings.autoHide || settings.keepOpen) return;
    // Long enough for the "cleared" flash to register first.
    const timer = setTimeout(() => void api.collapsePanel(), 900);
    return () => clearTimeout(timer);
  }, [items.length, mode, view, settings.autoHide, settings.keepOpen]);

  // Only the background layers take the alpha; text and borders stay opaque
  // so a translucent panel is still readable over busy windows.
  useEffect(() => {
    document.documentElement.style.setProperty("--alpha", String(settings.opacity));
  }, [settings.opacity]);

  // The default outline is nearly the same value as a dark desktop, so once
  // the panel is translucent its edge disappears. A chosen colour is drawn
  // fully opaque and thicker, which is what makes the boundary readable.
  useEffect(() => {
    const root = document.documentElement;
    if (settings.border) {
      root.style.setProperty("--frame", settings.border);
    } else {
      root.style.removeProperty("--frame");
    }
    root.classList.toggle("framed", Boolean(settings.border));
  }, [settings.border]);

  // The tray is outside the web view and cannot read the dictionary itself.
  useEffect(() => {
    void api.setTrayStrings(t.tray);
  }, [t]);

  const update = useCallback((next: Settings) => {
    void api.setSettings(next).then(setSettings);
  }, []);

  const pending = items.filter((i) => i.kind === "permission").length;
  const questions = items.filter((i) => i.kind === "needsInput").length;
  // Only finished turns can be cleared in bulk; a question is a stopped
  // session, not news.
  const clearable = items.filter((i) => i.kind === "completed").length;
  // useTick re-renders every second, so this counts down on its own and
  // disappears the moment the suppression lapses.
  const snoozeRemaining =
    snoozeUntil && snoozeUntil > Date.now()
      ? Math.ceil((snoozeUntil - Date.now()) / 60000)
      : null;

  if (mode === "pill") {
    return (
      <I18nContext.Provider value={t}>
        <main key="compact" className={`app compact ${pulse ? `pulse-${pulse}` : ""}`}>
          <Pill
            items={items}
            onPeek={settings.hoverExpand ? peek : cancelPeek}
            onCancelPeek={cancelPeek}
          />
        </main>
      </I18nContext.Provider>
    );
  }

  return (
    <I18nContext.Provider value={t}>
    <main
      key="full"
      className={`app ${pulse ? `pulse-${pulse}` : ""}`}
    >
      <header className="titlebar" data-tauri-drag-region>
        <span className={`count ${pending > 0 ? "is-hot" : ""}`} data-tauri-drag-region>
          {questions > 0
            ? t.header.withQuestions(pending, questions)
            : pending > 0
              ? t.header.pending(pending)
              : t.header.idle}
        </span>
        {/* What the inbox is not showing you. Rules approve calls with no
            row and no sound, so the header — which otherwise only counts
            what arrived — is the one place that number belongs. */}
        {view === "inbox" && autoAllowed > 0 && (
          <button
            className="auto-count"
            title={t.header.autoAllowedHint}
            onClick={() => setView("rules")}
          >
            ⚑ {autoAllowed}
          </button>
        )}
        {/* Always visible while in force, and it expires on its own, so a
            suppression cannot quietly become permanent. */}
        {snoozeRemaining !== null && (
          <button
            className="snoozed"
            title={t.snooze.hint}
            onClick={() => void api.toggleSnooze().then(setSnoozeUntil)}
          >
            🔕 {t.snooze.active(snoozeRemaining)}
          </button>
        )}
        {/* Every view is always reachable. A screen you can only leave by
            guessing a key is a screen the approvals are stuck behind.
            The titles matter once the panel is narrow enough that the labels
            collapse and only the shortcut letters are left. */}
        <nav>
          {TABS.map((tab) => (
            <button
              key={tab.view}
              className={view === tab.view ? "on" : ""}
              // Narrow, the button is only its icon, so the name has to be
              // here — and the key with it, since the chip is hidden too.
              title={`${t.nav[tab.name]}${tab.key ? ` (${tab.key})` : ""}`}
              onClick={() => setView(tab.view)}
            >
              <span className="nav-icon">{tab.icon}</span>
              <span className="label">{t.nav[tab.name]}</span>
              {tab.key && <kbd>{tab.key}</kbd>}
              {tab.view === "inbox" && pending > 0 && (
                <span className="nav-count">{pending}</span>
              )}
            </button>
          ))}
        </nav>

        {/* The only thing left pointing at the keyboard. Without it the keys
            would be unfindable, which is why the strip along the bottom
            could not simply be deleted. */}
        <button className="collapse" title={t.keys.hint} onClick={() => setKeys((o) => !o)}>
          ?
        </button>
        {/* Not a tab: it changes the panel's shape, not what is shown. Drawn
            as the window control it behaves like, since a bespoke glyph in
            the tab row read as another view. */}
        <button
          className="collapse"
          title={t.pill.collapse}
          onClick={() => void api.collapsePanel()}
        >
          –
        </button>
      </header>

      {view === "setup" && (
        <Setup
          installed={installed}
          live={live}
          misrouted={misrouted}
          port={port}
          settings={settings}
          onSettings={update}
          onChanged={(value) => {
            setInstalled(value);
            // Rewriting the hooks is what clears this in Rust; mirroring it
            // here keeps the warning from lingering until the next poll.
            setMisrouted(0);
          }}
          onDone={() => setView("inbox")}
        />
      )}
      {view === "windows" && <SessionsView />}
      {view === "projects" && <ProjectsView />}
      {view === "rules" && <RulesView />}

      {view === "inbox" &&
        (items.length === 0 ? (
          <div className="empty">
            <p className="empty-mark">✓</p>
            <p>{t.empty.title}</p>
            <p className="note">{t.empty.hint}</p>
            {/* An empty inbox has two causes that look identical: nothing
                asked, or a rule answered it silently. There is room to say
                which in words here, where the header only has room for a
                number. */}
            {autoAllowed > 0 && (
              <button className="empty-rules" onClick={() => setView("rules")}>
                {t.empty.autoAllowed(autoAllowed)}
              </button>
            )}
            {/* The one cause of an empty inbox that is not good news. It has
                to be said here, because a screen nobody opens is where this
                would otherwise be explained. */}
            {misrouted > 0 && (
              <button className="empty-warn" onClick={() => setView("setup")}>
                {t.empty.misrouted(misrouted)}
              </button>
            )}
          </div>
        ) : (
          <ul className="list">
            {items.map((item) => (
              <ItemRow
                key={item.id}
                item={item}
                selected={item.id === selectedId}
                remember={remember?.id === item.id ? remember.scope : null}
                onRemember={(scope) =>
                  setRemember(scope ? { id: item.id, scope } : null)
                }
                onSelect={() => setSelectedId(item.id)}
                onResolve={resolve}
                onDismiss={dismiss}
                onOpenEditor={openEditor}
              />
            ))}
          </ul>
        ))}

      {undo !== null && (
        <p className="undo">
          <span>{t.actions.ruleAdded(undo)}</span>
          <button
            onClick={() => {
              void api.undoLastRule();
              setUndo(null);
            }}
          >
            {t.actions.undo}
          </button>
        </p>
      )}

      {error && (
        <p className="failure" onClick={() => setError(null)}>
          {error}
        </p>
      )}

      {keys && (
        <div className="keys" onClick={() => setKeys(false)}>
          <dl>
            {/* The real key, not the configured one: they differ whenever
                something else already owns it. */}
            <dt>{shortcut ? <kbd>{shortcut}</kbd> : <kbd>{t.hints.trayOnly}</kbd>}</dt>
            <dd>{t.hints.show}</dd>
            {KEYS.map(([key, name]) => (
              <Fragment key={key}>
                <dt><kbd>{key}</kbd></dt>
                <dd>{t.hints[name]}</dd>
              </Fragment>
            ))}
          </dl>
          <p className="note">{t.keys.dismiss}</p>
        </div>
      )}

      {/* Only rendered when it has something to offer. The strip used to be
          permanent, spending height on hints that stop being news after the
          first session. */}
      {view === "inbox" && clearable > 0 && (
        <footer className="hints">
          <button className="clear-all" onClick={() => void api.dismissAll()}>
            <kbd>⇧D</kbd> {t.actions.dismissAll(clearable)}
          </button>
        </footer>
      )}
    </main>
    </I18nContext.Provider>
  );
}
