# Welcome Screen — the default empty-workspace surface

> _Design note. Status: **PLANNED**. Nothing here ships yet. The "Today"
> section records the behaviour this change replaces — it is the evidence for
> the decisions that follow. AI-generated from the source; where it disagrees
> with the code, the code is authoritative._

Fresh has three different things it can show when no file is open, none of
them a starting point. This note designs a fourth — an **interactive welcome
screen** — makes it the default for both empty-workspace paths (launch with
nothing to restore, and closing the last buffer), and keeps the current
behaviours reachable by configuration.

The screen has one job: from a standing start, put the user one keystroke away
from the two things Fresh is actually for — **editing a project** and
**running agents across worktrees** — without becoming a dashboard.

---

## 1. Today

There are three empty states, selected by config, and one plugin that
partially covers the same ground.

**1. `[No Name]` scratch buffer** (`editor.auto_create_empty_buffer_on_last_buffer_close = true`,
the default). `fresh` with no arguments, and closing the last tab, both leave
an empty untitled buffer with a cursor in it. It is a text buffer that will
never be saved, occupying the tab strip and the status bar. It answers no
question the user has.

**2. The blank pane** (same setting, `false`). The editor still synthesizes a
buffer — the invariant is that at least one exists — but marks it
`synthetic_placeholder`: hidden from the tab strip, skipped by the pane
renderer, and suppressed in the status bar's buffer-specific elements. The
pane paints one centred, subdued line:

```text
                Ctrl+P  command palette   ·   Ctrl+O  open file   ·   Ctrl+E  file explorer
```

That line is the whole of the current welcome experience. It is honest and it
is inert: nothing on it can be focused, clicked, or activated, and it names
three keys out of the dozen high-level surfaces the editor actually has.

**3. The Dashboard plugin** (`plugins.dashboard.enabled`, off by default, with
`auto-open` gating the ambient paths). This is the closest existing thing: a
read-only virtual buffer that opens at startup and after the last buffer
closes, auto-centres, repaints on `viewport_changed`, routes clicks through
the `mouse_click` hook, and drives row focus itself through a `defineMode`
mode. It is the proven pattern for this kind of screen, and much of its
mechanism is reused below. But it is an **information** surface — git status,
disk usage, weather, PRs — not a launcher, it is opt-in, and being a plugin it
cannot be the editor's default (see §3).

### What is wrong with all three

1. **Nothing is actionable.** Every empty state is either a text cursor or a
   sentence. The user's next move is always "remember a keybinding".
2. **The context the editor already holds is thrown away.** The workspace
   cache under `<data_dir>/workspaces/` has one entry per directory ever
   opened, each with its label, its saved layout and its `saved_at`; boot
   discovery already reads them all into `PersistedWindow`s. The editor knows
   exactly which projects you work in and which of them currently have an
   agent session — and shows none of it.
3. **The agentic half of the product is invisible from a cold start.** The
   Orchestrator — worktree-per-task, an agent per workspace, resumable
   sessions, the dock — is reachable only by knowing `Alt+O`, or by finding
   "Orchestrator: New Session" in the palette. For a new user it does not
   exist.
4. **The one screen that could do this is opt-in and off.** Making the
   dashboard the default would mean making network-fetching sections and a
   plugin dependency part of the zero-configuration promise.

---

## 2. Principles

- **Minimal.** One screen, no scrolling, no panels, no borders around the
  whole thing, no ASCII-art logo. If a row is not something a user does in
  their first ten seconds, it is not on the screen. The palette is one
  keystroke away and holds everything else.
- **Verbs on the left, context on the right.** The left column is what you can
  *do*; the right column is where you *were*. Both halves cover editing and
  agents — the agentic rows are not a separate mode, they are more verbs.
- **Every row is real.** Each row dispatches an action that already exists and
  displays its live keybinding, resolved from the keymap, not a hardcoded
  string. A row whose action is unavailable in this build or session is not
  shown greyed — it is not shown.
- **It is not a buffer.** Nothing to close, nothing to save, nothing in the
  tab strip, nothing serialized into the workspace file.
- **It gets out of the way instantly.** Opening anything replaces it; closing
  everything brings it back. There is no dismiss.
