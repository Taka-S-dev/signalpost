/**
 * The source of truth for both the English copy and the shape every other
 * locale must satisfy — `ja.ts` is typed as `typeof en`, so a missing or
 * misspelled key is a compile error rather than a blank label at runtime.
 */
export const en = {
  nav: {
    inbox: "Inbox",
    windows: "Sessions",
    projects: "Colors",
    rules: "Rules",
    settings: "Settings",
  },
  header: {
    idle: "Nothing waiting",
    pending: (n: number) => `${n} waiting`,
    withQuestions: (n: number, q: number) => `${n} waiting, ${q} asking`,
    autoAllowedHint: "Approved by rules without asking — click for the rule list",
  },
  time: { second: "s", hour: "h", minute: "m" },
  kind: {
    permission: "waiting",
    needsInput: "question",
    completed: "done",
  },
  /** Stands in when an event arrives with no text of its own. */
  summaryFor: {
    permission: "waiting for an answer",
    needsInput: "the session is asking something",
    completed: "the session finished its turn",
  },
  actions: {
    allow: "Allow",
    deny: "Deny",
    rememberLabel: "Allow this without asking from now on",
    rememberBlocked: " — not offered for calls marked dangerous",
    warnSilent:
      "From now on, matching calls run without asking. They will not appear here and make no sound.",
    warnBroad:
      "From now on, every call of this tool runs without asking, whatever it contains. None will appear here or make a sound.",
    coversExact: (project: string) =>
      `Covers only a byte-identical call, in ${project} and below.`,
    coversTool: (tool: string, project: string) =>
      `Covers every ${tool} call in ${project} and below.`,
    rememberWhere: "Rules you create can be deleted any time under ⚑ Rules (R)",
    scopePrefix: "commands starting with… (recommended)",
    scopeCall: (project: string) => `only a byte-identical call, in ${project}`,
    scopeTool: (tool: string, project: string) =>
      `every ${tool} call in ${project} and below`,
    prefixPlaceholder: "e.g. npm run build",
    prefixMatches: (prefix: string, project: string) =>
      `Covers commands in ${project} and below that start with "${prefix}".`,
    prefixEmpty: "Type the opening of the command to cover. Empty matches nothing.",
    ruleAdded: (tool: string) => `Standing rule created for ${tool}.`,
    undo: "Undo",
    answerThere: "Answer in the session",
    dismiss: "Dismiss",
    dismissAll: (n: number) => `clear ${n}`,
    openEditor: "Go to window",
  },
  errors: {
    noWindow:
      "No window for that session was found. A terminal titles its window after the shell, not the folder, so it cannot be matched — give the project a command under P to open something instead.",
  },
  empty: {
    title: "All clear.",
    hint: "Approvals will queue up here.",
    autoAllowed: (n: number) => `${n} approved by rules, without asking →`,
    misrouted: (n: number) =>
      n === 1
        ? "1 event went to a different copy of Signalpost →"
        : `${n} events went to a different copy of Signalpost →`,
  },
  hints: {
    show: "show the panel",
    move: "move between rows",
    bar: "collapse to the bar, and open it again",
    trayOnly: "tray",
    allow: "allow",
    deny: "deny",
    openEditor: "go to the window",
    dismiss: "dismiss this row",
    dismissAll: "clear finished and questions",
    views: "switch views",
    snooze: "stop popping up (30 min)",
    escape: "back to the inbox, or hide it when already there",
  },
  keys: {
    hint: "Keyboard shortcuts (?)",
    dismiss: "Click or press Esc to close",
  },
  setup: {
    title: "Setup",
    installed: "Hooks are installed",
    live: "Hooks are installed and arriving",
    needsRestart: "Installed, but nothing has arrived yet",
    needsRestartHint: "No hook has arrived since this app started; the first event turns it green. If you just installed them, sessions already running were started without them and need a restart.",
    misrouted: (n: number) =>
      n === 1
        ? "1 event went to a different copy of Signalpost"
        : `${n} events went to a different copy of Signalpost`,
    misroutedHint:
      "The hooks name a copy of this app that is not the one running. Each copy — installed, portable, development — keeps its own key, so events addressed to another one are turned away and nothing reaches the inbox. Repointing rewrites the hooks to this copy. Sessions already running keep the old address until they restart.",
    repoint: "Point the hooks at this copy",
    notInstalled: "Hooks are not installed yet",
    explain: (port: number) =>
      `Adds PermissionRequest / Notification / SessionEnd HTTP hooks to ~/.claude/settings.json. They post to 127.0.0.1:${port} only, existing settings are kept, and a .bak is written first.`,
    install: "Install hooks",
    uninstall: "Remove hooks",
    done: "Back to inbox",
    wrote: (path: string) => `Wrote ${path}. Restart any running Claude Code session.`,
    removed: "Hooks removed.",
    display: "Display",
    shortcut: "Global shortcut",
    shortcutHint: "Shows the panel from anywhere. Restart not needed.",
    shortcutTaken: (wanted: string, used: string) => `${wanted} is taken by another app; using ${used}.`,
    language: "Language",
    languageAuto: "Auto",
    sound: "Sound",
    soundHint: "A different cue per row type",
    flash: "Flash on arrival and on clearing",
    flashHint: "The border pulses, so a finished queue is still acknowledged",
    autoHide: "Hide when empty",
    autoHideHint: "Off keeps the panel put, which does not flicker",
    resetPosition: "Reset position and size",
    resetPositionHint: "Use this if a monitor change leaves the panel wrong",
    reset: "Reset",
    popup: "Bring the panel forward",
    popupHint: "A finished turn is news; a blocked call is something waiting on you",
    popupPermission: "Approvals only",
    popupAll: "Anything",
    popupNever: "Never",
    keepOpen: "Keep the list open",
    keepOpenHint: "The panel stays as a list instead of collapsing to the bar when the queue empties",
    emphasize: "Stand out while something waits",
    emphasizeHint: "Starts at once instead of after three minutes of being ignored",
    hoverExpand: "Open the bar on hover",
    hoverExpandHint: "Point at it to peek, move away to collapse. Clicking keeps it open.",
    toast: "System notification for the rest",
    toastHint: "For arrivals that do not raise the panel. Click it to go to that session.",
    border: "Frame color",
    borderHint: "Keeps the edge visible when translucent or on a dark desktop",
    borderNone: "Default",
    opacity: "Background opacity",
    opacityHint: "Text stays opaque, so it is still readable",
  },
  projects: {
    title: "Projects",
    emptyHint: "Nothing has arrived yet. Projects show up here once they ask for something.",
    resetToDefault: "Reset to default",
    command: "Command run by Enter",
    commandHint: "{cwd} is replaced with the path. Empty means the default.",
  },
  rules: {
    riskTitle: "Highlighting rules",
    riskHint:
      "Matched as a case-insensitive substring of the command. Danger beats caution, and the longer match wins within a level.",
    add: "Add",
    restoreDefaults: "Restore defaults",
    newPattern: "Text to match (e.g. git push)",
    danger: "Danger",
    caution: "Caution",
    allowTitle: "Auto-allow rules",
    allowEmpty:
      "None yet. Ticking \"allow without asking\" while approving adds one, and matching calls stop reaching the inbox.",
    remove: "Remove",
    silentTotal: (n: number) =>
      n === 0
        ? "Nothing approved without asking yet."
        : `${n} calls approved without asking so far.`,
    usedTimes: (n: number, when: string) => `${n}× · last ${when}`,
    neverUsed: "never used yet",
    startingWith: (prefix: string) => `commands starting with ${prefix} …`,
    everyCall: (tool: string) => `every ${tool} call`,
    everywhere: "all projects",
  },
  risk: {
    forcePush: "force push",
    push: "push",
    historyLoss: "history rewrite",
    recursiveDelete: "recursive delete",
    dropTable: "drop table",
    publish: "publish",
    release: "release",
    deploy: "deploy",
    infraChange: "infra change",
    commit: "commit",
    network: "network call",
  } as Record<string, string>,
  codex: {
    title: "Codex CLI",
    installed: "Codex notifications are wired up",
    notInstalled: "Codex notifications are not wired up",
    explain:
      "Codex only fires notify on turn completion and cannot return an approval, so its rows are informational. It allows one notify program, so any existing one is chained behind ours and restored if you remove this.",
    keepExisting: "Keep the existing notify program",
    keepExistingHint: "Chains it behind ours. Turn off to take the slot outright.",
    install: "Wire up Codex",
    uninstall: "Remove Codex wiring",
    wrote: (path: string) => `Wrote ${path}. Restart any running Codex session.`,
    removed: "Codex wiring removed.",
  },
  pill: {
    hint: "Click to open",
    drag: "Drag to move (does not open)",
    collapse: "Collapse to the bar (C) — click the bar or press Alt+Space to open it again",
    info: (n: number) => `${n} new`,
    done: (n: number) => (n === 1 ? "1 session finished" : `${n} sessions finished`),
  },
  snooze: {
    active: (min: number) => `${min}m`,
    hint: "Not popping up. Click to resume.",
  },
  sessions: {
    title: "Sessions",
    empty: "No sessions seen yet. They appear here once one submits a prompt.",
    running: "working",
    waiting: "waiting on you",
    idle: "done",
  } as Record<string, string>,
  windows: {
    title: "Other windows",
    refresh: "Refresh",
    empty: "No editor or terminal windows found.",
    editor: "editor",
    terminal: "terminal",
    minimized: "minimized",
  },
  tray: {
    show: "Open panel",
    bar: "Collapse to bar",
    keepOpen: "Keep the list open",
    snooze: "Stop popping up (30 min)",
    unsnooze: "Pop up again",
    reset: "Reset position",
    quit: "Quit",
    idle: "Signalpost — nothing waiting",
    pending: "Signalpost — {n} waiting",
  },
};

export type Dictionary = typeof en;
