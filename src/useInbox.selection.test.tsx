import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Item, Settings } from "./api";

/**
 * The queue lives in Rust and arrives over Tauri events, so both are replaced
 * here. What is under test is the part that is genuinely the frontend's: which
 * row the keys will act on.
 */
const listeners = new Map<string, (event: { payload: unknown }) => void>();
let initial: Item[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler);
    return Promise.resolve(() => listeners.delete(name));
  },
}));
vi.mock("./api", () => ({ api: { listItems: () => Promise.resolve(initial) } }));
vi.mock("./sound", () => ({ play: () => {} }));

const { useInbox } = await import("./useInbox");

const settings = { sound: false, flash: false } as Settings;

function row(id: string): Item {
  return {
    id,
    kind: "permission",
    agent: "claude",
    sessionId: id,
    cwd: "C:/demo/app",
    project: "app",
    label: "app",
    color: "#8ea3c4",
    toolName: "Bash",
    summary: "",
    detail: null,
    detailKind: "text",
    risk: null,
    repeat: 1,
    signature: `Bash:${id}`,
    createdAt: 0,
  } as unknown as Item;
}

/** Pushes a new queue the way the Rust side does. */
function emit(items: Item[]) {
  act(() => listeners.get("inbox:changed")?.({ payload: items }));
}

beforeEach(() => {
  listeners.clear();
  initial = [row("a"), row("b"), row("c")];
});

describe("useInbox selection", () => {
  it("starts on the first row", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));
  });

  /// The reason selection is held by id and not by index. A row resolving
  /// above the cursor would otherwise slide a different row under it, and the
  /// next `Y` would answer something the user never looked at.
  it("stays on the same row when an earlier one is resolved", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));

    act(() => result.current.setSelectedId("c"));
    emit([row("b"), row("c")]);

    expect(result.current.selectedId).toBe("c");
  });

  it("falls back to the first row only when the selected one is gone", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));

    act(() => result.current.setSelectedId("c"));
    emit([row("a"), row("b")]);

    await waitFor(() => expect(result.current.selectedId).toBe("a"));
  });

  it("clears the selection when the queue empties", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));

    emit([]);

    await waitFor(() => expect(result.current.selectedId).toBeNull());
  });

  it("stops at the ends rather than wrapping", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));

    act(() => result.current.move(-1));
    expect(result.current.selectedId).toBe("a");

    act(() => result.current.move(1));
    act(() => result.current.move(1));
    act(() => result.current.move(1));
    expect(result.current.selectedId).toBe("c");
  });

  /// A jump past the end lands on the end, rather than doing nothing. Only
  /// this distinguishes clamping from the guard that skips a missing row: for
  /// a step of one the two are identical, which is how the clamp survived
  /// being deleted while every other test still passed.
  it("lands on the last row when asked to move past it", async () => {
    const { result } = renderHook(() => useInbox(settings));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));

    act(() => result.current.move(10));
    expect(result.current.selectedId).toBe("c");

    act(() => result.current.move(-10));
    expect(result.current.selectedId).toBe("a");
  });
});
