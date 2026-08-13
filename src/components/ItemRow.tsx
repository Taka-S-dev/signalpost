import { useEffect, useState } from "react";
import { api, type Item, type Decision, type Scope, type ScopeKind } from "../api";
import { useT } from "../i18n";
import { elapsed } from "../useInbox";
import { Detail } from "./Detail";

interface Props {
  item: Item;
  selected: boolean;
  remember: Scope | null;
  onRemember: (scope: Scope | null) => void;
  onSelect: () => void;
  onResolve: (decision: Decision, remember?: Scope) => void;
  onDismiss: () => void;
  onOpenEditor: () => void;
}

export function ItemRow({
  item,
  selected,
  remember,
  onRemember,
  onSelect,
  onResolve,
  onDismiss,
  onOpenEditor,
}: Props) {
  const t = useT();
  const permission = item.kind === "permission";
  // Derived in Rust so the suggestion and the matching it feeds stay one
  // implementation. Held here so switching scopes back and forth does not
  // discard what the user typed in between.
  const [prefix, setPrefix] = useState("");
  useEffect(() => {
    if (!permission) return;
    void api.suggestPrefix(item.signature).then(setPrefix);
  }, [item.signature, permission]);

  const scopeOf = (kind: ScopeKind): Scope =>
    kind === "commandPrefix" ? { kind, prefix } : ({ kind } as Scope);

  const project = item.label || item.project;
  // Anything the risk rules call dangerous is, by definition, the kind of
  // call you never want repeated without looking. Those cannot be turned
  // into a standing rule from here at all.
  const dangerous = item.risk?.level === "danger";
  // How long this has been waiting, expressed as something the eye can read
  // without parsing a number. Only blocked calls age: nothing is waiting on a
  // finished turn.
  const waited = permission ? Date.now() - item.createdAt : 0;
  const heat = Math.min(waited / (10 * 60_000), 1);
  const stale = waited > 3 * 60_000;
  // Seeded rules carry a key so they read correctly in either language;
  // rules the user wrote show exactly what they typed.
  const riskLabel = item.risk
    ? (item.risk.key && t.risk[item.risk.key]) || item.risk.label
    : "";

  return (
    <li
      className={`row ${selected ? "is-selected" : ""} kind-${item.kind} ${
        item.risk ? `risk-${item.risk.level}` : ""
      } ${stale ? "is-stale" : ""}`}
      style={{ borderLeftColor: item.color, "--heat": heat } as React.CSSProperties}
      onClick={onSelect}
      aria-current={selected}
    >
      <div className="row-head">
        <span className="project" style={{ color: item.color }}>
          {item.label || item.project}
        </span>
        {/* Only Claude rows can be answered here, so the source has to be
            visible rather than inferred from which buttons appear. */}
        {item.agent === "codex" && <span className="agent">codex</span>}
        {/* Both, always. The risk mark used to replace the state badge, which
            hid whether a row was still waiting exactly on the rows where that
            matters most. */}
        <span className="badge">{t.kind[item.kind]}</span>
        {item.risk && (
          <span className="risk">
            {item.risk.icon} {riskLabel}
          </span>
        )}
        <span className={`elapsed ${stale ? "is-stale" : ""}`}>
          {elapsed(item.createdAt, t.time)}
        </span>
      </div>

      <div className="row-body">
        {item.toolName && <span className="tool">{item.toolName}</span>}
        <span className="summary">{item.summary}</span>
      </div>

      {selected && <Detail item={item} />}

      {/* The decision is allow or deny; everything else is a variation on it.
          Giving all five the same weight both scattered the eye and put a
          rule-creating button the same size as the one-off answer. */}
      {selected && (
        <>
          {permission && (
            <>
              {/* Creating a rule is now a decision made *before* answering,
                  not a button sitting next to the answer. A single keypress
                  should never be able to make a call permanent. */}
              <label className={`remember ${dangerous ? "blocked" : ""}`}>
                <input
                  type="checkbox"
                  checked={remember !== null}
                  disabled={dangerous}
                  onChange={(e) =>
                    onRemember(e.target.checked ? { kind: "commandPrefix", prefix } : null)
                  }
                />
                <span>
                  {t.actions.rememberLabel}
                  {dangerous && <em>{t.actions.rememberBlocked}</em>}
                </span>
              </label>
              {remember !== null && !dangerous && (
                <>
                  <select
                    className="remember-scope"
                    value={remember.kind}
                    onChange={(e) => onRemember(scopeOf(e.target.value as ScopeKind))}
                  >
                    {/* First, because it is the only one that both survives a
                        changed command and stays narrower than the whole
                        project. */}
                    <option value="commandPrefix">{t.actions.scopePrefix}</option>
                    <option value="exactCall">{t.actions.scopeCall(project)}</option>
                    <option value="toolInProject">
                      {t.actions.scopeTool(item.toolName, project)}
                    </option>
                  </select>
                  {remember.kind === "commandPrefix" && (
                    <>
                      {/* Editable, and it has to be: the suggestion is three
                          words off the front, which is wrong the moment a
                          command opens with `cd <path>;`. */}
                      <input
                        className="remember-prefix"
                        value={remember.prefix}
                        placeholder={t.actions.prefixPlaceholder}
                        onChange={(e) => {
                          setPrefix(e.target.value);
                          onRemember({ kind: "commandPrefix", prefix: e.target.value });
                        }}
                        onKeyDown={(e) => e.stopPropagation()}
                      />
                      <p className="remember-note">
                        {remember.prefix.trim()
                          ? t.actions.prefixMatches(remember.prefix.trim(), project)
                          : t.actions.prefixEmpty}
                      </p>
                    </>
                  )}
                  {/* Where to undo this belongs here, at the moment the rule
                      is about to exist — not in a settings screen you would
                      have to already know about to go looking. */}
                  <p className="remember-note">{t.actions.rememberWhere}</p>
                </>
              )}

              <div className="actions decide">
                <button className="primary" onClick={() => onResolve("allow", remember ?? undefined)}>
                  {t.actions.allow} <kbd>Y</kbd>
                </button>
                <button className="danger" onClick={() => onResolve("deny")}>
                  {t.actions.deny} <kbd>N</kbd>
                </button>
              </div>
            </>
          )}
          {/* A question is answered in the editor, so getting there is the
              action — not dismissing it, which only hides the fact that a
              session is still stopped. */}
          {item.kind === "needsInput" && (
            <div className="actions decide">
              <button className="primary" onClick={onOpenEditor}>
                {t.actions.answerThere} <kbd>↵</kbd>
              </button>
            </div>
          )}

          <div className="actions minor">
            {!permission && (
              <button onClick={onDismiss}>
                {t.actions.dismiss} <kbd>D</kbd>
              </button>
            )}
            {item.kind !== "needsInput" && (
              <button onClick={onOpenEditor}>
                {t.actions.openEditor} <kbd>↵</kbd>
              </button>
            )}
          </div>
        </>
      )}
    </li>
  );
}
