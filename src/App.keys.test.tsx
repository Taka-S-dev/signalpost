import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS } from "./api";
import { dictionary } from "./i18n";

/**
 * The window's shape is decided in Rust, so what is under test here is only
 * which way the key asks it to go.
 */
const listeners = new Map<string, (event: { payload: unknown }) => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler);
    return Promise.resolve(() => listeners.delete(name));
  },
}));
vi.mock("./sound", () => ({ play: () => {} }));

// Hoisted alongside the vi.mock call below, which runs before the imports.
const { collapsePanel, expandPanel, showContextMenu, focusEditor } = vi.hoisted(() => ({
  collapsePanel: vi.fn(() => Promise.resolve()),
  expandPanel: vi.fn(() => Promise.resolve()),
  showContextMenu: vi.fn(() => Promise.resolve()),
  focusEditor: vi.fn(() => Promise.resolve()),
}));

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  api: {
    listItems: () => Promise.resolve([]),
    suggestPrefix: () => Promise.resolve(""),
    getSettings: () =>
      Promise.resolve({ ...DEFAULT_SETTINGS, sound: false, flash: false, lang: "ja" }),
    getSnooze: () => Promise.resolve(null),
    getMode: () => Promise.resolve("full"),
    activeShortcut: () => Promise.resolve("Alt+Space"),
    serverPort: () => Promise.resolve(8787),
    hooksStatus: () =>
      Promise.resolve({
        installed: true,
        installedAt: 1,
        lastHookAt: 2,
        misrouted: null,
        misroutedAt: null,
      }),
    listRules: () => Promise.resolve([]),
    setTrayStrings: () => Promise.resolve(),
    collapsePanel,
    expandPanel,
    showContextMenu,
    focusEditor,
    dismiss: () => Promise.resolve(),
  },
}));

const settings = { ...DEFAULT_SETTINGS, sound: false, flash: false, lang: "ja" as const };
const t = dictionary("ja");

const { default: App } = await import("./App");

/**
 * Mounts the app and lets everything it asks Rust on startup come back, so no
 * answer lands mid-assertion.
 */
async function mount() {
  render(<App />);
  await waitFor(() => expect(listeners.has("mode:changed")).toBe(true));
  await act(async () => {});
}

/** Puts the window into the bar, the way Rust announces it. */
function enterBarMode() {
  act(() => listeners.get("mode:changed")?.({ payload: "pill" }));
}

function pressC() {
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true }));
  });
}

describe("the collapse key", () => {
  beforeEach(() => {
    listeners.clear();
    collapsePanel.mockClear();
    expandPanel.mockClear();
    showContextMenu.mockClear();
  });

  it("collapses the panel", async () => {
    await mount();

    pressC();
    expect(collapsePanel).toHaveBeenCalledTimes(1);
    expect(expandPanel).not.toHaveBeenCalled();
  });

  // It used to call "collapse" here too, which is already the state: the key
  // did nothing at all in the one place its other half is wanted.
  it("opens the bar again rather than doing nothing", async () => {
    await mount();

    enterBarMode();
    pressC();
    expect(expandPanel).toHaveBeenCalledTimes(1);
    expect(collapsePanel).not.toHaveBeenCalled();
  });
});

describe("right-clicking", () => {
  beforeEach(() => {
    listeners.clear();
    showContextMenu.mockClear();
  });

  // Without this the web view raises Edge's own menu — reload, save as,
  // print — over what is meant to look like a native panel.
  it("raises the app's menu instead of the web view's", async () => {
    await mount();

    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 12,
      clientY: 34,
    });
    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(event.defaultPrevented).toBe(true);
    expect(showContextMenu).toHaveBeenCalledWith(12, 34);
  });

  // The bar is the surface people actually right-click: the tray icon sits in
  // the Windows 11 overflow, hidden, which is why the bar exists at all.
  it("works in the bar as well as the panel", async () => {
    await mount();
    enterBarMode();

    act(() => {
      document.body.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 6 }),
      );
    });

    expect(showContextMenu).toHaveBeenCalledWith(5, 6);
  });
});

