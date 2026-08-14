import { render, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { type Item } from "../api";
import { dictionary, I18nContext } from "../i18n";
import { Pill } from "./Pill";

vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: { expandPanel: () => Promise.resolve() },
}));

const t = dictionary("en");

function row(kind: Item["kind"], project: string): Item {
  return {
    id: `${kind}-${project}`,
    kind,
    agent: "claude",
    sessionId: project,
    cwd: `C:/demo/${project}`,
    project,
    label: project,
    color: "#8ea3c4",
    toolName: kind === "permission" ? "Bash" : "",
    summary: "",
    detail: null,
    detailKind: "text",
    risk: null,
    repeat: 1,
    signature: "",
    createdAt: Date.now(),
  } as unknown as Item;
}

function show(items: Item[]) {
  const { container } = render(
    <I18nContext.Provider value={t}>
      <Pill items={items} onPeek={() => {}} onCancelPeek={() => {}} />
    </I18nContext.Provider>,
  );
  return { container, q: within(container) };
}

describe("the bar", () => {
  // It used to describe the blocked calls and nothing else, so a session that
  // had finished was absent from the bar for as long as anything was waiting
  // — which, with several sessions running, is most of the time.
  it("counts the finished sessions even while something is waiting", () => {
    const { q } = show([
      row("permission", "web-app"),
      row("completed", "api"),
      row("completed", "docs"),
    ]);
    expect(q.getByText("✓2")).toBeTruthy();
  });

  it("says nothing about finished sessions when there are none", () => {
    const { q } = show([row("permission", "web-app")]);
    expect(q.queryByText(/✓/)).toBeNull();
  });

  // With nothing blocked the bar is already describing the finished rows
  // themselves, so a second count of the same thing would be noise.
  it("does not repeat the count when the finished rows are the subject", () => {
    const { q } = show([row("completed", "api"), row("completed", "docs")]);
    expect(q.queryByText(/✓/)).toBeNull();
    expect(q.getByText("+1")).toBeTruthy();
  });
});
