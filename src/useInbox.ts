import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Item, type Settings } from "./api";
import { play } from "./sound";

/**
 * Mirrors the Rust-side queue.
 *
 * Selection is tracked by id rather than by index, so a row resolving
 * elsewhere in the list cannot move the cursor under the user's fingers and
 * turn the next keystroke into the wrong decision.
 */
export function useInbox(settings: Settings) {
  const [items, setItems] = useState<Item[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pulse, setPulse] = useState<"arrived" | "cleared" | null>(null);
  const itemsRef = useRef<Item[]>([]);
  const selectedIdRef = useRef<string | null>(null);
  const settingsRef = useRef(settings);
  itemsRef.current = items;
  selectedIdRef.current = selectedId;
  settingsRef.current = settings;

  const flash = useCallback((kind: "arrived" | "cleared") => {
    if (!settingsRef.current.flash) return;
    setPulse(kind);
    // Outlasts the bar's two-blink animation; removing the class early would
    // cut the flash short exactly where it is hardest to notice.
    setTimeout(() => setPulse(null), 1200);
  }, []);

  useEffect(() => {
    void api.listItems().then(setItems);
    const changed = listen<Item[]>("inbox:changed", (event) => {
      // The panel no longer disappears when the queue drains, so clearing the
      // last row needs its own acknowledgement.
      if (event.payload.length === 0 && itemsRef.current.length > 0) {
        flash("cleared");
      }
      setItems(event.payload);
    });
    const arrived = listen<Item>("inbox:arrived", (event) => {
      if (settingsRef.current.sound) play(event.payload.kind);
      flash("arrived");
    });
    return () => {
      void changed.then((un) => un());
      void arrived.then((un) => un());
    };
  }, [flash]);

  useEffect(() => {
    if (items.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!items.some((i) => i.id === selectedIdRef.current)) {
      setSelectedId(items[0].id);
    }
  }, [items]);

  const move = useCallback((delta: number) => {
    const current = itemsRef.current;
    const index = current.findIndex((i) => i.id === selectedIdRef.current);
    const next = Math.min(Math.max(index + delta, 0), current.length - 1);
    if (current[next]) setSelectedId(current[next].id);
  }, []);

  const selected = items.find((i) => i.id === selectedId) ?? null;
  return { items, selected, selectedId, setSelectedId, move, pulse };
}

/** Re-renders once a second so elapsed times stay honest. */
export function useTick() {
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, []);
}

/**
 * How long a row has been waiting.
 *
 * The units come from the dictionary; `m:ss` needs no translation and is the
 * form people already read on a stopwatch.
 */
export function elapsed(since: number, units: { second: string; hour: string; minute: string }) {
  const seconds = Math.max(0, Math.floor((Date.now() - since) / 1000));
  if (seconds < 60) return `${seconds}${units.second}`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
  return `${Math.floor(minutes / 60)}${units.hour}${minutes % 60}${units.minute}`;
}