/** A row, as Rust would push it. */
function queued(kind: "permission" | "completed", ageMs = 0) {
  return [
    {
      id: "1",
      kind,
      agent: "claude",
      sessionId: "web-app",
      cwd: "C:/demo/web-app",
      project: "web-app",
      label: "web-app",
      color: "#8ea3c4",
      toolName: kind === "permission" ? "Bash" : "",
      summary: "",
      detail: null,
      detailKind: "text",
      risk: null,
      repeat: 1,
      signature: "",
      createdAt: Date.now() - ageMs,
    },
  ];
}

describe("the bar standing out", () => {
  beforeEach(() => {
    listeners.clear();
  });

  function barClass() {
    return document.querySelector("main.compact")?.className ?? "";
  }

  // The pulse is set on the window, not on the button inside it: the button
  // starts after the drag grip, so setting it there left the left end of the
  // bar dark and began the wash partway across.
  it("stays calm while a row is still fresh", async () => {
    await mount();
    act(() => listeners.get("inbox:changed")?.({ payload: queued("permission") }));
    await act(async () => {});
    enterBarMode();
    expect(barClass()).not.toContain("is-waiting");
    expect(barClass()).not.toContain("is-insistent");
  });

  it("breathes once a row has been ignored for minutes", async () => {
    await mount();
    act(() => listeners.get("inbox:changed")?.({ payload: queued("permission", 4 * 60_000) }));
    await act(async () => {});
    enterBarMode();
    expect(barClass()).toContain("is-waiting");
  });

  it("insists from the first second when asked to", async () => {
    await mount();
    // Rust announces the change; the screen follows rather than keeping what
    // it read at startup.
    act(() => listeners.get("settings:changed")?.({ payload: { ...settings, emphasize: true } }));
    act(() => listeners.get("inbox:changed")?.({ payload: queued("permission") }));
    await act(async () => {});
    enterBarMode();
    expect(barClass()).toContain("is-insistent");
  });

  // Nothing is blocked, so nothing is being waited on.
  it("says nothing when the queue is empty", async () => {
    await mount();
    enterBarMode();
    expect(barClass()).not.toContain("is-waiting");
    expect(barClass()).not.toContain("is-insistent");
  });

  // A finished turn is news, not a session held up. Insisting about it would
  // make the pulse mean two different things at once.
  it("stays calm for finished rows even with the setting on", async () => {
    await mount();
    act(() => listeners.get("settings:changed")?.({ payload: { ...settings, emphasize: true } }));
    act(() => listeners.get("inbox:changed")?.({ payload: queued("completed") }));
    await act(async () => {});
    enterBarMode();
    expect(barClass()).not.toContain("is-insistent");
    expect(barClass()).not.toContain("is-waiting");
  });
});

describe("jumping to a session's window", () => {
  beforeEach(() => {
    listeners.clear();
    focusEditor.mockReset();
  });

  async function pressEnterOnARow() {
    await mount();
    act(() => listeners.get("inbox:changed")?.({ payload: queued("permission") }));
    await act(async () => {});
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
  }

  // Rust answers with a code when it cannot find the window; the sentence is
  // written here so it comes out in the reader's language.
  it("explains a jump that found nothing, in the reader's language", async () => {
    focusEditor.mockImplementation(() => Promise.reject(new Error("no-window")));
    await pressEnterOnARow();
    expect(document.querySelector(".failure")?.textContent).toBe(t.errors.noWindow);
  });

  it("says nothing when the jump worked", async () => {
    focusEditor.mockImplementation(() => Promise.resolve());
    await pressEnterOnARow();
    expect(document.querySelector(".failure")).toBeNull();
  });

  // Anything else is passed through rather than dressed up as this one.
  it("passes an unrelated failure through unchanged", async () => {
    focusEditor.mockImplementation(() => Promise.reject(new Error("that item is gone")));
    await pressEnterOnARow();
    expect(document.querySelector(".failure")?.textContent).toContain("that item is gone");
  });
});
