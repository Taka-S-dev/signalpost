import { render, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS } from "../api";
import { dictionary, I18nContext } from "../i18n";
import { Setup } from "./Setup";

vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: {
    codexInstalled: () => Promise.resolve(false),
    palette: () => Promise.resolve([]),
    activeShortcut: () => Promise.resolve("Alt+Space"),
  },
}));

const t = dictionary("en");

function show(over: { installed?: boolean; live?: boolean; misrouted?: number } = {}) {
  const { container } = render(
    <I18nContext.Provider value={t}>
      <Setup
        installed={over.installed ?? true}
        live={over.live ?? false}
        misrouted={over.misrouted ?? 0}
        port={8787}
        settings={DEFAULT_SETTINGS}
        onSettings={() => {}}
        onChanged={() => {}}
        onDone={() => {}}
      />
    </I18nContext.Provider>,
  );
  return within(container);
}

describe("Setup", () => {
  // Each copy of the app keeps its own token, so hooks written by one and read
  // by another are refused with nothing shown anywhere. This is the screen
  // that has to name it.
  it("names the copy problem rather than blaming a stale session", () => {
    const q = show({ misrouted: 4 });
    expect(q.getByText(t.setup.misrouted(4))).toBeTruthy();
    expect(q.getByText(t.setup.misroutedHint)).toBeTruthy();
    // "No hook has arrived yet" is true but useless here: they are arriving,
    // somewhere else. Showing both leaves the reader to pick.
    expect(q.queryByText(t.setup.needsRestartHint)).toBeNull();
  });

  // Rewriting the hooks is the fix, and it was previously unreachable while
  // installed — the only button on offer was "remove".
  it("offers to repoint the hooks while they are still installed", () => {
    const q = show({ misrouted: 1 });
    expect(q.getByRole("button", { name: t.setup.repoint })).toBeTruthy();
  });

  it("says nothing about copies when nothing has been misaddressed", () => {
    const q = show({ misrouted: 0 });
    expect(q.queryByText(t.setup.misroutedHint)).toBeNull();
    expect(q.queryByRole("button", { name: t.setup.repoint })).toBeNull();
    expect(q.getByText(t.setup.needsRestartHint)).toBeTruthy();
  });
});
