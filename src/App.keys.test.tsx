import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS } from "./api";

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
const { collapsePanel, expandPanel, showContextMenu } = vi.hoisted(() => ({
  collapsePanel: vi.fn(() => Promise.resolve()),
  expandPanel: vi.fn(() => Promise.resolve()),
  showContextMenu: vi.fn(() => Promise.resolve()),
}));

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  api: {
    listItems: () => Promise.resolve([]),
    getSettings: () => Promise.resolve({ ...DEFAULT_SETTINGS, sound: false, flash: false }),
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
  },
}));

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
