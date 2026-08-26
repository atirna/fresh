# Welcome Screen — a ladder, not a launcher

> _Design note. Status: **PARTLY IMPLEMENTED** — `plugins/welcome_screen.ts`
> ships the ladder, the three doors, the jump keys, foldable cards, the live
> finder, the live theme picker, the real git and Orchestrator cards, and the
> startup toggle. §13 records what the build taught, including the three
> places the wireframes above are still aspirational. AI-generated from the
> source; where it disagrees with the code, the code is authoritative._

Fresh replaces the `[No Name]` scratch buffer and the blank placeholder hint
with an **interactive welcome buffer**: a scrollable document, rendered in the
editor's own idiom, that onboards three very different audiences without
overwhelming the simplest one.

The screen's whole argument is one sentence, and it is the product's argument
too: **it starts as simple as nano, and scales to mission control — only when
you ask it to.** The first viewport mentions no LSP, no git, no worktrees and
no agents. You reach those by scrolling, which is exactly the gesture that
says "show me more."

---

## 1. What this replaces

Three empty states exist today, and none is a starting point:

1. **`[No Name]` scratch buffer** (`editor.auto_create_empty_buffer_on_last_buffer_close = true`,
   the default) — a text cursor in a buffer that will never be saved.
2. **The blank pane** (same setting, `false`) — a `synthetic_placeholder`
   buffer, hidden from tabs, painting one inert centred line:
   `Ctrl+P  command palette · Ctrl+O  open file · Ctrl+E  file explorer`.
3. **The Dashboard plugin** — opt-in, off by default, and an *information*
   surface (git, disk, weather, PRs) rather than an onboarding one.

All three answer a question nobody asked. None of them tells a first-time user
that the thing they just installed also runs four coding agents across four
worktrees — and none of them reassures a nervous one that `Ctrl+S` still
saves.

---

## 2. The shape: progressive disclosure as a ladder

Three audiences, one screen, ordered by sophistication:

| Rung | Audience | What they need to see |
|---|---|---|
| **First viewport** | everyone | logo, one line, three doors, four verbs, one promise |
| **Level 1 · Just edit** | "I want to edit a file" | recent files, big-file handling, `$EDITOR` setup |
| **Level 2 · It's a project now** | "I expect a real IDE" | LSP, hunk-level git review, themes, power tools |
| **Level 3 · Run the whole shop** | "I orchestrate agents" | the Orchestrator dock, worktrees, remotes, daemon |

Three affordances keep the ladder navigable rather than merely long:

- **Three numbered path cards** on the first screen. Click one, or press
  `1` / `2` / `3`, and the buffer scrolls to that level. Nobody has to
  discover the depth by scrolling blindly.
- **A depth meter in the status bar** — `editor → IDE → orchestrator` — that
  lights the segment you are in as you scroll, and highlights the matching
  path card. It is a "you are here" for a document whose whole structure is
  depth.
- **Fold arrows in the gutter.** Every card folds to a single line. A user who
  does not care about git folds that card and it stays folded.

### Show, don't list

Every major feature is a **small live demo**, not a bullet:

| Card | What is actually live |
|---|---|
| Pick up where you left off | a real `TextInput`; typing really fuzzy-finds, `Enter` really opens |
| Language smarts | a real embedded editor view: real grammar, real hover popup, real diagnostic with its real code action |
| Review your diff | stage / unstage really run; the file counts on the left really move |
| Make it yours | the theme buttons restyle the entire editor, live, for real |
| The Orchestrator dock | the dock's own widget list; arrowing it swaps the transcript beside it |

Two things fall out of this that a bullet list can never do. Every
interaction teaches a real keybinding in passing (the finder card is where you
learn `Ctrl+P`, not a line that says "press `Ctrl+P`"). And the screen cannot
lie about the product, because it *is* the product: if the hover popup is
ugly, the welcome screen is ugly.

---

## 3. Why a buffer

The welcome screen is a **virtual buffer with a tab**, called `Welcome`. Not a
modal, not a dock panel, not a hidden placeholder. That is the most
Fresh-native answer available, and it earns a surprising amount for free:

- **It scrolls,** because buffers scroll. The status bar's scroll readout
  (`top` / `58%` / `bot`) is the reader's progress bar through the ladder.
- **It ends in `~` tildes,** because that is how every file in Fresh ends. The
  page finishes the way the editor finishes.
- **It has a gutter,** which is where the fold arrows live — the same
  affordance as folding code.
- **It is stripped from workspace serialization,** like every virtual buffer,
  so it can never come back as a stale tab in a restored session.