- **Zero configuration, but configurable.** Default on, with both previous
  behaviours reachable by one setting.

---

## 3. Where it lives: a render mode for the placeholder pane

The pivotal decision, and the one that removes most of the potential
complexity.

The welcome screen is **not a buffer**. It is what the existing
`synthetic_placeholder` pane paints *instead of* the one-line hint. The
placeholder buffer already exists for exactly this situation, is already
hidden from tabs, already skipped by the pane renderer, already suppressed in
the status bar, and is already stripped from workspace serialization.

Everything else falls out of that:

| Question | Answer, by construction |
|---|---|
| How is it dismissed? | It isn't. Open a file and the placeholder is replaced. |
| Does it take a tab? | No. `hidden_from_tabs` is already set. |
| Is it persisted? | No. Virtual/placeholder buffers are stripped on save. |
| What about `hot_exit`? | Unaffected — there is no content to recover. |
| Does it fight session restore? | No. If a session restores, there is no placeholder, so no welcome screen. |
| Splits? | Only the placeholder pane draws it; a split showing a real buffer is untouched. |

### Why core Rust and not a plugin

The dashboard proves the plugin pattern works, and it would be the faster
build. It is still the wrong home for a default:

- **Timing.** The dashboard deliberately does not auto-open at module load,
  because `init.ts` has not been evaluated yet and would race its own
  `setAutoOpen`. A default startup surface that arrives a few frames late,
  after an empty pane, is worse than no animation at all.
- **`plugins` is a cargo feature.** A build with plugins compiled out would
  have no welcome screen. The default UX cannot depend on an optional
  feature.
- **The data is already core-side.** Workspace discovery, `PersistedWindow`,
  window materialization (`set_active_window`) and the keymap resolver are all
  Rust. A plugin implementation would have to ask the host for all of it.
- **Zero configuration is the product's stated pitch.** "The default start
  screen requires enabling a plugin" contradicts it.

The extension seam (§9) keeps the plugin route open for *contributions* to the
screen, which is where plugins earn their place.

---

## 4. The screen

### 4.1 `fresh` with no arguments, nothing to restore

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  File   Edit   View   Selection   Go   LSP   Explorer   Help                                                         │   <- menu bar, unchanged
 │  +                                                                                                                   │   <- tab strip: no tabs, only the [+] button
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │               fresh 0.4.10                                        ~/src/fresh   main                                 │   <- wordmark, then where you are (muted)
 │               ──────────────────────────────────────────────────────────────────────                                 │
 │                                                                                                                      │
 │               START                                         RESUME                                                   │   <- left: verbs.  right: context.
 │                                                                                                                      │
 │               ▸ Open File…                    Ctrl+O          ↺ Reopen last session · 5 tabs                         │   <- focused row; Enter activates it
 │                 Open Folder…                                  ● fresh              claude          2m                │   <- ● live session  ○ dormant  (blank: never opened as one)
 │                 New File                      Ctrl+N          ○ fresh-docs         codex           1h                │
 │                 Search in Project…            Alt+/             dotfiles                           2d                │
 │                                                                 nixpkgs                            6d                │
 │               AGENTS                                                                                                 │
 │                                                                                                                      │
 │                 New Agent Workspace…                                                                                 │   <- the agentic half is shaped like the editing half
 │                 Run Agent Here…                                                                                      │
 │                 Open Sessions Dock            Alt+O                                                                  │
 │                                                                                                                      │
 │               ──────────────────────────────────────────────────────────────────────                                 │
 │               type to search  ·  ↑↓ move  ·  Tab column  ·  Enter open  ·  Ctrl+P palette  ·  F1 help                │   <- one hint line — the only chrome the screen adds
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │  Restricted   Local                                                                                 Palette: Ctrl+P  │   <- {cursor}/{encoding}/{language} stay suppressed
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

The block is centred on both axes in the pane, exactly as the dashboard
centres itself, and repaints on `viewport_changed`.

### 4.2 After closing the last buffer, with the explorer and the dock open

