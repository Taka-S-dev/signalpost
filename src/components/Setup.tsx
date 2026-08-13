import { useEffect, useState } from "react";
import { api, type Lang, type PopupWhen, type Settings } from "../api";
import { useT } from "../i18n";

/** A neutral light option, then the project palette. */
const FRAME_EXTRA = "#d5dae3";

interface Props {
  installed: boolean;
  /// True once a hook has actually arrived since the config was written.
  live: boolean;
  port: number;
  settings: Settings;
  onSettings: (settings: Settings) => void;
  onChanged: (installed: boolean) => void;
  onDone: () => void;
}

interface ToggleProps {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}

function Toggle({ label, hint, checked, onChange }: ToggleProps) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        {label}
        <em>{hint}</em>
      </span>
    </label>
  );
}

/**
 * Shown until the hooks are wired up. Without them nothing ever reaches the
 * inbox, so this is the one screen that has to explain itself.
 */
export function Setup({ installed, live, port, settings, onSettings, onChanged, onDone }: Props) {
  const t = useT();
  const [message, setMessage] = useState<string | null>(null);
  const [codex, setCodex] = useState(false);
  const [codexMessage, setCodexMessage] = useState<string | null>(null);
  const [keepExisting, setKeepExisting] = useState(true);
  const [palette, setPalette] = useState<string[]>([]);
  const [active, setActive] = useState<string | null>(null);

  useEffect(() => {
    void api.codexInstalled().then(setCodex);
    void api.palette().then(setPalette);
    void api.activeShortcut().then(setActive);
  }, []);

  const toggleCodex = async () => {
    try {
      if (codex) {
        await api.uninstallCodex();
        setCodexMessage(t.codex.removed);
        setCodex(false);
      } else {
        const path = await api.installCodex(keepExisting);
        setCodexMessage(t.codex.wrote(path));
        setCodex(true);
      }
    } catch (error) {
      setCodexMessage(String(error));
    }
  };

  const install = async () => {
    try {
      const path = await api.installHooks();
      setMessage(t.setup.wrote(path));
      onChanged(true);
    } catch (error) {
      setMessage(String(error));
    }
  };

  const uninstall = async () => {
    try {
      await api.uninstallHooks();
      setMessage(t.setup.removed);
      onChanged(false);
    } catch (error) {
      setMessage(String(error));
    }
  };

  return (
    <div className="setup">
      <h2>{t.setup.title}</h2>
      {/* Three states, not two. Hooks are read when a session starts, so
          "written to the file" and "actually in effect" are different facts,
          and showing only the first made a working setup and a stale one look
          identical. */}
      <p className={`status ${!installed ? "warn" : live ? "ok" : "warn"}`}>
        {!installed
          ? t.setup.notInstalled
          : live
            ? t.setup.live
            : t.setup.needsRestart}
      </p>
      {installed && !live && <p className="note">{t.setup.needsRestartHint}</p>}
      <p className="note">{t.setup.explain(port)}</p>
      <div className="actions">
        {installed ? (
          <button onClick={uninstall}>{t.setup.uninstall}</button>
        ) : (
          <button className="primary" onClick={install}>
            {t.setup.install}
          </button>
        )}
        <button onClick={onDone}>
          {t.setup.done} <kbd>Esc</kbd>
        </button>
      </div>
      {message && <p className="note">{message}</p>}

      <h2>{t.codex.title}</h2>
      <p className={`status ${codex ? "ok" : "warn"}`}>
        {codex ? t.codex.installed : t.codex.notInstalled}
      </p>
      <p className="note">{t.codex.explain}</p>
      {!codex && (
        <Toggle
          label={t.codex.keepExisting}
          hint={t.codex.keepExistingHint}
          checked={keepExisting}
          onChange={setKeepExisting}
        />
      )}
      <button className={codex ? "" : "primary"} onClick={toggleCodex}>
        {codex ? t.codex.uninstall : t.codex.install}
      </button>
      {codexMessage && <p className="note">{codexMessage}</p>}

      <h2>{t.setup.display}</h2>

      <div className="command">
        <span>
          {t.setup.shortcut}
          <em>
            {active && active !== settings.shortcut
              ? t.setup.shortcutTaken(settings.shortcut, active)
              : t.setup.shortcutHint}
          </em>
        </span>
        <input
          key={settings.shortcut}
          defaultValue={settings.shortcut}
          placeholder="Alt+Space"
          onBlur={(e) => {
            const value = e.target.value.trim() || "Alt+Space";
            onSettings({ ...settings, shortcut: value });
            void api.setShortcut(value).then(setActive);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            e.stopPropagation();
          }}
        />
      </div>

      <label className="slider">
        <span>{t.setup.language}</span>
        <select
          value={settings.lang}
          onChange={(e) => onSettings({ ...settings, lang: e.target.value as Lang })}
        >
          <option value="auto">{t.setup.languageAuto}</option>
          <option value="ja">日本語</option>
          <option value="en">English</option>
        </select>
      </label>

      <label className="slider">
        <span>
          {t.setup.popup}
          <em>{t.setup.popupHint}</em>
        </span>
        <select
          value={settings.popup}
          onChange={(e) =>
            onSettings({ ...settings, popup: e.target.value as PopupWhen })
          }
        >
          <option value="permission">{t.setup.popupPermission}</option>
          <option value="all">{t.setup.popupAll}</option>
          <option value="never">{t.setup.popupNever}</option>
        </select>
      </label>

      <Toggle
        label={t.setup.keepOpen}
        hint={t.setup.keepOpenHint}
        checked={settings.keepOpen}
        onChange={(keepOpen) => onSettings({ ...settings, keepOpen })}
      />
      <Toggle
        label={t.setup.hoverExpand}
        hint={t.setup.hoverExpandHint}
        checked={settings.hoverExpand}
        onChange={(hoverExpand) => onSettings({ ...settings, hoverExpand })}
      />
      <Toggle
        label={t.setup.toast}
        hint={t.setup.toastHint}
        checked={settings.toast}
        onChange={(toast) => onSettings({ ...settings, toast })}
      />
      <Toggle
        label={t.setup.sound}
        hint={t.setup.soundHint}
        checked={settings.sound}
        onChange={(sound) => onSettings({ ...settings, sound })}
      />
      <Toggle
        label={t.setup.flash}
        hint={t.setup.flashHint}
        checked={settings.flash}
        onChange={(flash) => onSettings({ ...settings, flash })}
      />
      <Toggle
        label={t.setup.autoHide}
        hint={t.setup.autoHideHint}
        checked={settings.autoHide}
        onChange={(autoHide) => onSettings({ ...settings, autoHide })}
      />

      <div className="toggle">
        <span>
          {t.setup.resetPosition}
          <em>{t.setup.resetPositionHint}</em>
        </span>
        <button onClick={() => api.resetPanelPosition()}>{t.setup.reset}</button>
      </div>

      <div className="command">
        <span>
          {t.setup.border}
          <em>{t.setup.borderHint}</em>
        </span>
        <div className="swatches">
          <button
            className={`swatch pick none ${settings.border ? "" : "on"}`}
            title={t.setup.borderNone}
            onClick={() => onSettings({ ...settings, border: "" })}
          />
          {[FRAME_EXTRA, ...palette].map((color) => (
            <button
              key={color}
              className={`swatch pick ${settings.border === color ? "on" : ""}`}
              style={{ background: color }}
              title={color}
              onClick={() => onSettings({ ...settings, border: color })}
            />
          ))}
        </div>
      </div>

      <label className="slider">
        <span>
          {t.setup.opacity}
          <em>{t.setup.opacityHint}</em>
        </span>
        <input
          type="range"
          min={0.4}
          max={1}
          step={0.05}
          value={settings.opacity}
          onChange={(e) => onSettings({ ...settings, opacity: Number(e.target.value) })}
        />
        <output>{Math.round(settings.opacity * 100)}%</output>
      </label>
    </div>
  );
}
