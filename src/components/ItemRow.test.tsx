import { render, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Item } from "../api";
import { dictionary, I18nContext } from "../i18n";
import { ItemRow } from "./ItemRow";

vi.mock("../api", () => ({ api: { suggestPrefix: () => Promise.resolve("") } }));

const t = dictionary("en");

function row(over: Partial<Item> = {}): Item {
  return {
    id: "1",
    kind: "completed",
    agent: "claude",
    sessionId: "s",
    cwd: "C:/demo/web-app",
    project: "web-app",
    label: "web-app",
    color: "#8ea3c4",
    toolName: "",
    summary: "",
    detail: null,
    detailKind: "text",
    risk: null,
    repeat: 1,
    signature: "",
    createdAt: Date.now(),
    ...over,
  } as unknown as Item;
}

// Queried through the rendered container rather than the global screen:
// each case renders its own row, and a document-wide query would find
// the previous case's row still attached.
function show(item: Item) {
  const { container } = render(
    <I18nContext.Provider value={t}>
      <ItemRow
        item={item}
        selected={false}
        remember={null}
        onRemember={() => {}}
        onSelect={() => {}}
        onResolve={() => {}}
        onDismiss={() => {}}
        onOpenEditor={() => {}}
      />
    </I18nContext.Provider>,
  );
  return { container, q: within(container) };
}

describe("ItemRow", () => {
  // A turn reported by the Stop hook carries no message — that payload has
  // no field for one — so the row used to be a project name and two buttons
  // with nothing said between them.
  it("says something when the event carried no text of its own", () => {
    const { q } = show(row());
    expect(q.getByText(t.summaryFor.completed)).toBeTruthy();
  });

  it("prefers what the session actually said", () => {
    const { q } = show(row({ summary: "Ran the tests" }));
    expect(q.getByText("Ran the tests")).toBeTruthy();
    expect(q.queryByText(t.summaryFor.completed)).toBeNull();
  });

  // The stripe down the side of a finished row is a divider colour, which is
  // legible as a 3px bar and not as text. Painting the name with it left the
  // project unreadable at 1.3:1.
  it("names the project in the project's own colour, not the stripe", () => {
    const { container } = show(row({ color: "#8ea3c4" }));
    const name = container.querySelector(".project") as HTMLElement;
    expect(name.textContent).toBe("web-app");
    expect(name.style.color).toBe("rgb(142, 163, 196)");
  });
});
