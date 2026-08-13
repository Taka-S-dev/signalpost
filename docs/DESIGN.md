# Design notes

Why the app is built the way it is, and what was learned the expensive way.

The README says what Signalpost does. This file records the decisions behind
it and the facts that took measurement to establish — the things that leave no
trace in the code, and that cost real time to rediscover.

Verified facts are marked with the date they were checked, because several of
them are undocumented behaviour that could change.

---

## 1. The parked hook

The whole app rests on one trick: `PermissionRequest` can be an HTTP hook, the
hook's response becomes the verdict, and a hook is allowed 600 seconds. So the
response is **held open** until the user presses a key.

Everything else — the panel, the bar, the tray — exists to make that 600-second
window usable.

**The response shape is undocumented and the published docs are wrong.**

```json
{ "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": { "behavior": "allow" } } }
```

`decision` is an object with a `behavior` field, not a string. The denial text
key is `message`, not `reason`. The docs show `"decision": "allow"`, which the
implementation does not read — the check inside `claude.exe` is
`e.hookSpecificOutput.decision.behavior === "allow"`.

A wrong shape **fails silently**. It looks exactly like the app being ignored,
which is how the first working build was mistaken for a broken one.

**It fails safe.** Hold expires at 570s, ten seconds inside the limit, and
returns no decision — the editor then prompts as usual. A closed app, a
crashed app and an unanswered row all end the same way: the session is never
stuck, only slower.

---

## 2. What the hooks can and cannot tell us

This section is the most valuable thing in this file. Most of it is *negative*
knowledge — mechanisms that look like they should work and do not.

| Event | Fires | Usable as |
| --- | --- | --- |
| `PermissionRequest` | when a call needs approving | the row, parked |
| `PreToolUse` | **before** the permission decision | nothing useful here |
| `PostToolUse` | **after the tool finishes running** | the call was settled |
| `PermissionDenied` | **auto mode denials only** | almost never fires |
| `Notification` | questions, and Claude's own prompt appearing | rows |
| `Stop` | turn ended | retire anything still parked |
| `SessionEnd` | session ended | clean up its rows |

Checked 2026-08-13 against the published hook documentation. The response
shape in §1 is the one thing here read out of the binary, because the docs
contradict it.

### There is no signal for "the user approved it"

No `PermissionGranted` event exists. `PreToolUse` fires *before* the decision,
so it cannot report one. `PermissionDenied` fires only when auto mode denies —
not when a person denies in the editor — so wiring it to the settle endpoint is
close to inert, and it is registered only because it costs nothing.

That leaves `PostToolUse`, which fires **after the tool has finished running**.

### Claude Code does not hang up when the editor answers first