- **It closes like a tab,** because it is one. No bespoke dismiss gesture.

And it makes the mock honest: what the wireframes below draw is a Fresh
buffer, drawn by Fresh's own renderer, in the user's own theme.

---

## 4. The primitives this rides on

This concept looks expensive. It is not: every capability it needs already
ships. This section is the feasibility spine — the reason the build order in
§10 starts at "assemble" rather than "invent".

| Need | Existing primitive |
|---|---|
| A tabbed, read-only, plugin-owned page | `createVirtualBuffer({ name, mode, readOnly, showLineNumbers:false })` — the Dashboard pattern |
| Interactive controls **inside** that page | `mountWidgetPanel(panelId, bufferId, spec)` — "mount a declarative widget panel inside a virtual buffer". It renders the spec *into the buffer's content* via `set_virtual_buffer_content` and maps widget focus onto a real buffer cursor. `search_replace.ts` already ships a buffer-mounted panel with live text inputs. |
| Buttons, lists, trees, text inputs, toggles, dropdowns | the widget library (`button`, `list`, `tree`, `textInput`, `toggle`, `divider`, `hintBar`, …) — the same one the Orchestrator dock and the tour panel are built from |
| A **real editor view** embedded in a card | `windowEmbed({ windowId, rows })` — reserves a rectangle the host paints the live window UI into: split tree, terminals, syntax highlighting, decorations |
| Syntax-highlighted fenced code in a card | `Text` widgets with `markdown: true` render through the shared markdown engine **with the grammar registry** attached |
| Vertical scrolling, `~` filler, scroll position | the buffer's own; widget panels pin *horizontal* scroll and leave vertical alone |
| The depth meter | a status-bar `CustomToken` element plus the per-buffer `status_bar_values` map |
| Keyboard model for a cursorless page | `defineMode(name, bindings, readOnly, allowTextInput, inheritNormalBindings)` — the Dashboard's `j`/`k`/`Tab`/`Enter` idiom |
| Mouse | the `mouse_click` hook with per-row registered column ranges — the Dashboard's hit-testing pattern |
| Not a plugin dependency | the widget runtime is **deliberately not gated** behind the `plugins` cargo feature, so core can mount panels in a plugin-less build |

Two genuinely new pieces are needed, both small and both useful beyond this
screen:

1. **A `{scroll}` status-bar element** (`top` / `NN%` / `bot`). The status bar
   has no scroll readout today. Every long buffer wants one.
2. **Scroll-position → depth-meter plumbing**: the welcome buffer needs to
   know which level banner is on screen. This is a viewport-row comparison
   against known banner rows, recomputed on scroll.

---

## 5. The screen

