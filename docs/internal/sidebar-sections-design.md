# The sidebar as a column of sections

> _Design note. Status: **PLANNED**. Nothing here ships today. It answers
> sinelaw/fresh#3045 ("Make the file explorer sidebar vertically splittable"):
> whether the request duplicates something Fresh already has, what the feature
> would look like, and where it belongs relative to the `fresh-ui` migration._

---

## 1. What is being asked for

The request: make the file explorer sidebar vertically splittable — stacked
sections with a draggable divider, so the lower section can host a plugin
panel, another tree, or anything else. The stated reason is that the sidebar is
single-occupancy today, so anything wanting to live alongside the explorer has
to replace it entirely.

Two claims are bundled there, and they need separating because only one of them
is true.

**True:** the sidebar column is single-occupancy. It is a chrome region carved
out of the frame's width, it holds exactly one thing — the file tree — and it
has no notion of a second occupant. Nothing can share it.

**Not true:** that a panel wanting to live "alongside the explorer" must replace
it. Fresh has two other places a panel can live, both of which coexist with the
explorer perfectly well (§2). What it does not have is a way to put a panel
*inside the sidebar column*, stacked under the tree, which is what the request
describes and what the rest of this note designs.

The distinction matters for triage. This is not "plugin panels have nowhere to
go". It is "the sidebar is the one region of the frame that cannot be
subdivided, and it is the region users most expect to subdivide."

---

## 2. Overlap audit

### 2.1 Fresh already has four ways to place a panel

| Mechanism | Where it lives | Resize | Who uses it |
|---|---|---|---|
| **File explorer sidebar** | A chrome column carved from the frame width, left or right, outside the split tree | Drag its inner border; width persists per workspace as percent or absolute columns | The file tree, and only the file tree |
| **Utility Dock** | A *tagged singleton leaf inside the split tree* — at most one leaf carries the role | Split separators; ratio persists with the split tree | Diagnostics, search/replace results, terminals, quickfix, code tour; plugins target it by split role |
| **Editor-global left dock** | A full-height column pinned left of *all* chrome, including the menu and status bars | Drag its right border; width persists across toggles | The Orchestrator's workspace switcher; any plugin panel that re-anchors itself there |
| **Floating widget panel** | Centered modal overlay, or a popup anchored at a screen cell | Sized by percentage, or by content | Plugin panels by default; context menus |

So the sidebar is not the only home for auxiliary UI — it is the only home that
is *adjacent to the file tree*. The Utility Dock gets a panel a resizable
region with a draggable divider today; it just cannot put that region in the
sidebar column.

**Conclusion: #3045 is not a duplicate.** No existing mechanism subdivides the
sidebar. But it is not a greenfield feature either — it is a fifth placement
for content whose *type* already exists, which is the constraint that should
shape the design (§4.1).

### 2.2 Fresh already implements this drag three times

A draggable divider between two resizable regions exists in three independent
implementations:

- **Split separators** — hit-tested from cached separator rects, dragged by a
  per-window `dragging_separator` flag, applied by adjusting a container ratio.
- **The explorer's width border** — hit-tested as the rightmost column of the
  explorer rect, dragged by `dragging_file_explorer` plus a start-width
  snapshot, applied by mutating the explorer width and reflowing.
- **The left dock's width border** — hit-tested in the dock's chrome
  component, dragged by an editor-level `dock_resizing` flag.

All three were exercised live with synthetic SGR mouse events: the explorer
border resizes the column, the dock separator resizes the dock, and neither
knows about the other. Each carries its own hit-test rect, its own drag-start
snapshot, its own flag on the mouse state, and its own clamping rules.
**Adding a fourth on the current chrome stack is the thing to avoid**, and it is the main reason the
sequencing in §6 is what it is: `fresh-ui` collapses all three into one
mechanism — a gesture node that captures the pointer — and the migration
already plans to delete the machinery behind all three.

### 2.3 Correlated issues

