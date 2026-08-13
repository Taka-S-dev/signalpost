# ClaudeNotify

*[日本語版はこちら](README.ja.md)*

A tray app that collects the approval prompts from every Claude Code session
you are running **and lets you answer them in one place**.

The point is not to notify you so you go and find the right window — it is to
make finding the window unnecessary. Approvals are answered from the panel, so
the sessions never need to be visited at all.

## How it works

Claude Code's `PermissionRequest` hook can be an HTTP hook, and whatever it
returns becomes the verdict. A hook is allowed 600 seconds.

ClaudeNotify **holds that response open** and only answers when you press a key.

```
Claude Code ──POST /hook/permission──▶ ClaudeNotify (response parked)
                                              │
                                       a row appears
                                              │
                                     you press Y
                                              ▼
Claude Code ◀──── verdict ────────────── response released → session resumes
```

The response has to be shaped exactly like this:

```json
{ "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": { "behavior": "allow" } } }
```

`decision` is an **object with a `behavior` field**, not a string, and the
denial text is `message`, not `reason`. The published docs show
`"decision": "allow"`, but the implementation
(`e.hookSpecificOutput.decision.behavior === "allow"` inside `claude.exe`)
does not read that. A wrong shape **fails silently**, which looks exactly like
the app being ignored.

It fails safe: if the app is closed, or nothing is pressed within 570 seconds,
it returns **no decision** and the prompt appears in the editor as usual.
Nothing can end up stuck.

## Getting started

1. Run the app — it lives in the tray.
2. The **Settings** screen opens on first run. Press **Install hooks**.
   They are merged into `~/.claude/settings.json`, keeping your existing
   settings and writing a `.bak` first.
3. Restart any running Claude Code session.

From then on the panel appears when something needs approving. It **does not
take keyboard focus**, so it never interrupts what you are typing. Press
`Alt+Space` when you are ready to deal with it.

### Keys

| Key | Action |
| --- | --- |
| `Alt+Space` | Show / hide the panel (global) |
| `J` / `K` | Move between rows |
| `Y` | Allow |
| `N` | Deny |
| `A` | Allow + **always allow this exact call** |
| `Shift+A` | Allow + **always allow this tool in this project** |
| `Enter` | Bring that session's window to the front |
| `D` | Dismiss an informational row |
| `W` / `P` / `R` / `S` | Windows / colors / rules / settings |
| `Esc` | Hide the panel; from any other view, go back to the inbox |

### Ordering

Blocked calls first, oldest at the top. Nothing can sit forgotten at the
bottom of the list.

### Projects

Each project (`cwd`) gets a colour, derived from the path so it is there
without any configuration. Rename it or pick a different colour under `P`;
both persist in `projects.json`.

Identity is keyed on the folder rather than the session id, because that is
what survives a restart and what maps one-to-one to an editor window.

`Enter` runs `code -r "{cwd}"` by default. Sessions run from a terminal can
use something else — `wt -d "{cwd}"`, for instance — set per project.

### Highlighting risky calls

Rules match a case-insensitive substring of the command and give the row an
icon, a label and a colour. Danger outranks caution; within a level the longer
match wins, so `git push --force` reads as a force push rather than a push.

Fifteen rules ship enabled by default (force push, `reset --hard`, `rm -rf`,
`DROP TABLE`, `npm publish`, `terraform apply`, …). Edit them under `R`.

### Feedback

| Setting | What it does | Default |
| --- | --- | --- |
| Sound | A different cue per row type | on |
| Flash | Amber on arrival, green when the last row clears | on |
| Hide when empty | Collapse the panel at zero rows | off |
| Background opacity | 40–100%. Text and borders stay opaque | 100% |

"Hide when empty" is off by default because a window that disappears and
reappears on its own reads as flicker. The flash acknowledges the clear
instead.

### Language

English and Japanese, following the OS by default. Change it under `S`.

## Codex CLI

Codex sessions show up in the same list, tagged `codex`. They are
informational only: Codex fires `notify` on turn completion and has no hook
that can return an approval, so there is nothing to answer.

Codex allows exactly one `notify` program, so installing cannot simply take
the slot. A small shim goes in front and chains whatever was already there:

```toml
notify = ["…\\claudenotify-codex.exe", "--chain", "…\\your-program.exe", "its-arg"]
```

The shim posts the event JSON to the panel and then runs the original program
with the arguments it would have received. Removing the wiring puts the
original back. Both are done from **Settings**.

## Hooks it installs

| Event | Endpoint | Purpose |
| --- | --- | --- |
| `PermissionRequest` | `/hook/permission` | Parks the call and shows the row (timeout 600s) |
| `Notification` | `/hook/notification` | Questions and finished turns |
| `PostToolUse` | `/hook/tool-settled` | Retires a row approved in the editor |
| `PermissionDenied` | `/hook/tool-settled` | Retires a row denied in the editor |
| `Stop` | `/hook/turn-end` | Clears anything still parked when the turn ends |
| `SessionEnd` | `/hook/session-end` | Cleans up a finished session's rows |

No matchers are used; the app filters by type itself, so a notification type
added later cannot silently stop arriving.

The server binds `127.0.0.1` only. The port defaults to `8787` and can be
changed with `CLAUDENOTIFY_PORT` — reinstall the hooks afterwards.

## Auto-allow rules

Rules created with `A` / `Shift+A` are stored in `auto-allow.json`, and
matching calls are **allowed immediately without ever reaching the panel**.
The queue gets quieter the more you use it. List and remove them under `R`.

Matching is tool name plus the scope you chose — there are no wildcards.
`A` (this exact call) is the narrowest, so a rule can never allow more than
what you approved.

## Development

```sh
npm install
npm run tauri dev      # develop
npm run tauri build    # produce an NSIS installer
cargo test --manifest-path src-tauri/Cargo.toml
```

- Frontend: React 19 + TypeScript + Vite
- Backend: Rust / Tauri 2 / axum

Config lives in the app config directory: `auto-allow.json`, `projects.json`,
`risk.json`, `settings.json`, `window.json`.

## Limitations

- Windows only. The approval path itself is portable, but the window list and
  the default jump command are Win32 / VS Code specific.
- VS Code can host several sessions in one window. They share a `cwd`, so
  `Enter` can focus the window but not the tab.
- `code` must be on `PATH` (VS Code's "Shell Command: Install 'code' command
  in PATH").
