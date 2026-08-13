import { afterEach, describe, expect, it, vi } from "vitest";
import { elapsed } from "./useInbox";

const units = { second: "s", hour: "h", minute: "m" };

/** `now` is fixed so the boundaries are exact rather than nearly right. */
function at(now: number, since: number) {
  vi.useFakeTimers();
  vi.setSystemTime(now);
  return elapsed(since, units);
}

afterEach(() => vi.useRealTimers());

describe("elapsed", () => {
  const start = 1_700_000_000_000;

  it("counts seconds on their own for the first minute", () => {
    expect(at(start, start)).toBe("0s");
    expect(at(start + 59_000, start)).toBe("59s");
  });

  it("switches to m:ss on the minute and pads the seconds", () => {
    expect(at(start + 60_000, start)).toBe("1:00");
    expect(at(start + 65_000, start)).toBe("1:05");
    expect(at(start + 59 * 60_000 + 59_000, start)).toBe("59:59");
  });

  it("switches to hours and minutes on the hour", () => {
    expect(at(start + 60 * 60_000, start)).toBe("1h0m");
    expect(at(start + 90 * 60_000, start)).toBe("1h30m");
  });

  /// A row created a moment in the future — the clock stepping back, or two
  /// machines disagreeing — must not read as a negative age.
  it("never goes below zero", () => {
    expect(at(start, start + 5_000)).toBe("0s");
  });
});
