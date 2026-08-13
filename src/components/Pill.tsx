import { api, type Item } from "../api";
import { useT } from "../i18n";
import { elapsed } from "../useInbox";

/**
 * The collapsed panel: a bar small enough to leave on screen permanently.
 *
 * This is what makes a queued row noticeable without interrupting. A toast
 * disappears and the Windows 11 tray is collapsed by default, so neither can
 * be the thing that waits for you to look up.
 *
 * It shows the row that has been waiting longest rather than a bare count,
 * because "which one, and for how long" is the question a glance is asking.
 */
interface Props {
  items: Item[];
  // Hovering opens the bar — except over the grip, which exists to be
  // grabbed. Expanding out from under the pointer as it arrives makes the
  // bar impossible to drag.
  onPeek: () => void;
  onCancelPeek: () => void;
}

export function Pill({ items, onPeek, onCancelPeek }: Props) {
  const t = useT();
  const pending = items.filter((i) => i.kind === "permission");
  const hot = pending.length > 0;
  // `items` is already ordered blocked-first, oldest-first.
  const lead = (hot ? pending : items)[0];
  const rest = (hot ? pending.length : items.length) - 1;
  // The bar is what is on screen while nobody is looking at the panel, so it
  // is where being ignored has to become visible.
  const stale = hot && Date.now() - lead.createdAt > 3 * 60_000;

  return (
    <>
      {/* A grip that is not the button, so the bar can be moved as well as
          opened. */}
      <span
        className="pill-grip"
        data-tauri-drag-region
        onMouseEnter={onCancelPeek}
        title={t.pill.drag}
      >
        ⠿
      </span>
      <button
        className={`pill ${hot ? "is-hot" : ""} ${stale ? "is-stale" : ""}`}
        onClick={() => void api.expandPanel()}
        onMouseEnter={onPeek}
        title={t.pill.hint}
      >
        <span
          className={`pill-mark ${lead ? "" : "idle"}`}
          style={lead ? { background: lead.color } : undefined}
        />
        <span className="pill-label">
          {lead ? lead.label || lead.project : t.header.idle}
        </span>
        {rest > 0 && <span className="pill-more">+{rest}</span>}
        <span className="pill-age">{lead ? elapsed(lead.createdAt, t.time) : ""}</span>
        <span className="pill-open">▸</span>
      </button>
    </>
  );
}
