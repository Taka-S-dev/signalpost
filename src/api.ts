import { invoke } from "@tauri-apps/api/core";

export type ItemKind = "permission" | "needsInput" | "completed";
export type Decision = "allow" | "deny";
export type Scope = "exactCall" | "toolInProject" | "toolEverywhere";

export type Agent = "claude" | "codex";

export interface Item {
  id: string;
  kind: ItemKind;
  agent: Agent;
  sessionId: string;
  cwd: string;
  project: string;
  label: string;
  color: string;
  toolName: string;
  summary: string;
  detail: string | null;
  detailKind: "text" | "diff";
  risk: RiskMark | null;
  /** How many times this happened for the session; repeats replace the row. */
  repeat: number;
  signature: string;
  createdAt: number;
}

export type RiskLevel = "danger" | "caution";

export interface RiskMark {
  level: RiskLevel;
  icon: string;
  /** Free text the user typed; empty on seeded rules, which carry a key. */
  label: string;
  key: string | null;
}

export interface RiskRule {
  pattern: string;
  level: RiskLevel;
  icon: string;
  label: string;
  key: string | null;
  enabled: boolean;
}

export type Lang = "auto" | "ja" | "en";

export interface Settings {
  sound: boolean;
  flash: boolean;
  autoHide: boolean;
  opacity: number;
  lang: Lang;
  /** Empty means the subtle default outline. */
  border: string;
  popup: PopupWhen;
  toast: boolean;
  hoverExpand: boolean;
  shortcut: string;
}

export type PopupWhen = "permission" | "all" | "never";

export interface ResolveOutcome {
  resolved: boolean;
  ruleAdded: boolean;
  ruleLabel: string | null;
}

export interface HookStatus {
  installed: boolean;
  installedAt: number | null;
  lastHookAt: number | null;
}

export const DEFAULT_SETTINGS: Settings = {
  sound: true,
  flash: true,
  autoHide: false,
  opacity: 1,
  lang: "auto",
  border: "",
  popup: "permission",
  toast: true,
  hoverExpand: true,
  shortcut: "Alt+Space",
};

export interface TrayStrings {
  show: string;
  snooze: string;
  unsnooze: string;
  reset: string;
  quit: string;
  idle: string;
  pending: string;
}

/** A rule's parts; the sentence is assembled per language in the UI. */
export interface RuleView {
  toolName: string;
  signature: string | null;
  project: string | null;
}

export interface Project {
  cwd: string;
  label: string;
  color: string;
  openCommand: string;
  customized: boolean;
  lastSeen: number;
}

export type SessionState = "running" | "waiting" | "idle";

export interface Session {
  sessionId: string;
  cwd: string;
  label: string;
  color: string;
  state: SessionState;
  since: number;
}

export interface WindowEntry {
  handle: number;
  title: string;
  app: string;
  kind: "editor" | "terminal";
  minimized: boolean;
}

export const api = {
  listProjects: () => invoke<Project[]>("list_projects"),
  setProject: (
    cwd: string,
    name: string | null,
    color: string | null,
    openCommand: string | null,
  ) => invoke<Project[]>("set_project", { cwd, name, color, openCommand }),
  defaultOpenCommand: () => invoke<string>("default_open_command"),
  palette: () => invoke<string[]>("palette"),
  listRiskRules: () => invoke<RiskRule[]>("list_risk_rules"),
  setRiskRules: (rules: RiskRule[]) => invoke<RiskRule[]>("set_risk_rules", { rules }),
  restoreRiskDefaults: () => invoke<RiskRule[]>("restore_risk_defaults"),
  listSessions: () => invoke<Session[]>("list_sessions"),
  focusSession: (cwd: string) => invoke<void>("focus_session", { cwd }),
  listWindows: () => invoke<WindowEntry[]>("list_windows"),
  focusWindow: (handle: number) => invoke<void>("focus_window", { handle }),
  resetPanelPosition: () => invoke<void>("reset_panel_position"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  listItems: () => invoke<Item[]>("list_items"),
  resolve: (id: string, decision: Decision, remember?: Scope) =>
    invoke<ResolveOutcome>("resolve", { id, decision, remember: remember ?? null }),
  undoLastRule: () => invoke<RuleView[]>("undo_last_rule"),
  dismiss: (id: string) => invoke<void>("dismiss", { id }),
  dismissAll: () => invoke<number>("dismiss_all"),
  focusEditor: (id: string) => invoke<void>("focus_editor", { id }),
  hidePanel: () => invoke<void>("hide_panel"),
  expandPanel: (peek = false) => invoke<void>("expand_panel", { peek }),
  pinPanel: () => invoke<void>("pin_panel"),
  collapsePanel: () => invoke<void>("collapse_panel"),
  getMode: () => invoke<"full" | "pill">("get_mode"),
  listRules: () => invoke<RuleView[]>("list_rules"),
  removeRule: (index: number) => invoke<RuleView[]>("remove_rule", { index }),
  setTrayStrings: (strings: TrayStrings) => invoke<void>("set_tray_strings", { strings }),
  activeShortcut: () => invoke<string | null>("active_shortcut"),
  setShortcut: (shortcut: string) => invoke<string | null>("set_shortcut", { shortcut }),
  getSnooze: () => invoke<number | null>("get_snooze"),
  toggleSnooze: () => invoke<number | null>("toggle_snooze_command"),
  hooksStatus: () => invoke<HookStatus>("hooks_status"),
  installHooks: () => invoke<string>("install_hooks"),
  uninstallHooks: () => invoke<void>("uninstall_hooks"),
  codexInstalled: () => invoke<boolean>("codex_installed"),
  installCodex: (keepExisting: boolean) =>
    invoke<string>("install_codex", { keepExisting }),
  uninstallCodex: () => invoke<void>("uninstall_codex"),
  serverPort: () => invoke<number>("server_port"),
};