| Issue | State | Relationship |
|---|---|---|
| **#950** — sidebar with file outline for markdown TOC, typst, etc. | Open | A *consumer*. Blocked on exactly this: an outline wants to be beside the tree, not instead of it. |
| **#1791** — side panel for markdown table-of-contents and code outline, navigable and auto-syncing | Open | The same consumer, specified in more detail. #3045 is its missing infrastructure. |
| **#1468** — move sidebar to the right side | Closed | Established the left/right side setting. The section layout must mirror correctly (§5.6). |
| **#1213** — absolute (fixed) width for the file explorer | Closed | Established that sidebar extent is percent *or* absolute columns, user-chosen, and that a drag must preserve whichever variant the user picked. Section heights should follow the same rule. |
| **#2282** — terminal tabs in the Utility Dock, vertically | Open | Sibling ask, different region: "subdivide the dock". Same underlying want — regions of the frame should compose. |

The honest summary for the issue thread: **#3045 partially overlaps three
shipped mechanisms and is the blocker for two open feature requests.** It is
worth doing, and it is worth doing once, generically.

---

## 3. What driving it actually shows

Everything below was traced from a live session (84x26, the repo's own tree),
not from reading the render code. Three observations changed the design.

### 3.1 Today

```text
  File   Edit   View   Selection   Go   LSP   Help                                  
┌ File Explorer ───────×─┐ main.rs ×   +                                          □×
│▼ demo                  │▾ 1 │ fn main() -> Result<()> {                           
│  ▼ crates              │  2 │     let editor = Editor::new()?;                    
│    > fresh-core        │  3 │     editor.run()                                    
│    > fresh-ui          │  4 │ }                                                   
│  > docs                │  5 │                                                     
│    Cargo.toml          │▾ 6 │ impl Editor {                                       
│    lib.rs              │▾ 7 │     fn run(&mut self) -> Result<()> {               
│    main.rs          ●  │  8 │         loop { self.tick()?; }                      
│    README.md           │  9 │     }                                               
│                        │ 10 │ }                                                   
│                        │ 11 │                                                     
└────────────────────────┘~                                                         
  Restricted  Local  Ln 1, Col 1       LF  ASCII  Rust   LSP (off)   Palette: Ctrl+P
```

The sidebar is one bordered block filling the column. Its right border column
is the width-drag handle — a real drag confirms it, and it clamps nothing on
the way out. Two details a section header must not collide with: the selection
marker `▌` painted in the first content column, and the plugin decoration slot
(`●`) painted at the right of each row, just inside the border.

### 3.2 The sidebar is the only region that doesn't compose

Open a terminal in the Utility Dock and the frame stacks — *except* under the
sidebar:

```text
  File   Edit   View   Selection   Go   LSP   Explorer   Help                       
┌ File Explorer ───────×─┐ main.rs ×   +                                          □×
│▼ demo                  │▾ 1 │ fn main() -> Result<()> {                           
│  ▼ crates              │  2 │     let editor = Editor::new()?;                    
│    > fresh-core        │  3 │     editor.run()                                    
│    > fresh-ui          │  4 │ }                                                   
│  > docs                │  5 │                                                     
│    Cargo.toml          │▾ 6 │ impl Editor {                                       
│    lib.rs              │──────────────────────────────────────────────────────────
│    main.rs          ●  │ bash — /demo ×   +                                     □×
│    README.md           │root@vm:/demo#                                            
│                        │                                                          
│                        │                                                          
└────────────────────────┘                                                          
  Restricted  Local  Ln 1, Col 1       LF  ASCII  Rust   LSP (off)   Palette: Ctrl+P
```

The dock's separator is a plain rule that begins where the sidebar's border
ends. The editor area splits; the sidebar doesn't. That picture is the request,
stated as a diagram: **every other region of the frame already subdivides, and
the sidebar is the one that can't.**

It also settles a design question. The split grid's divider carries **no
title** — identity lives in the tab strip below it, with its own controls
(`□×`) at the right. Mirroring that in the sidebar would cost two rows per
boundary (rule + tab strip) out of a column typically 24-30 columns wide, where
a tab strip cannot fit meaningful labels anyway. So the sidebar takes the other
option: **the divider row is the section header**, in the shape the explorer's
own title bar already uses.

### 3.3 Under vertical pressure, today's sidebar just empties

Shrinking the terminal to four rows leaves the explorer as a top and bottom
border with no content between them, still occupying its full width. There is
no minimum, and no collapse. Whatever this feature does under pressure is a new
decision, not a rule to inherit (§3.7).

One related papercut, observed rather than inferred: at a narrow width the
title's keybinding suffix and its close button collide, rendering
`┌ File Explorer (Ctrl+×)┐` — the `×` overwrites the binding text instead of
the fill. A section header adds a chevron to that same row, so it inherits the
pressure; whatever fixes one should fix both.

### 3.4 Two sections

```text
  File   Edit   View   Selection   Go   LSP   Explorer   Help                       
┌ ▼ File Explorer ─────×─┐ main.rs ×   +                                          □×
│▼ demo                  │▾ 1 │ fn main() -> Result<()> {                           
│  ▼ crates              │  2 │     let editor = Editor::new()?;                    
│    > fresh-core        │  3 │     editor.run()                                    
│    > fresh-ui          │  4 │ }                                                   
│  > docs                │  5 │                                                     
│    Cargo.toml          │▾ 6 │ impl Editor {                                       
├ ▼ Outline ───────────×─┤▾ 7 │     fn run(&mut self) -> Result<()> {               
│▼ fn main               │  8 │         loop { self.tick()?; }                      
│    let editor          │  9 │     }                                               
│  > fn run              │ 10 │ }                                                   
│> impl Editor           │ 11 │                                                     
└────────────────────────┘~                                                         
  Restricted  Local  Ln 1, Col 1       LF  ASCII  Rust   LSP (off)   Palette: Ctrl+P
```

**The key layout decision: adjacent sections share one border row.** Section
one's bottom border *is* section two's top border, and that shared row carries
section two's title, in the explorer's existing title shape - lead, fill,
close. Two separately bordered blocks would spend two rows of chrome per
boundary; this spends one. The shared row is the drag handle, the collapse
toggle and the section header at once.

### 3.5 After dragging the divider up

```text
┌ ▼ File Explorer ─────×─┐
│▼ demo                  │
│  ▼ crates              │
│    > fresh-core        │
├ ▼ Outline ───────────×─┤
│▼ fn main               │
│    let editor          │
│  > fn run              │
│> impl Editor           │
│                        │
│                        │
│                        │
└────────────────────────┘
```

The section above the divider takes an explicit height; the last section always
flexes to absorb the remainder. Both neighbours clamp at one content row, so a
section can't be dragged out of existence - collapsing is the reversible way to
reclaim its space.

### 3.6 Three sections, one collapsed

```text
┌ ▼ File Explorer ─────×─┐
│▼ demo                  │
│  ▼ crates              │
│    > fresh-core        │
│    > fresh-ui          │
│  > docs                │
├ > Outline ───────────×─┤
├ ▼ Git Changes ───────×─┤
│M  main.rs              │
│A  docs/design.md       │
│?  scratch.txt          │
│                        │
└────────────────────────┘
```

A collapsed section keeps its header row and gives up its body; the chevron
toggles it. Modelling sections as a list from the start means N > 2 needs no
re-modelling, and this is already the natural UI for it.

### 3.7 Squeezed

```text
┌ ▼ File Explorer ─────×─┐
│▼ demo                  │
│  ▼ crates              │
│    > fresh-core        │
├ > Outline ───────────×─┤
├ > Git Changes ───────×─┤
└────────────────────────┘
```

When the column is shorter than the sum of the sections' minimums, the sidebar
collapses **from the bottom up** until what remains fits, and restores on the
way back out. This has to be chosen explicitly. The migration's frame work
already found that `fresh-ui` and ratatui starve different rows when a band is
over-subscribed, and recorded that a caller who cares must pick its own
starvation order rather than inherit either engine's. The sidebar cares: the
top section is the tree.

---

## 4. The model

### 4.1 A section hosts content that already exists

The design rule that keeps this from becoming a fifth placement mechanism:

> A sidebar section hosts either the built-in file tree, or a plugin widget
> panel — the *same* mounted-panel content the left dock already hosts.

So the sidebar column becomes a second **host** for a content type Fresh
already renders, focuses, and reconciles, rather than a new content type. In
state terms the sidebar stops being "the explorer's width" and becomes:

```text
SidebarColumn
  cols:     ExplorerWidth          // unchanged: percent or absolute
  side:     FileExplorerSide       // unchanged: left or right
  sections: Vec<SidebarSection>

SidebarSection
  kind:      Explorer | Panel(PanelKey)
  extent:    SectionExtent          // Rows(n) | Pct(n); the last section flexes
  collapsed: bool
```

`Vec` from the start even though the first cut ships two sections: N > 2 then
needs no re-modelling, and §3.6 is already the natural UI for it.

**The alternative considered and rejected:** mirror the split grid exactly —
a plain separator rule plus a per-section tab strip, so one section could hold
several panels as tabs (which is also what #2282 wants for the dock). It loses
on width, not on principle. §3.2 shows the dock's tab strip carrying a
truncated path and two controls across 58 columns; the same strip in a 24-column
sidebar has room for neither labels nor controls, and it costs a second row at
every boundary. Tabs within a section stay open as a later addition if a section
ever hosts enough panels to need them.

### 4.2 Sizing and the drag

Section extent mirrors the width model established by #1213: a section is sized
in **rows or percent, whichever the user chose**, and a drag preserves the
variant rather than silently flipping it. The last section is always flexible
and absorbs the remainder, so the column is always exactly filled and there is
no accumulated rounding drift.

The drag itself is one gesture on the shared border row: press captures the
pointer, move recomputes the extent of the section *above* the divider from the
pointer row, release ends the capture. Both neighbours clamp at one content
row. Because the extent recomputes from the absolute pointer row rather than
accumulating deltas, the divider cannot drift away from the cursor over a long
drag — a failure mode the delta-accumulating explorer-width drag is only
immune to because it snapshots the start width.

### 4.3 Focus

Today the sidebar is a single keyboard context: focus is either "in the file
explorer" or not. With sections, focus has to move *between* sections, and each
section's content keeps its own key handling — the tree keeps its incremental
search and its navigation, a plugin panel keeps the widget panel's routing and
its focused/blurred distinction.

Post-migration this is a focus scope with sections in tree order and nothing
else: no new keyboard context, no new entry in a precedence table. Pre-
migration it is a new context plus edits to the focus-cycling ladder — which is
§6's argument in miniature.

One new bindable action: focus the next sidebar section. Unbound by default.

### 4.4 Persistence

Section layout is *session state*, so it is app state, not element state — the
migration's own rule is that anything the workspace file serializes must stay
app state, because elements are disposed on unmount and do not survive a
restart. The workspace's file-explorer state gains a `sections` list, defaulted
so that an existing workspace file with no `sections` key restores as exactly
one Explorer section flexing to fill the column: byte-identical to today.

### 4.5 Configuration and defaults

**The default configuration is one section.** Out of the box the sidebar looks
and behaves exactly as it does now — §3.1, not §3.4. The chevron and the shared
border row only appear once a second section exists. This is not a soft
preference: a feature that changes the default sidebar for every user who never
asked for a second panel has mis-scoped itself.

Config gains an optional list of sections to open with, in the existing file
explorer table. Nothing else changes; width, side, hidden-file and gitignore
settings all keep their meaning.

### 4.6 Plugin API

Additive, and shaped like what exists. A mounted floating panel is already
re-anchored by a control operation carrying an op name and a numeric argument
(`dock` with a width, `center`, `focus`, `blur`, `fullscreen`). This adds one
op: **`sidebar`**, with the argument as the section's requested rows. A plugin
that today calls `dock` to become the left column calls `sidebar` to become a
sidebar section instead, and everything downstream — the spec, the reconcile,
the widget commands, the mutation fast path — is unchanged.

Panels already carry a composite `(plugin, id)` identity, so the identity a
persisted section needs already exists and already survives a restart, and a
section whose plugin is not loaded restores as a header row with a "panel
unavailable" body rather than vanishing.

---

## 5. What this touches

| Area | Change |
|---|---|
| Frame layout | The sidebar region stops being one leaf and becomes a column of section leaves plus divider rows |
| Explorer rendering | Must render into a sub-rect and stop assuming it owns the column: top border conditional on being first, bottom border conditional on being last |
| Hit-testing | One new target class (the divider/header row) with three behaviours: drag, collapse toggle, close |
| Row chrome | The header row must not collide with the selection marker in the first content column or the decoration slot at the right (§3.1), and it inherits the narrow-width title collision (§3.3) |
| Focus | Sections join the focus order; the explorer's keyboard context becomes per-section |
| Persistence | `sections` added to the workspace's file-explorer state, defaulted |
| Config | Optional default section list |
| Plugin API | One new placement op |
| Tests | Frame-rect coverage for N sections and the squeeze order; restore coverage for the empty-`sections` default; a drag test that the divider tracks the pointer |

### 5.6 Right-side sidebars

Everything above is side-agnostic: sections stack vertically, and the column's
side only decides which frame edge it is carved from. The one detail to get
right is that the *width*-drag border is the column's inner edge — the right
border on a left sidebar, the left border on a right sidebar — while the
*height*-drag rows are interior and identical on both sides.

---

## 6. Where this belongs: the `fresh-ui` migration

The UI migration moves the whole editor UI onto the `fresh-ui` retained tree,
outside-in. Its current state and its plan both bear directly on when #3045
should be built.

**What has landed.** The frame's geometry now comes from a single `fresh-ui`
description rather than from ratatui layout calls: one host region per area,
with the sidebar carved as a fixed-width child of a row. Every region is still
painted by today's painters, so nothing changed on screen — which was the
point. Input is offered to the shell tree ahead of the legacy walk. The status
bar has since moved across as the first native description.

**What has not.** The stage that turns the dock column, the file explorer and
plugin panels into native descriptions has not started. The stage that
decomposes the split grid — and with it deletes the chrome component registry,
the pointer-grab drag machinery, and the keyboard-context precedence table —
is last.

### 6.1 The two ways to build it

| | On the current chrome stack | After the explorer migrates |
|---|---|---|
| Divider drag | A **fourth** hit-test rect, drag-start snapshot and mouse-state flag (§2.2) | One gesture node that captures the pointer — the mechanism already in the library |
| Section heights | Manual rect arithmetic in the sidebar's layout function, plus a starvation rule written by hand | Column children sized in cells with the last one flexing; the starvation rule is the one decision left to make |
| Focus between sections | A new keyboard context, plus edits to the focus-cycling ladder and the precedence table | Sections in a focus scope, in tree order |
| Collapse | New state plus a branch in the layout function | The same, minus the layout branch |
| Lifespan of that code | Deleted by the split-grid stage | Permanent |

Everything in the left column is work the migration is explicitly scheduled to
delete. That is the whole argument.

### 6.2 But not *inside* a migration wave

The migration's acceptance test is that **cell output stays byte-identical per
wave**, and its own rule is that deliberate visual changes are made separately,
*after* a wave, so a regression stays distinguishable from an intended change.
A splittable sidebar is a deliberate visual change. Landing it inside the wave
that migrates the explorer would destroy the property that makes the wave
reviewable.

### 6.3 Recommended sequencing

1. **Overlay stage** — layers, modality, dismissal, focus, as planned. This is
   the migration's own go/no-go and #3045 does not affect it.
2. **Explorer stage** — the sidebar becomes a native `fresh-ui` description.
   Byte-identical: still one section, still one bordered block, no chevron.
   This is the migration's work, unchanged by this note.
3. **#3045** — the first *feature* built on the migrated sidebar, as a
   follow-on commit on the migration branch or a branch off it. The diff is
   the section model (§4.1), a column of children with one gesture node per
   divider, and the persistence default.

The one thing worth doing before then is **deciding §4.1 now**: whether a
sidebar section hosts arbitrary new content or the panel content that already
exists. That decision constrains the explorer stage — it is the difference
between migrating the explorer as "the thing in the sidebar" and migrating it
as "one section kind among several" — and it costs nothing to settle in
advance.

### 6.4 If it is wanted before then

There is a cheaper answer that needs no new mechanism, and it should be offered
in the issue thread rather than left implicit: a plugin panel can already take
a Utility Dock leaf, or the left dock column, and both coexist with the
explorer and both are already divider-resizable. Neither is *in* the sidebar,
so neither matches the sketch — but for the concrete consumers (#950, #1791, an
outline beside the tree) a dock leaf is a working answer available today, and
the sidebar version is a placement upgrade rather than the feature itself.

---

## 7. Open questions

1. **Section content type** (§4.1) — panel-hosted, or a new content kind?
   Blocks the explorer migration stage; settle first.
2. **Squeeze order** (§3.7) — bottom-up collapse is proposed. The alternative,
   proportional shrink to minimums, keeps every section visible but makes all
   of them useless at once; bottom-up keeps the top section usable, and the top
   section is the tree.
3. **Does the explorer stay pinned first?** Proposed yes for the first cut —
   reordering sections is a separate feature and a separate set of drag
   affordances.
4. **Per-window or per-workspace?** The explorer's width and visibility are
   per-window today, persisted per workspace. Sections should follow, but a
   window-switch that changes the section list is a visible jump worth
   confirming against the workspace-restore suites.