The screen has no chrome of its own and no fixed size: it lays out inside
whatever the pane is, alongside the file explorer and the Orchestrator dock.
Below 76 columns the two columns fold into one, `RESUME` drops its heading and
keeps its top three rows, and the hint line shortens.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  File   Edit   View   Selection   Go   LSP   Explorer   Help                                                         │
 │                                                  +                                                                   │   <- explorer and dock keep their own chrome
 │ Sessions        [×]   fresh                                                                                          │
 │ ──────────────────    ▾ crates                                                                                       │
 │ ▸ fresh               ▸ docs                                                                                         │
 │   claude ● working    ▸ scripts                                                                                      │
 │                         Cargo.toml                                                                                   │
 │   fresh-docs            README.md                                                                                    │
 │   codex  ○ idle                                                                                                      │
 │                                                                                                                      │
 │ ──────────────────                                          fresh 0.4.10       ~/src/fresh  main                     │
 │ + New Workspace…                                            ────────────────────────────────────────────             │   <- under 76 cols the two columns fold into one
 │                                                             ▸ Open File…                   Ctrl+O                    │
 │                                                               Open Folder…                                           │
 │                                                               New File                     Ctrl+N                    │
 │                                                               Search in Project…           Alt+/                     │
 │                                                               New Agent Workspace…                                   │
 │                                                               Run Agent Here…                                        │
 │                                                               Open Sessions Dock           Alt+O                     │
 │                                                             ────────────────────────────────────────────             │   <- RESUME keeps its top 3 and loses the header
 │                                                             ● fresh             claude         2m                    │
 │                                                             ○ fresh-docs        codex          1h                    │
 │                                                               dotfiles                         2d                    │
 │                                                             ────────────────────────────────────────────             │
 │                                                             type to search · ↑↓ · Enter · Ctrl+P                     │   <- hint line shortens with the pane
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │  Trusted   Local                                                                                    Palette: Ctrl+P  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 The smallest pane that still draws it

Below 44x13 the screen falls back to today's single centred hint line, and
below that, to nothing. A welcome screen that wraps is worse than no welcome
screen.

```text
 ┌──────────────────────────────────────────┐
 │  File  Edit  View  …                     │
 │  +                                       │
 │  fresh 0.4.10                            │
 │  ──────────────────────────────────────  │
 │  ▸ Open File…              Ctrl+O        │
 │    New File                Ctrl+N        │
 │    Search in Project…      Alt+/         │
 │    New Agent Workspace…                  │
 │    Open Sessions Dock      Alt+O         │
 │  ──────────────────────────────────────  │
 │  ● fresh                        2m       │
 │  ○ fresh-docs                   1h       │
 │  Restricted                       Ctrl+P │
 └──────────────────────────────────────────┘
```

### 4.4 First run — no workspace history on disk

With no `<data_dir>/workspaces/` entries there is nothing to resume, so the
right column is *replaced* rather than left empty, and `Open Folder…` leads
the left column instead of `Open File…`.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  File   Edit   View   Selection   Go   LSP   Explorer   Help                                                         │
 │  +                                                                                                                   │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                  fresh 0.4.10                                        ~/src/fresh                                     │
 │                  ──────────────────────────────────────────────────────────────────────                              │
 │                                                                                                                      │
 │                  START                                         FIRST TIME HERE                                       │   <- no history yet, so RESUME is replaced, not left empty
 │                                                                                                                      │
 │                  ▸ Open Folder…                                  Take the guided tour                                │   <- 'Open Folder…' leads when there is nothing to resume
 │                    Open File…                    Ctrl+O          Keyboard shortcuts             F1                   │
 │                    New File                      Ctrl+N          Settings                                            │
 │                                                                  Documentation  ↗                                    │
 │                  AGENTS                                                                                              │
 │                                                                                                                      │
 │                    New Agent Workspace…                                                                              │
 │                    Run Agent Here…                                                                                   │
 │                                                                                                                      │
 │                  ──────────────────────────────────────────────────────────────────────                              │
 │                  type to search  ·  ↑↓ move  ·  Tab column  ·  Enter open  ·  Ctrl+P palette                         │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │                                                                                                                      │
 │  Restricted   Local                                                                                 Palette: Ctrl+P  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Content

### 5.1 `START` — the editing verbs

| Row | Action | Key shown |
|---|---|---|
| Open File… | `open` | `Ctrl+O` |
| Open Folder… | `switch_project` | — |
| New File | `new` | `Ctrl+N` |
| Search in Project… | `open_live_grep` | `Alt+/` |

