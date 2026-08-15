import { act, render, within } from "@testing-library/react";
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

// The screen asks Rust three questions as it mounts. Awaiting them keeps the
// answers from landing after the assertions, outside React's knowledge.
async function show(
  over: {
    installed?: boolean;
    live?: boolean;
    misrouted?: number;
    elsewhere?: boolean;
  } = {},
) {
  const { container } = render(
    <I18nContext.Provider value={t}>
      <Setup
        installed={over.installed ?? true}
        live={over.live ?? false}
        misrouted={over.misrouted ?? 0}
        elsewhere={over.elsewhere ?? false}
        port={8787}
        settings={DEFAULT_SETTINGS}
        onSettings={() => {}}
        onChanged={() => {}}
        onDone={() => {}}
      />
    </I18nContext.Provider>,
  );
  await act(async () => {});
  return within(container);
}

describe("Setup", () => {
  // Each copy of the app keeps its own token, so hooks written by one and read
  // by another are refused with nothing shown anywhere. This is the screen
  // that has to name it.
  it("names the copy problem rather than blaming a stale session", async () => {
    const q = await show({ misrouted: 4 });
    expect(q.getByText(t.setup.misrouted(4))).toBeTruthy();
    expect(q.getByText(t.setup.misroutedHint)).toBeTruthy();
    // "No hook has arrived yet" is true but useless here: they are arriving,
    // somewhere else. Showing both leaves the reader to pick.
    expect(q.queryByText(t.setup.needsRestartHint)).toBeNull();
  });

  // Rewriting the hooks is the fix, and it was previously unreachable while
  // installed — the only button on offer was "remove".
  it("offers to repoint the hooks while they are still installed", async () => {
    const q = await show({ misrouted: 1 });
    expect(q.getByRole("button", { name: t.setup.repoint })).toBeTruthy();
  });

  // Known from the settings file alone. The refusal counter needs the other
  // copy to be running and posting; the file says so at startup, which is
  // when this screen is looked at.
  it("names the copy problem before a single request has been refused", async () => {
    const q = await show({ installed: false, misrouted: 0, elsewhere: true });
    expect(q.getByText(t.setup.elsewhere)).toBeTruthy();
    // "Not installed" points at the button that overwrites the other copy.
    expect(q.queryByText(t.setup.notInstalled)).toBeNull();
    expect(q.getByRole("button", { name: t.setup.repoint })).toBeTruthy();
  });

  it("says nothing about copies when nothing has been misaddressed", async () => {
    const q = await show({ misrouted: 0 });
    expect(q.queryByText(t.setup.misroutedHint)).toBeNull();
    expect(q.queryByRole("button", { name: t.setup.repoint })).toBeNull();
    expect(q.getByText(t.setup.needsRestartHint)).toBeTruthy();
  });
});