The handler retires its row when the client disconnects (`Parked`'s `Drop`).
Measured 2026-08-13 against the running app:

```
POST /hook/permission, then hold   → GET /queue: count 1
close the socket                   → GET /queue: count 0   (within 2s)
```

So disconnect detection works. But a row approved in VS Code was observed
lingering until the command finished, which means Claude Code **keeps the
connection open and ignores the response** rather than cancelling it.

**Consequence:** approve in the editor instead of the panel and the row stays
until the tool completes. For `ruff && pyright && pytest` that is tens of
seconds. There is no way around it with the events that exist.

### Why the row is not dismissed on a guess

A long-waiting row could be assumed settled and dropped. It is not, and should
not be. The one failure this app must never have is **silently discarding a
call that is still waiting for an answer** — that is worse than missing a
notification, because the session stalls for the full 600s with nothing on
screen. A stale row is merely confusing, and self-heals.

### `GET /queue`

Read-only, returns each row and how long it has waited. It exists because
"did that row get retired?" was otherwise only answerable by looking at the
panel, which cannot answer a timing question. The disconnect measurement above
was made with it.

Unlike the hooks it carries no token. It reports what the panel is already
showing on screen, and something has to answer without one so that "is the app
up?" is checkable.

---

## 2b. Who may post to the hooks

Loopback keeps other machines out but not other processes here, so every hook
URL carries a secret written on first run and kept next to the config. The
paths in `settings.json` read `/hook/<token>/permission`; anything else is
answered 404, which tells a caller guessing at the port nothing.

What that is and is not worth, measured 2026-08-13 on a running build:

- **A web page cannot post either way.** The endpoints require
  `application/json`, which is not a CORS-simple content type, so a browser
  must preflight — and nothing here answers a preflight. Sending `text/plain`
  instead is refused with 415. This was already true before the token.
- **Another account on the machine** reaches loopback but not, normally, the
  file. "Normally" is doing real work in that sentence: reading the ACL on the
  machine this was written on found a group some other tool had added with
  read access, and administrators can always read it.
- **Code running as the user cannot be kept out.** It can read the token like
  any other file. The token is not a defence against that and should not be
  described as one.

A forged request cannot make Claude Code execute anything — answering it
answers that request and nothing else. What it can do is draw a convincing row
in a panel people trust at a glance, which is the whole reason for the secret.

---

## 3. Answering safely

The app answers permission prompts on the user's behalf, so the defaults lean
toward asking again rather than deciding.

- **No key creates a standing rule.** `A` next to `Y` meant one slip could make
  a call permanent. Creating a rule is a checkbox ticked *before* answering.
- **Calls the risk rules mark dangerous cannot be remembered at all.** By
  definition those are the ones never to repeat unseen.
- **Bulk dismiss never touches permissions or questions.** It clears finished
  turns only. Everything else has something waiting on it.
- **A rule announces itself.** Ticking the box shows what will happen, which
  calls it covers, and where to delete it — before the rule exists. An undo bar
  offers it back for ten seconds afterwards.
- **Rules count their hits.** An approval made by a rule is invisible by
  design, so without a tally a rule scoped too wide could approve for weeks
  unnoticed. The count shows in the header and per rule.

---

## 4. Auto-allow rule scoping

Three scopes, and the middle one exists because the other two were unusable
for shell commands.

| Scope | Matches |
| --- | --- |
| exact call | the signature byte for byte |
| **command prefix** | commands opening with an editable prefix |
| tool in project | every call of that tool |

**Why the prefix scope was added.** "This exact call" compares the whole
command string. No two shell invocations are identical — one changed flag and
the rule never fires again. The first rules made in real use had zero hits and
never would have had any. The only alternative, "every call of this tool", is
effectively unrestricted for `Bash`. The prefix scope is the only one that both
survives an edited command and stays narrower than the whole project.

**The prefix ends on a word boundary** (whitespace or `; | &`). A plain
`starts_with` would let a rule for `npm run build` answer for
`npm run build-and-deploy`.

**The suggested prefix is only a suggestion.** It is the first three words, and
that guess is wrong the moment a command opens with `cd <path>;` — which is
common. The field is editable and shows back the sentence describing what it
will cover.

**A project rule covers subdirectories.** A session reports the directory it
was opened in, so a rule made at a repository root matched nothing from a
session opened in `frontend/`. The separator is required before the
subdirectory, so a rule for `app` cannot answer for a sibling `app-secrets`.

---

## 5. The panel as an object on the desktop

Ordinary web assumptions do not hold for a frameless always-on-top window.

- **DOM `mouseleave` is not reliable** for hover-to-expand: it also fires when
  a native tooltip opens over a button, which closed the panel on hover. The
  pointer is polled against the OS cursor instead, and a held mouse button
  counts as inside so a resize drag does not collapse it.
- **Geometry is saved, and must sometimes not be.** Corrective moves the app
  makes itself — placing the panel, switching shape — are suppressed for
  600ms, otherwise the correction is persisted and the bar walks up the screen
  each time it is expanded.
- **Sizes are sanity-checked on load.** A DPI transition produced a 259×344
  window at 3076,-1682, off every monitor. Out-of-range geometry falls back to
  the default, and there is a tray item to reset it.
- **`work_area()`, not `size()`**, or the bar hides under the taskbar.
- **Never take focus.** The panel appears without stealing keyboard focus, so
  it cannot interrupt typing.

---

## 6. Visual design

**A small internal scale rather than a vendor design system.** Not because
the vendor systems are wrong — a type scale and a spacing scale are exactly
what they supply, and taking Material's would have fixed the inconsistency
below as surely as writing five steps by hand did.

The reason is what else comes with them. A row here is about 54px for two
lines of text; Material's list components are specified taller than that, and
while its desktop density levels close some of the gap, a panel meant to be
read at a glance from across a desk wants to stay at the small end. Apple's
HIG describes Apple's platforms, and this is a Windows app. Fluent is the
coherent choice here and remains a reasonable move later — it is a bigger
change than the problem called for, not a worse one.

So the idea was taken without the identity: scales, no components.

**What was actually wrong was measured, not argued:**

| | Before | Now |
| --- | --- | --- |
| font sizes | 10 | 5 |
| paddings | ~25 | 6 |
| corner radii | 8 | 3 |
| focus indicators | **0** | ring on `:focus-visible` |
| control outline contrast | 1.4:1 | 3.0:1 |

The panel looked assembled rather than designed because every component had
brought its own numbers. Scales fixed that; a design system was not required.

**Colour means state.** The palette was Tokyo Night down to the hex values,
picked for being at hand rather than for saying anything, and every surface
carried a blue tint that meant nothing. Surfaces are neutral grey now. Hue is reserved for
waiting (amber), dangerous (red) and allowed (green). Selection and focus are
brightness, not hue, so the loudest thing on screen is a row that is waiting
rather than the row the cursor is on. A finished row wears no colour at all.

Project colours are muted deliberately: naming a project must not outshout the
amber that means something needs answering.

**Every colour is checked against all four surfaces** — 4.5:1 for text, 3:1
for control outlines. The figures above are the *worst* of the four, which is
the only number that means anything: quoting the panel background flatters
every colour, because the selected row is brighter than it. Two colours moved
a shade to clear the thresholds there. Re-measure after any palette change.

---

## 7. Codex

Codex has no hook that can answer an approval, so its rows are informational
only. The value is having Claude and Codex sessions in one list rather than two
places to watch.

`notify` accepts exactly **one** program, so installing cannot simply
overwrite it — whatever was there is chained behind the shim and restored on
uninstall. `toml_edit` keeps the rest of the user's config formatted as they
left it.

The shim must **read the HTTP response before exiting** or the request is lost.

Only `agent-turn-complete` is ever sent. The payload fields were found by
grepping the binary: `type`, `thread-id`, `turn-id`, `cwd`, `client`,
`input-messages`, `last-assistant-message`.

---

## 8. Naming

Signalpost. `signalbox` was the first choice and was dropped after a search
turned up an existing project of that name in the same niche.

The app has been renamed once and its config directory moved twice, so the
directory is migrated on startup and the Codex shim still recognises its
former binary name — an old registration can be removed by the current build
rather than left pointing at a binary that is gone.

---

## 9. Deliberately not done

- **Dismissing rows on a heuristic** — see §2.
- **A permanent hint strip** — it spent height every session answering a
  question asked in the first. The list moved behind `?`.
- **A running counter of auto-approvals in the bar** — being invisible is the
  point of a rule; the number belongs where you go to ask about it.
- **Making "always allow" easy to reach** — see §3.

---

## 9b. What the tests reach, and what they do not

Weighted towards the parts where being wrong is expensive rather than towards
a coverage number. `rules.rs` carries the most of any module because a
mistake there approves a call nobody looked at.

Covered: the verdict shape, the token guard, rule scoping and matching, the
danger invariant, risk marking, project colours, both installers, the Codex
shim's argument parsing, geometry sanity, session state.

**Not covered: the parked request itself.** `AppState` holds a
`tauri::AppHandle` tied to the concrete runtime, so the mock runtime cannot
build one, and the handler that parks a response cannot be driven in-process.
What is tested instead is everything that path is made of — the verdict it
returns, the guard in front of it — while the parking, the disconnect
retirement and the settle hooks are measured against a running build (§2).

That is a real gap and the honest fix is to make `AppState` generic over the
runtime, which reaches ui.rs and lib.rs as well. Until then the measurements
in §2 are the evidence, and they are dated for that reason.

The frontend holds what is actually its own: the elapsed-time formatting at
each of its three boundaries, and which row the keys will act on. Selection
is tracked by id rather than index so that a row resolving above the cursor
cannot slide a different one under it — a test asserts exactly that, because
the cost of it being wrong is answering a call nobody read.

Ordering and the collapsing of repeats are **not** frontend concerns despite
looking like it; both are in `state.rs`, and both are among the untestable
parts above.

Also uncovered: `ui.rs`, window placement that is largely Win32 and DPI
behaviour a test would only restate.

**Each of these was checked by breaking it on purpose** and confirming the
right test failed. One survived: deleting the clamp in `move` changed nothing
observable, because the guard that skips a missing row already covers a step
of one. The test that now distinguishes them asks for a jump past the end,
which lands on the end rather than doing nothing. A test suite nobody has
tried to break is a suite of unknown strength.

---

## 10. Open

- The lingering row after an editor approval (§2). Nothing to do until Claude
  Code either closes the connection or emits an approval event.
- Multiplatform. The approval path is portable; window focusing, the toast and
  the default jump command are Win32 / VS Code specific.
- VS Code hosts several sessions per window. They share a `cwd`, so the app can
  focus the window but not the tab.