Four verbs. `Ctrl+B` (explorer), ``Alt+` `` (terminal in dock) and the rest are
deliberately absent: they are in the palette, on the menu bar, and on the hint
line's `Ctrl+P`. The keys in the third column are resolved from the **live
keymap**, so an Emacs or VS Code keymap, or any user rebinding, is what the
screen shows.

### 5.2 `AGENTS` — the orchestration verbs

| Row | Action | Key shown |
|---|---|---|
| New Agent Workspace… | `orchestrator_new` | — |
| Run Agent Here… | `orchestrator_run_agent` | — |
| Open Sessions Dock | `toggle_dock_focus` | `Alt+O` |

The first two are plugin commands. Per §2 they are shown **only when the
Orchestrator plugin has registered them** — resolved by command name through
the command registry, the same lookup the palette does. In a build with
plugins compiled out, or with the Orchestrator disabled, the `AGENTS` heading
and its rows are simply not drawn, and the left column is four rows long.

This is the point of the screen that matters most for a new user: "start a
task in its own worktree with an agent in it" is a first-class row on the
first screen, at the same weight as "open a file".

### 5.3 `RESUME` — one list, because a project *is* a session

Fresh's session registry is the directory set: one workspace file per
directory ever opened, carrying its label, its layout, its per-window plugin
state and its `saved_at`. Live windows this run are the subset that has been
materialized. So "recent projects" and "agent sessions" are not two lists that
need reconciling — they are one list with a badge:

```text
  ↺ Reopen last session · 5 tabs
  ● fresh              claude          2m
  ○ fresh-docs         codex           1h
    dotfiles                           2d
    nixpkgs                            6d
```

- **`↺ Reopen last session`** appears only when *this* directory has a saved
  workspace with buffers and none of them are open — the "I closed everything
  and want it back" row. Highest-value row on the screen for an existing user,
  and it is the one thing the current empty states make genuinely hard.
- **`●` live** — a window exists in this process for that root.
  **`○` dormant** — a workspace file exists, discovered at boot, not yet
  materialized. **No badge** — a directory in the recent list that was never
  opened as an orchestrator session.
- The second column is the workspace **label** as persisted (the Orchestrator
  writes an auto-name tracking the agent's terminal title, which is why it
  reads `claude` / `codex`). Core reads it from the workspace file; it does
  **not** invent an agent state it cannot observe. Live agent activity
  (`working` / `idle`) is the Orchestrator's own inference from terminal
  output, so it is annotated onto the row only when that plugin is loaded —
  see §9.
- Rows are sorted by `saved_at` descending, capped at five (three when the
  layout has folded to one column), with directories that no longer exist
  garbage-collected by the discovery pass that already does this.

Activating a row: if a window exists for that root, `set_active_window` (which
materializes it lazily — the same path the dock's dive uses). Otherwise
restore that directory's workspace into the current window. One row, one
obvious result, no dialog.

### 5.4 Type to search

Any printable character typed on the welcome screen opens **quick open**
seeded with that character. This is the "new tab page" affordance and it is
what keeps the screen minimal: the screen does not have to list everything,
because typing reaches everything — `>` for commands, `#` for buffers, `:`
for lines, bare text for files, all of which quick open already supports.

Mechanically: the key handler starts the quick-open prompt and then feeds the
character through the ordinary prompt-input path, so there is no second input
implementation to keep in sync.

---

## 6. Interaction

**Focus.** The welcome pane is focusable like any pane. A new
`KeyContext::Welcome` applies while it is the active pane, so the rest of the
keymap is untouched.

| Key | Effect |
|---|---|
| `Up` / `Down` / `k` / `j` | Move within the current column, wrapping |
| `Tab` / `Shift+Tab` / `Left` / `Right` | Switch column |
| `Enter` | Activate the focused row |
| any printable character | Quick open, seeded (§5.4) |
| `Esc` | Inert — there is nothing to dismiss |
| everything else | Falls through to global bindings — `Ctrl+P`, `F1`, the menu bar, `Alt+O` all work |

Focus starts on the first row of `START` (`Open Folder…` on first run). It is
not remembered across appearances: the screen is a standing start, and a
remembered cursor position on a list whose contents change is a trap.

