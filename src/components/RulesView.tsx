import { useEffect, useState } from "react";
import { api, type RiskLevel, type RiskRule, type RuleView } from "../api";
import { useT } from "../i18n";
import { elapsed } from "../useInbox";

/**
 * Two lists that both shape what the inbox does with a call: which ones skip
 * it entirely, and which ones it makes loud.
 */
export function RulesView() {
  const t = useT();
  const [allowRules, setAllowRules] = useState<RuleView[]>([]);
  const [risk, setRisk] = useState<RiskRule[]>([]);
  const [adding, setAdding] = useState(false);
  const totalHits = allowRules.reduce((sum, rule) => sum + rule.hits, 0);

  const levels: { value: RiskLevel; label: string }[] = [
    { value: "danger", label: t.rules.danger },
    { value: "caution", label: t.rules.caution },
  ];

  useEffect(() => {
    void api.listRules().then(setAllowRules);
    void api.listRiskRules().then(setRisk);
  }, []);

  const save = (next: RiskRule[]) => {
    setRisk(next);
    void api.setRiskRules(next).then(setRisk);
  };

  const update = (index: number, patch: Partial<RiskRule>) =>
    save(risk.map((rule, i) => (i === index ? { ...rule, ...patch } : rule)));

  const add = (pattern: string) => {
    setAdding(false);
    if (!pattern.trim()) return;
    save([
      {
        pattern: pattern.trim(),
        level: "danger",
        icon: "⚠",
        label: pattern.trim(),
        key: null,
        enabled: true,
      },
      ...risk,
    ]);
  };

  /// Rules read differently per language, so the sentence is built here from
  /// the parts rather than shipped as prose from Rust.
  const describe = (rule: RuleView) => {
    // A prefix rule stores no signature, so without its own branch it would
    // be described as covering every call of the tool — far wider than it
    // does, on the one screen where that has to be exact.
    const what = rule.prefix
      ? t.rules.startingWith(rule.prefix)
      : (rule.signature ?? t.rules.everyCall(rule.toolName));
    return `${what} — ${rule.project ?? t.rules.everywhere}`;
  };

  return (
    <div className="rules">
      {/* Auto-allow first: these are the rules that make calls stop asking,
          so they are the ones worth finding again. Highlighting rules only
          change how a row looks. */}
      <h2>{t.rules.allowTitle}</h2>
      {allowRules.length === 0 ? (
        <p className="note">{t.rules.allowEmpty}</p>
      ) : (
        <>
          {/* What these rules have actually done. Approvals made on your
              behalf are invisible by design, so without a tally there is no
              way to tell a rule that saved you fifty clicks from one that is
              quietly approving more than you meant it to. */}
          <p className="note">{t.rules.silentTotal(totalHits)}</p>
          <ul>
            {allowRules.map((rule, index) => (
              <li key={`${rule.toolName}-${index}`}>
                <span className="rule-label">
                  <span className="rule-what">{describe(rule)}</span>
                  <em className={rule.hits === 0 ? "unused" : ""}>
                    {rule.hits === 0
                      ? t.rules.neverUsed
                      : t.rules.usedTimes(rule.hits, elapsed(rule.lastHitAt!, t.time))}
                  </em>
                </span>
                <button onClick={() => api.removeRule(index).then(setAllowRules)}>
                  {t.rules.remove}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      <h2 className="secondary">{t.rules.riskTitle}</h2>
      <p className="note">{t.rules.riskHint}</p>

      <div className="rules-actions">
        <button onClick={() => setAdding(true)}>{t.rules.add}</button>
        <button onClick={() => api.restoreRiskDefaults().then(setRisk)}>
          {t.rules.restoreDefaults}
        </button>
      </div>

      {adding && (
        <input
          autoFocus
          className="rename"
          placeholder={t.rules.newPattern}
          onBlur={(e) => add(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setAdding(false);
            e.stopPropagation();
          }}
        />
      )}

      <ul className="risk-list">
        {risk.map((rule, index) => (
          <li key={`${rule.pattern}-${index}`} className={rule.enabled ? "" : "off"}>
            <input
              type="checkbox"
              checked={rule.enabled}
              onChange={(e) => update(index, { enabled: e.target.checked })}
            />
            <input
              className="icon-field"
              value={rule.icon}
              maxLength={2}
              onChange={(e) => update(index, { icon: e.target.value })}
              onKeyDown={(e) => e.stopPropagation()}
            />
            <span className="risk-text">
              <code>{rule.pattern}</code>
              <em>{(rule.key && t.risk[rule.key]) || rule.label}</em>
            </span>
            <select
              value={rule.level}
              onChange={(e) => update(index, { level: e.target.value as RiskLevel })}
            >
              {levels.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
            <button onClick={() => save(risk.filter((_, i) => i !== index))}>×</button>
          </li>
        ))}
      </ul>

    </div>
  );
}