**1 — First viewport.** The zero-anxiety zone: logo, one line, three doors, four verbs, one promise. Nothing here mentions LSP, git, worktrees or agents.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  >_  File   Edit   View   Selection   Go   LSP   Help                                                                │
 │  Welcome ×  │  Ctrl+P to open a file…                                                                                │   <- a real, closable tab — and a ghost tab that teaches Ctrl+P
 │               ███████╗██████╗ ███████╗███████╗██╗  ██╗                                                               │
 │               ██╔════╝██╔══██╗██╔════╝██╔════╝██║  ██║                                                               │
 │               █████╗  ██████╔╝█████╗  ███████╗███████║                                                               │
 │               ██╔══╝  ██╔══██╗██╔══╝  ╚════██║██╔══██║                                                               │
 │               ██║     ██║  ██║███████╗███████║██║  ██║                                                               │
 │               ╚═╝     ╚═╝  ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝                                                               │
 │                                                                                                                      │
 │               A terminal text editor and IDE.  It grows when your work does.                                         │   <- one line of positioning. no feature list, no bullets.
 │               v0.4.10  ·  single static binary  ·  open source                                                       │
 │                                                                                                                      │
 │               ── WHAT BRINGS YOU HERE? ──                                                                            │   <- three doors, sized like doors
 │                                                                                                                      │
 │               ┌────────────────────────────┐ ┌────────────────────────────┐ ┌────────────────────────────┐           │   <- 1 / 2 / 3 jump straight down; the status bar tracks which door you took
 │               │ [1] JUST EDIT TEXT         │ │ [2] CLASSIC IDE            │ │ [3] ORCHESTRATE            │           │
 │               │ Open a file & go           │ │ Code with LSP & git        │ │ Run agents in parallel     │           │
 │               │ Notes, configs, huge logs. │ │ Completions, goto & hover, │ │ One workspace per worktree │           │
 │               │ Standard keys, full mouse  │ │ hunk-level diff review,    │ │ — claude, codex, aider and │           │
 │               │ — nothing to learn first.  │ │ splits, themes, plugins.   │ │ remotes. Tour the diffs.   │           │
 │               │                            │ │                            │ │                            │           │
 │               │ jump ↓  ·  or press 1      │ │ jump ↓  ·  or press 2      │ │ jump ↓  ·  or press 3      │           │
 │               └────────────────────────────┘ └────────────────────────────┘ └────────────────────────────┘           │
 │                                                                                                                      │
 │               ▸ Open file                                      Ctrl+O                                                │
 │               ▸ Find a recent file                             Ctrl+P                                                │   <- the plain verbs, for anyone who already knows what they want
 │               ▸ New buffer                                     Ctrl+N                                                │
 │                                                                                                                      │
 │               ┌──────────────────────────────────────────────────────────────────────────────────────┐               │
 │               │ Nothing to learn first.  It works like you'd expect:  Ctrl+S saves,  Ctrl+Z undoes,  │               │   <- reassurance BEFORE capability — this is the anxiety valve
 │               │ Ctrl+F finds,  Ctrl+C/V copy-paste — and the mouse just works.  Click, drag, select. │               │
 │               └──────────────────────────────────────────────────────────────────────────────────────┘               │
 │                                       ▼ scroll — the rest is here when you need it ▼                                 │
 │  Welcome   [editor]  →  IDE  →  orchestrator   Palette: Ctrl+P               LF · UTF-8   Tokyo Night   14:32   top  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**2 — Level 1 · Just edit.** Everyday editing. The finder is a live widget, not a picture of one; the second card is folded to show that any card can be dismissed.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  >_  File   Edit   View   Selection   Go   LSP   Help                                                                │
 │  Welcome ×  │  Ctrl+P to open a file…                                                                                │
 │               ──── LEVEL 1 · JUST EDIT ──────────────────────────────────────────────────────────────────            │   <- the level banner. scrolling IS the disclosure.
 │               Open a file. Type. Save. Fresh stays out of the way.                                                   │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │   <- gutter fold arrows — the same affordance as folding code
 │               │ Pick up where you left off                                 this box is live — type in it │           │
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │  ┌ fuzzy find ────────────────────────────────────────────────────────────┐              │           │
 │               │  │ cfg█                                                                   │              │           │   <- a real TextInput widget. the fuzzy finder actually runs.
 │               │  └────────────────────────────────────────────────────────────────────────┘              │           │
 │               │                                                                                          │           │
 │               │  ▸ ./config.toml                                                   1 h ago               │           │   <- Enter opens it. the demo IS the feature.
 │               │    src/store.rs                                                  14 min ago              │           │
 │               │    deploy@prod:/etc/nginx/nginx.conf                               yesterday             │           │
 │               │                                                                                          │           │
 │               │  Fresh remembers your cursor position in every file.  Hot Exit restores                  │           │
 │               │  unsaved buffers after a crash — even unnamed scratch ones.                              │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │                                                                                                                      │
 │           ▸   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │   <- a folded card: one line until you want it
 │               │ Built for the ugly files too                                    folded — click ▸ to open │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │
 │               │ Make it your $EDITOR                                        quality-of-life from day one │           │
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │  # Use Fresh for commit messages and rebases                                             │           │   <- fenced code, highlighted by the real grammar engine
 │               │  git config --global core.editor "fresh --wait"                                          │           │
 │               │                                                                                          │           │
 │               │  # Keep a project session alive across terminal disconnects                              │           │
 │               │  fresh -a myproject                                                                      │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │  Welcome   [editor]  →  IDE  →  orchestrator   Palette: Ctrl+P               LF · UTF-8   Tokyo Night   14:32   31%  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**3 — Level 2 · It's a project now.** IDE features, each one demonstrated rather than listed. The code pane is a real embedded editor view; the git pane really stages.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  >_  File   Edit   View   Selection   Go   LSP   Help                                                                │
 │  Welcome ×  │  Ctrl+P to open a file…                                                                                │
 │               ──── LEVEL 2 · IT'S A PROJECT NOW ─────────────────────────────────────────────────────────            │
 │               Language servers, git review, themes — here the whole time, waiting.                                   │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │
 │               │ Language smarts, zero setup                                hover the dotted & wavy words │           │
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │   29 │ pub struct UserStore {                                                            │           │   <- a real editor view, embedded. real grammar, real gutter.
 │               │        ┄┄┄┄┄┄┄┄┄                                                                         │           │
 │               │        ┌────────────────────────────────────────────────┐                                │           │   <- the editor's own hover popup, not a drawing of one
 │               │   30 │ │ struct UserStore                               │                                │           │
 │               │   31 │ │   users: HashMap<u64, User>                    │                                │           │
 │               │   42 │ │ Owns all users by id.                          │ -> impl Iterator {             │           │
 │               │   43 │ │ F12 goto definition · Shift+F12 references     │ .is_actve)                     │           │
 │               │        └────────────────────────────────────────────────┘ ~~~~~~~~                       │           │
 │               │   44 │     }                                                                             │           │
 │               │  ⚠ unknown field `is_actve` — a field with a similar name exists: `is_active`            │           │   <- a real diagnostic, carrying its real code action
 │               │    Code action:  Ctrl+.  →  rename to is_active                                          │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │
 │               │ Review your diff before it reviews you                 the stage buttons work — try them │           │
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │  STAGED (1)               │ @@ src/store.rs · 42–44   staged ✓ [unstage] [discard]       │           │   <- these buttons run. the counts on the left really move.
 │               │   M src/store.rs          │      pub fn active_users(&self) …                            │           │
 │               │                           │ -        self.users.values()                                 │           │
 │               │  UNSTAGED (1)             │ +        self.users.values().filter(|u| u.is_active)         │           │
 │               │   M src/main.rs           │                                                              │           │   <- hunk-level review, exactly as you'd use it on a live repo
 │               │                           │ @@ src/store.rs · 61–62   unstaged [stage] [discard]         │           │
 │               │  UNTRACKED (1)            │      impl UserStore {                                        │           │
 │               │   ? notes/todo.md         │ +    pub fn len(&self) -> usize { self.users.len() }         │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │  Welcome   editor  →  [IDE]  →  orchestrator   Palette: Ctrl+P               LF · UTF-8   Tokyo Night   14:32   58%  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**4 — Level 3 · Run the whole shop.** Agent orchestration. Clicking or arrowing the dock swaps the transcript beside it — because it is the dock, embedded.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  >_  File   Edit   View   Selection   Go   LSP   Help                                                                │
 │  Welcome ×  │  Ctrl+P to open a file…                                                                                │
 │               ──── LEVEL 3 · RUN THE WHOLE SHOP ─────────────────────────────────────────────────────────            │
 │               One workspace per git worktree. An agent in each. Hop with an arrow key.                               │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │
 │               │ The Orchestrator dock                                 click a workspace, or ↑ ↓ the list │           │   <- the last rung. this card is why the ladder exists.
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │  WORKSPACES              │ branch fix/login-bug · +124 −18 · PR #241 · CI running        │           │   <- the dock's own widget list — the same one Alt+O gives you
 │               │  ▸ fix/login-bug      ●  │ ┌────────────────────────────────────────────────────┐        │           │   <- arrowing the list swaps the transcript. it IS the live dock.
 │               │    feat/i18n-ko       ◐  │ │ worktree ~/w/fix-login-bug                         │        │           │
 │               │    chore/deps         ✓  │ │ claude ▸ Reproduced the race in session refresh.   │        │           │
 │               │    deploy@prod        ⇅  │ │ claude ▸ Patched token rotation; 3 files changed.  │        │           │
 │               │                          │ │ claude ▸ Running the auth test suite…              │        │           │   <- a real terminal, embedded. that cursor really blinks.
 │               │  + add workspace         │ │ tests: 41 passed, 2 running █                      │        │           │
 │               │    (cuts a worktree      │ └────────────────────────────────────────────────────┘        │           │
 │               │     and a branch)        │                                                               │           │
 │               │                          │ ● working   ◐ waiting on you   ✓ done   ⇅ remote              │           │   <- the glyph legend, once, where the glyphs are
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │                                                                                                                      │
 │           ▾   ┌──────────────────────────────────────────────────────────────────────────────────────────┐           │
 │               │ Your other machines are workspaces too                           SSH + detachable daemon │           │
 │               ├──────────────────────────────────────────────────────────────────────────────────────────┤           │
 │               │                                                                                          │           │
 │               │  # Edit nginx config on prod — saves transfer only the patch                             │           │
 │               │  fresh deploy@prod:/etc/nginx/nginx.conf                                                 │           │   <- your other machines, on the same ladder
 │               │                                                                                          │           │
 │               │  # Open a file in an already-running daemon                                              │           │
 │               │  fresh --cmd daemon open-file myproject src/main.rs:42                                   │           │
 │               │                                                                                          │           │
 │               └──────────────────────────────────────────────────────────────────────────────────────────┘           │
 │                                                                                                                      │
 │                                                                                                                      │
 │  Welcome   editor  →  IDE  →  [orchestrator]   Palette: Ctrl+P               LF · UTF-8   Tokyo Night   14:32   84%  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**5 — The end of the buffer.** Links, the startup toggle, and the closing line. Below it, the buffer's own `~` filler: the page ends the way every file in Fresh ends.

```text
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │  >_  File   Edit   View   Selection   Go   LSP   Help                                                                │
 │  Welcome ×  │  Ctrl+P to open a file…                                                                                │
 │                                                                                                                      │
 │               ──────────────────────────────────────────────────────────────────────────────────────────             │
 │                                                                                                                      │
 │               That's the whole ladder.  Most days you'll live on rung one — the rest keeps up                        │   <- the last line grants permission to stay on rung one
 │               when you climb.                                                                                        │
 │                                                                                                                      │
 │               Docs      Keybindings      Plugin registry      GitHub      Discord                                    │
 │                                                                                                                      │
 │               [x] Show this screen on startup                                                                        │   <- the screen knows when to get out of the way
 │                                                                                                                      │
 │           ~                                                                                                          │   <- the buffer's own end-of-file tildes. it really is a buffer.
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │           ~                                                                                                          │
 │  Welcome   editor  →  IDE  →  [orchestrator]   Palette: Ctrl+P               LF · UTF-8   Tokyo Night   14:32   bot  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**6 — A narrow pane (62×34).** One column, cards stacked, the depth meter abbreviated. Below ~46 columns the screen falls back to the plain hint line.

```text
 ┌────────────────────────────────────────────────────────────┐
 │  >_ File Edit View …                                       │
 │  Welcome ×                                                 │
 │     ███████╗██████╗ ███████╗███████╗██╗  ██╗               │
 │     ██╔════╝██╔══██╗██╔════╝██╔════╝██║  ██║               │
 │     █████╗  ██████╔╝█████╗  ███████╗███████║               │
 │     ██╔══╝  ██╔══██╗██╔══╝  ╚════██║██╔══██║               │
 │     ██║     ██║  ██║███████╗███████║██║  ██║               │
 │     ╚═╝     ╚═╝  ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝               │
 │                                                            │
 │     It grows when your work does.                          │
 │                                                            │
 │     ┌──────────────────────────────────────────────────┐   │   <- cards stack. the ascii art is the last thing dropped.
 │     │ [1] JUST EDIT TEXT                               │   │
 │     │ Open a file & go                                 │   │
 │     │ Notes, configs, huge logs.                       │   │
 │     │ jump ↓  ·  press 1                               │   │
 │     └──────────────────────────────────────────────────┘   │
 │                                                            │
 │     ┌──────────────────────────────────────────────────┐   │
 │     │ [2] CLASSIC IDE                                  │   │
 │     │ Code with LSP & git                              │   │
 │     │ Completions, git, themes.                        │   │
 │     │ jump ↓  ·  press 2                               │   │
 │     └──────────────────────────────────────────────────┘   │
 │                                                            │
 │     ┌──────────────────────────────────────────────────┐   │
 │     │ [3] ORCHESTRATE                                  │   │
 │     │ Agents in parallel                               │   │
 │     │ Worktrees, remotes, dock.                        │   │
 │     │ jump ↓  ·  press 3                               │   │
 │     └──────────────────────────────────────────────────┘   │
 │                                                            │
 │  [editor] → IDE → orch                         14:32  top  │
 └────────────────────────────────────────────────────────────┘
```
---

## 6. Copy

Three rules, in priority order.

**Reassure first, impress later.** The first viewport's job is to lower the
pulse of someone who typed `fresh notes.txt` and got an IDE. Hence the
reassurance card — `Ctrl+S` saves, `Ctrl+Z` undoes, the mouse works — sitting
*above the fold*, and hence the deliberate absence of the words LSP, git,
worktree and agent from that first screen.

**Editor voice: plain, confident, no exclamation marks.** "Open a file. Type.
Save. Fresh stays out of the way." Not "Welcome to Fresh! 🎉". The product is
a terminal editor; the copy sounds like one.

**The last line grants permission to stop climbing.** "That's the whole
ladder. Most days you'll live on rung one — the rest keeps up when you climb."
A screen that shows off three levels of power must end by saying that using
one of them is the normal outcome, or it reads as a demand.

A note on the level names. `JUST EDIT` / `IT'S A PROJECT NOW` / `RUN THE WHOLE
SHOP` are deliberately in the user's voice about their own situation, not in
the product's voice about its feature tiers. "Level 2: IDE Features" would be
a manual; "It's a project now" is a thing that happens to you.

---

## 7. Interaction

| Input | Effect |
|---|---|
| `1` / `2` / `3` | scroll to that level; the path card and depth meter update |
| `0` | back to the top |
| scroll / `PgUp` / `PgDn` / mouse wheel | ordinary buffer scrolling; the depth meter follows |
| `Tab` / `Shift+Tab` | move focus between interactive widgets |
| `Enter` / `Space` | activate the focused widget |
| click on a fold arrow, or `za` on its row | fold / unfold that card |
| typing, while the finder is focused | it is a real text input; it really searches |
| `Ctrl+P`, `F1`, `Alt+O`, the menu bar | all work — this is a normal pane, not a modal |
| `Ctrl+W` / the tab's `×` | close it, like any tab |

**Focus.** A `welcome` mode (via `defineMode`) with `inheritNormalBindings:
false`: on a cursorless page, every key is either bound here or intentionally
inert. When a `TextInput` is focused the widget runtime flips `show_cursors`
on and puts a real cursor in it — the same path the search/replace panel uses.

**Reveal on scroll.** Cards below the fold fade in as they enter the viewport,
so the first screen stays calm and the ladder feels like descent rather than a
wall of content. Fresh already has a frame-buffer animation layer
(`editor.animations`) and already gates on it; when animations are off, or the
terminal is slow, cards simply appear. This is decoration and it is the first
thing to go.

**Mouse.** Everything clickable is a widget with a registered hit region;
hovering a row underlines it. Rows that are prose are not clickable, so the
underline stays an honest affordance.

---

## 8. Lifecycle and configuration

```jsonc
{
  "editor": {
    // "welcome"      — the welcome buffer (default)
    // "empty_buffer" — the historical [No Name] scratch buffer
    // "blank"        — the blank pane with the one-line hint
    "empty_workspace_screen": "welcome"
  }
}
```

One setting, governing **both** empty-workspace paths: launch with nothing to
restore, and closing the last buffer. They are the same state.

**Migration.** `editor.auto_create_empty_buffer_on_last_buffer_close` stays,
deprecated, and keeps working. The partial-config layer holds it as
`Option<bool>`, so an *explicit* value is distinguishable from the default and
maps to `empty_buffer` (`true`) or `blank` (`false`). If both are set
explicitly, the new key wins.

**When it opens.**

- At startup, when the session restore produced no buffers.
- After the last buffer is closed.
- On demand: `Welcome` in the palette, or **Help ▸ Welcome**.

**When it gets out of the way.** This is the ruling that decides whether the
screen is a good citizen or a nuisance:

> A welcome buffer that was **auto-opened and never interacted with** —
> never scrolled, never focused, no widget touched — closes itself when a
> real file opens. One the user **engaged with** stays open until they close
> it.

An ambient screen you ignored was ambient; a document you started reading is
yours. This also means `fresh src/main.rs` never leaves a Welcome tab behind,
without needing the Dashboard's blunt "close on any file open".

**Closing it never reopens it.** Closing the last tab when the welcome buffer
was the thing you just closed leaves the plain placeholder for the rest of the
session. There is no loop, and no way to get trapped.

**The startup toggle.** The footer's `[x] Show this screen on startup` writes
`empty_workspace_screen` — `"welcome"` when checked, `"blank"` when not. The
screen carries its own off switch, at the bottom, where you arrive after
deciding you have seen enough.

**Neighbouring surfaces.**

- **Dashboard.** If it is enabled with auto-open it creates its own virtual
  buffer and simply wins; the two never draw at once. Documented, not
  special-cased.
- **`file_explorer.auto_open_on_last_buffer_close`.** The explorer still opens
  if configured, but focus stays in the welcome buffer.
- **Workspace-trust dialog.** A blocking modal in a higher z-band; it renders
  over the welcome buffer and owns the keyboard, exactly as it does over
  everything else.
- **`restore_previous_session`.** Untouched. A restored session has buffers,
  so no welcome screen.

---

## 9. Quality floor

- **Responsive.** Two breakpoints. Below ~76 columns the path cards stack into
  one column, the depth meter abbreviates, and card bodies reflow (frame 6).
  Below ~46 columns, or ~14 rows, the screen falls back to today's single
  centred hint line — a welcome screen that wraps is worse than none.
- **Reduced motion.** Reveal-on-scroll and the scroll-hint bob are gated on
  the animations setting; with it off, everything is simply present.
- **Keyboard-complete.** Every interactive element is reachable by `Tab` and
  activatable by `Enter`. Nothing is mouse-only.
- **Colour is never the only signal.** `●` / `◐` / `✓` / `⇅` in the dock carry
  a text legend on the same card; diagnostics carry their text, not just a
  squiggle. All colours come from theme keys, so the screen follows a theme
  switch — including the one made from its own theme card.
- **i18n.** Every string is a `t!()` key under `welcome.*`. Layout is computed
  from measured widths, never from assumed English lengths — the wireframes
  are the English rendering, not the layout contract.
- **Never blocks a frame.** The recent-file list and the workspace list are
  read off the editor thread (`spawn_off_loop_effect`) and cached; the first
  paint uses what boot discovery already loaded, and late arrivals repaint.

---

## 10. Cost, and what is honestly hard

The concept is buildable on shipping primitives, but three things deserve
naming rather than hand-waving:

1. **The embedded live views are the expensive part.** `windowEmbed` paints a
   real window into a reserved rectangle. For the LSP card that means standing
   up a real buffer with a real language server to demo against. The
   mitigation is to ship the card with a **static, syntax-highlighted sample**
   first (markdown `Text` with grammars — cheap, no LSP) and upgrade to a live
   embed only if the demo proves worth the machinery. The wireframe draws the
   destination; phase 3 draws the affordable version.
2. **Folds are widget-level, not buffer-level.** Fresh's real folding
   (`view/folding.rs`) works on buffer syntax, which a widget panel does not
   have. The gutter arrows are drawn by the panel and collapse the card by
   re-rendering its spec. Visually and behaviourally identical; mechanically
   not the same code, and `za` needs an explicit binding in the `welcome` mode
   rather than falling through.
3. **A long widget spec re-renders as a whole.** `updateWidgetPanel` replaces
   the spec; `widgetMutate` is the targeted fast path. A ladder this long
   should use `widgetMutate` for fold toggles and finder keystrokes, or every
   keypress in the finder re-transmits the entire page.

---

## 11. Open questions

1. **Does the welcome buffer get a keybinding of its own?** Everything else on
   the first screen teaches one. `Alt+H`? Or is the palette entry enough?
2. **Should Level 3 appear at all on a machine with no git repo and no
   worktrees?** Showing "one workspace per worktree" to someone editing
   `~/notes.txt` is honest about the product but useless to them. Options:
   always show it (the ladder is the pitch), or fold Level 3 by default
   outside a repo. Leaning: always show, always expanded — the whole point is
   that the ceiling is visible from the floor.
3. **First run versus every run.** Should the screen remember it has been seen
   and open *collapsed to the first viewport* thereafter, with levels folded?
   That trades the "I forgot Fresh could do that" rediscovery for a shorter
   page.
4. **Does the theme card write the theme, or preview it?** Writing config from
   a welcome screen is a real mutation. Proposal: it applies live like the
   theme picker does, and persists only on an explicit "keep this one".

---

## 12. Build order

| Phase | Contents |
|---|---|
| 1 | The buffer: `Welcome` virtual buffer, `welcome` mode, `empty_workspace_screen` setting with migration, open/close/get-out-of-the-way lifecycle. Static content, no widgets. |
| 2 | The ladder: first viewport, three path cards, `1`/`2`/`3` jumps, level banners, `{scroll}` status element, depth meter. |
| 3 | Cards as widget panels: fold arrows, the live finder, static syntax-highlighted code and diff samples, the footer toggle. |
| 4 | Live demos: theme picker, git staging, the embedded Orchestrator dock. |
| 5 | Polish: reveal-on-scroll, narrow breakpoints, the sub-minimum fallback, locale keys. |
| 6 | Flip the default to `"welcome"`; user docs under `docs/features/`. |

Phases 1–2 are shippable on their own and already beat all three of today's
empty states. The default does not move until phase 6.

---

## 13. What the build taught

`plugins/welcome_screen.ts` implements this design. It is a TypeScript plugin
— **no host change was needed**, which was the bet §4 made and it held. The
page is a virtual buffer with a `WidgetPanel` mounted into it; every control
is a widget from `plugins/lib/widgets.ts`; the demos read real data through
`spawnProcess`, `getAllThemes` / `applyTheme`, and
`getPluginApi("orchestrator").listWorkspaces()`.

Ten things the wireframes did not know:

1. **A panel repaint replaces the whole buffer, so it parks the viewport at
   line 0.** Fine for a panel that fits its pane; wrong for a document you
   scroll. Every repaint now captures `getViewport().topLine` and restores it
   afterwards. Folding only removes rows *below* a card's header, so the
   restore is exact rather than approximate.

2. **`viewport_changed` fires on height changes too — including the one the
   command palette causes by taking a row.** Repainting there cancelled the
   prompt the user had just opened (`Search cancelled.` in the status bar,
   every time). The listener now dedupes on **width only**: width is what the
   layout depends on, and every list pins its own `visibleRows`, so height
   changes have nothing to recompute.

3. **`scrollBufferToLine` is a *reveal*, not a scroll-to-top** — it
   deliberately leaves `viewport_height / 3` of context above its target.
   Right for "show me this match", wrong for a level jump and for the repaint
   restore above. A local `scrollTopTo` compensates rather than asking for a
   second host verb.

4. **`move_page_up` / `move_page_down` page the *cursor*.** On a cursorless
   page whose cursor is wherever the widget runtime last parked it, that jumps
   somewhere the reader never was. Page keys compute the new top line from the
   viewport instead.

5. **A mode with `allowTextInput` owns the keyboard**: the host blocks unbound
   Ctrl-/Alt-modified keys so a focused text field can never be hijacked by
   Open or Save. That is the right default, and it means the accelerators this
   page promises have to be named — `FORWARDED` lists them and each one
   forwards to the real action, so a rebound key keeps working. Notably they
   do *not* mark the page engaged: reaching past it for the palette is the
   ambient case.

6. **Tab moves widget focus, but the host only scrolls the pane for a focused
   *text* widget.** A focused button further down a long document was
   invisible. The `focus` event now reveals the focused widget's card, using
   card-header rows read back from the painted buffer — the same read-back
   `resolveLevelLines` already did for the jump keys. A keyed read-only widget
   also joins the Tab cycle, so the markdown sample is deliberately keyless.

7. **`getAllThemes()` answers with the registry object, not a list.** Its keys
   are the theme names.

8. **Closing the page reopened it.** `closeBuffer` fires `buffer_closed`,
   which — with no other buffer left — is exactly the condition the ambient
   open path watches for. Escape, the tab's `×` and `Ctrl+W` were all
   unclosable. A `dismissed` flag now records that the *reader* closed it and
   the ambient paths stay quiet for the session; the `Welcome` command clears
   it. Stepping aside for a file deliberately does not set it — that is the
   page being polite, not the reader dismissing it. §8's "closing it never
   reopens it" was a design rule; it needed code.

9. **A `List` inside a `labeledSection` cannot reach the section's right
   border.** Its items are emitted at their natural width, so every finder
   result ended in a `…` clip marker exactly where the frame should be — and
   padding cannot fix it: one column short leaves the border undrawn, one
   column over draws the marker in its cell. `raw` rows, which the host pads
   to the enclosing section, do reach it. The results are rows now and the
   plugin owns the selection, which `finderIndex` already was.

10. **Enter on a single-line `Text` widget is advance-focus**, so a finder
    that merely forwarded the key moved on instead of opening the pick.

### Still aspirational

- **The LSP card** shows a real syntax-highlighted Rust sample (a markdown
  `Text` widget carrying the grammar registry — the highlighting is genuinely
  the editor's own), but no live hover popup or diagnostic. That needs a real
  buffer with a real language server behind a `windowEmbed`, which is §10's
  first cost item.
- **The git card** reports the real branch and the real changed-file list, but
  the stage / unstage buttons of the mock are not there; it links to the
  branch-diff review instead.
- **The Orchestrator card** lists the real workspaces with their agent state
  and focuses one on click, but does not embed a live terminal transcript.

### Verified by hand

Driven in tmux at 160×44, 74×40 and 52×30, against both a two-file scratch
repo and this repository (where the workspace-trust modal correctly renders
over the page and owns the keyboard, and the finder fuzzy-matches the whole
tracked tree — `wlcscr` finds `welcome_screen.ts`): the ladder
and jump keys, `/` to the finder, live fuzzy-find over `git ls-files` and
`Enter` to open a hit, folding by click and by `Enter`, live theme switching
by click (status bar confirms), the startup toggle flipping and **persisting
across a restart** (the screen then stays away, and the `Welcome` command
brings it back), `Ctrl+P` opening the palette from the page, the `[No Name]`
seed being retired on open, `fresh notes.txt` never leaving a Welcome tab
behind, and both responsive breakpoints.