**Mouse.** Hovering a row underlines it; clicking activates it. Hit testing
follows the pattern the dashboard established — registered column ranges per
row, so padding, headings and the gap between columns are not clickable and
the underline is an honest affordance. The boxes are contributed by a
`Welcome` chrome component in the `EDITOR_BASE` band, which collects only
while the active pane is a welcome placeholder, per the chrome event model.

**Accessibility and plain terminals.** Every row is readable text with its
keybinding beside it; `●`/`○` are supplementary to the label, never the only
carrier of meaning. All colours come from theme keys
(`syntax.keyword` for headings, `syntax.comment` for muted text,
`syntax.function` for actions, `ui.file_status_added_fg` / `syntax.comment`
for the live/dormant badges), so the screen follows a theme switch for free
and degrades on a monochrome terminal to plain text with a `▸` marker.

---

## 7. Configuration

One new setting replaces the existing boolean's role:

```jsonc
{
  "editor": {
    // "welcome"      — the welcome screen (default)
    // "empty_buffer" — the historical [No Name] scratch buffer
    // "blank"        — the blank pane with the one-line hint
    "empty_workspace_screen": "welcome"
  }
}
```

It governs **both** empty-workspace paths — launch with nothing to restore,
and closing the last buffer — because they are the same state and splitting
them into two settings has never been asked for.

**Migration.** `editor.auto_create_empty_buffer_on_last_buffer_close` stays,
deprecated, and keeps working: because the partial-config layer holds it as
`Option<bool>`, an *explicit* value is distinguishable from the default, and
maps to `empty_buffer` (`true`) or `blank` (`false`). If both keys are set
explicitly, the new one wins. Unset, the new default applies. The old key is
marked deprecated in the schema so the Settings UI says so.

**Interactions with neighbouring settings:**

- `file_explorer.auto_open_on_last_buffer_close` currently *focuses* the
  explorer when the last buffer closes. Under `"welcome"` the explorer still
  opens if configured, but **focus stays on the welcome screen** — two
  focusable surfaces appearing at once, with focus in the one that was not
  the point, is the bug this would otherwise ship.
- **Dashboard precedence.** If the dashboard is enabled with auto-open, it
  creates a real virtual buffer and therefore *replaces* the placeholder: the
  dashboard wins, unchanged. The two never draw at once. Documented, not
  enforced by special-casing.
- **Workspace-trust dialog.** It is a blocking modal in a higher z-band and
  renders over the welcome screen, as it does over everything. The welcome
  screen must not steal its keyboard: the existing modal dispatch order
  already guarantees this, since `KeyContext::Welcome` is a normal pane
  context.
- **`restore_previous_session`.** Untouched. A restored session has buffers,
  so no placeholder, so no welcome screen.

---

## 8. Implementation shape

Three pieces, in the split the codebase already uses (pure model, imperative
shell, painter):

1. **`app/welcome/model.rs` — pure.** `build(WelcomeInputs) -> WelcomeModel`.
   `WelcomeInputs` is a plain struct: pane width and height, the resolved
   keybinding for each candidate action, which commands are registered, the
   recent-workspace list, whether this directory has a restorable session,
   the version string and the current root. `WelcomeModel` is rows, sections,
   the fold decision, and per-row click ranges. No `Editor`, no terminal, no
   I/O — unit-testable at every breakpoint, which is where the layout bugs
   would otherwise live.
2. **`app/welcome/mod.rs` — the shell.** Owns focus index and column,
   dispatches row activation into `handle_action` / the command registry, and
   owns the recent-list cache.
3. **`view/ui/split_rendering/…/welcome.rs` — the painter.** Replaces
   `render_placeholder_hint`, which becomes the sub-minimum fallback.

**Data freshness.** The recent list is read from `<data_dir>/workspaces/`
**off the editor thread** via `spawn_off_loop_effect`, cached on `Editor`, and
refreshed when the screen appears and on window focus — never inside a frame.
The first paint uses whatever is cached (boot discovery has already read these
files, so in practice it is warm) and the list fills in on a later frame. No
frame ever blocks on disk.

**Repaint.** Driven by `viewport_changed` dimensions, focus movement, theme
change, and the recent-list refresh — the dashboard's dedupe-on-dimensions
approach, so scroll-only events cost nothing.

**i18n.** Every string is a `t!()` key under `welcome.*` added to
`locales/en.json`, per the existing convention. The wireframes above are the
English rendering; the layout is computed from measured widths, not from
assumed English lengths.

**Tests.**
- Model unit tests: each breakpoint (two-column, folded, minimum, below
  minimum), the empty-history case, the plugin-absent case, keybinding
  resolution under each shipped keymap.
- Headless scenario tests through `EditorTestApi`: close the last buffer →
  welcome appears; `Enter` on `Open File…` → the open prompt; type `a` →
  quick open seeded with `a`; open a file → welcome gone; close it → welcome
  back.
- ANSI-capture golden tests for the full-size and folded renderings.

---

## 9. Extension (phase 2)

The screen is deliberately closed in v1 — no plugin API, so there is nothing
to keep compatible while the layout is still moving. What the Orchestrator
needs in v1 it already provides through data the core reads: the persisted
workspace label.

Phase 2 adds the seam the dashboard's `registerSection` proved out, in the
smallest form that covers the two known cases:

```ts
editor.registerWelcomeRow({ slot: "agents", id, label, hint, order, handler });
editor.annotateWelcomeResume(rootPath, { text, tone });  // "working" / "idle"
```

The first lets any plugin contribute a verb; the second lets the Orchestrator
paint its live agent state onto a `RESUME` row that core drew from disk. Both
are additive and neither is on the critical path.

---

## 10. Deliberately not on the screen

- **Git status, disk, weather, PRs, clock.** That is the dashboard, it exists,
  and it is one command away. Merging the two would make the default screen a
  network client.
- **A recent *files* list.** Recent *projects* is the higher-value unit and
  the one the editor actually persists; a file list would be a second,
  weaker ranking of the same intent, and quick open already ranks files
  better.
- **Tips / "did you know".** They age badly and they are noise on a screen
  whose whole argument is minimalism.
- **A scrollable area.** If the content does not fit, the layout folds; it
  never scrolls. Scrolling implies more content than the screen should have.
- **Anything that writes.** Nothing on the screen mutates a repository. The
  agentic rows open the Orchestrator's own dialog, which is where the
  worktree/branch/agent decisions and the trust gate already live.

---

## 11. Open questions

1. **Should `Open Folder…` change the current window's working directory
   (today's `switch_project`) or dive into a window for that root (the dock's
   model)?** They differ in whether you keep one window or gain one. The
   proposal above uses `switch_project` for the verb and window-dive for
   `RESUME` rows, on the grounds that the verb is "I am moving" and the row is
   "take me back". This is the design's least settled point.
2. **Should the welcome screen appear in a *split* whose buffer was closed
   while other splits hold files?** The proposal says no — the placeholder
   only survives when it is the last buffer in the window — but the forced
   empty-placeholder path (a dock present, editor leaf kept blank) is a real
   case that needs a ruling.
3. **Version line.** `fresh 0.4.10` doubles as an update affordance if the
   update checker has news. Worth wiring, or does it start the slide into a
   dashboard?
4. **Should `type to search` be scoped to files (quick-open-files) rather than
   the full quick open?** Full quick open is more powerful and its prefixes
   are already documented; files-only is more predictable.

---

## 12. Build order

| Phase | Contents |
|---|---|
| 1 | `WelcomeModel` + painter + the `"welcome"` / `"empty_buffer"` / `"blank"` setting with migration. Static rows only — `START`, `AGENTS`, hint line. Keyboard focus and `Enter`. |
| 2 | `RESUME`: workspace-cache read off-loop, badges, `Reopen last session`, row activation via `set_active_window`. |
| 3 | Mouse: `Welcome` chrome component, hover underline, click ranges. |
| 4 | Type-to-search; first-run variant; fold breakpoints and the sub-minimum fallback. |
| 5 | Flip the default to `"welcome"`, user docs (`docs/features/`), locale keys for the shipped languages. |
| 6 | Plugin contribution API (§9), Orchestrator live-state annotation. |

Phases 1–3 are independently shippable behind the setting; the default does
not move until phase 5.
