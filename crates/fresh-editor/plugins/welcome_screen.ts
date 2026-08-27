/// <reference path="./lib/fresh.d.ts" />
import {
  button,
  col,
  divider,
  flexSpacer,
  labeledSection,
  raw,
  row,
  spacer,
  type StyledSegment,
  styledRow,
  text,
  textInput,
  textInputChar,
  toggle,
  type WidgetSpec,
  WidgetPanel,
  key as widgetKey,
} from "./lib/widgets.ts";

const editor = getEditor();

// ═════════════════════════════════════════════════════════════════════
//   WELCOME SCREEN
//
//   The empty-workspace surface: a scrollable buffer that onboards
//   three audiences on one page without overwhelming the simplest.
//   Design note: docs/internal/welcome-screen-design.md.
//
//   Structure is a ladder. The first viewport is a zero-anxiety zone
//   (wordmark, one line, three numbered doors, four verbs, one
//   reassurance) and mentions no LSP, git, worktree or agent.
//   Scrolling descends through three bannered levels ordered by
//   sophistication; `1` / `2` / `3` jump straight to one.
//
//   Everything here is built from the existing plugin surface — a
//   virtual buffer with a mounted widget panel (the same pairing
//   `search_replace.ts` uses), the widget library's own controls, and
//   the orchestrator's exported API. No new host mechanism.
//
//   The demos are real rather than illustrated: the finder runs over
//   `git ls-files`, the theme buttons call `applyTheme`, the git card
//   reads `git status`, and the workspace rows come from
//   `getPluginApi("orchestrator").listWorkspaces()`.
// ═════════════════════════════════════════════════════════════════════

const C = {
  art: "syntax.function",
  title: "syntax.keyword",
  accent: "syntax.function",
  /** All prose. The page is a document, so its default ink is the
   *  editor's default ink — not `syntax.comment`, which is green on the
   *  stock dark theme, nor `syntax.string`, which is dark red on light. */
  body: "editor.fg",
  /** Literal quotable things only: paths, commands, branch names. */
  value: "syntax.string",
  /** Genuinely recessive in every shipped theme, unlike `syntax.comment`. */
  muted: "editor.line_number_fg",
  /** Bullets, markers, separators — structure, not content. */
  gutter: "editor.line_number_fg",
  key: "ui.help_key_fg",
  /** `ui.popup_border_fg` is built for a floating edge over dimmed
   *  content, so it is deliberately loud; across ten full-width frames
   *  it made the chrome the brightest ink on the page. This is the
   *  editor's own "rule between regions" colour. */
  frame: "ui.split_separator_fg",
  ok: "ui.file_status_added_fg",
  err: "diagnostic.error_fg",
};

/** Per-porcelain-code colour, matching the file explorer and git gutter.
 *  Themes that leave these unset fall back to the `diagnostic.*` family,
 *  so they stay theme-derived rather than a hardcoded grey. */
function statusFg(xy: string): string {
  const c = xy.trim();
  if (c === "??") return "ui.file_status_untracked_fg";
  if (c.includes("U") || c === "AA" || c === "DD") return "ui.file_status_conflicted_fg";
  if (c.includes("R")) return "ui.file_status_renamed_fg";
  if (c.includes("D")) return "ui.file_status_deleted_fg";
  if (c.includes("A")) return "ui.file_status_added_fg";
  return "ui.file_status_modified_fg";
}

// ── Model ────────────────────────────────────────────────────────────

type FinderItem = { path: string };

type Workspace = {
  name: string;
  branch: string;
  agentState: string;
  kind: string;
  active: boolean;
  windowId: number;
  dirty: number;
};

let bufferId: number | null = null;
let panel: WidgetPanel | null = null;
let opening = false;
/** True once the user has scrolled, clicked or typed here. An
 *  auto-opened screen nobody touched steps aside when a real file
 *  opens; one the reader engaged with is theirs to close. */
let engaged = false;
/** Set when the reader closes the page — by Escape, by the tab's `×`,
 *  by `Ctrl+W`. The ambient open paths then stay quiet for the rest of
 *  the session: closing this screen must never be undone by the very
 *  `buffer_closed` event the close itself produced, and "I closed it"
 *  is an answer, not a question to ask again. The `Welcome` command
 *  clears it — asking for the page back is the other answer. */
let dismissed = false;

const folded = new Set<string>();

let finderQuery = "";
let finderCursor = 0;
let repoFiles: string[] | null = null;
let repoFilesLoading = false;
let finderHits: FinderItem[] = [];
let finderIndex = 0;

let themeNames: string[] = [];
let activeTheme = "";

let gitDirty: string[] = [];
let gitBranch = "";
let gitProbed = false;

let workspaces: Workspace[] = [];

/** Best-effort proxy for "which widget has focus", mirrored from
 *  `widget_event`. The welcome buffer is a document first, so Up/Down
 *  scroll it — except while the finder owns focus, where they walk its
 *  results. The widget runtime does not report focus directly; this is
 *  the same proxy `search_replace.ts` keeps for its history walk. */
let lastFocusedWidget = "";

function finderFocused(): boolean {
  return lastFocusedWidget === "finderField";
}

/** Buffer line of each level banner, resolved after each render by
 *  searching the painted buffer text for the banner marker. The widget
 *  runtime owns layout, so the rows are read back rather than
 *  predicted. */
const levelLines: Record<string, number> = {};

/** Buffer line of each card's header, resolved the same way as the
 *  level banners. Tab moves widget focus but the host only scrolls the
 *  pane for a focused *text* widget — a focused button on a long
 *  document can land off-screen. Knowing each card's row lets the
 *  focus event bring its card into view. */
const cardLines: Record<string, number> = {};


/** Card title text, by card id — the string searched for above, and
 *  the one rendered in the card header, so the two cannot drift. */
const CARD_TITLE: Record<string, string> = {
  finder: "Pick up where you left off",
  ugly: "Built for the ugly files too",
  editorvar: "Make it your $EDITOR",
  lsp: "Language smarts, zero setup",
  git: "Review your diff before it reviews you",
  themes: "Make it yours",
  power: "Power tools when your hands get fast",
  orch: "The Orchestrator dock",
  remote: "Your other machines are workspaces too",
};

/** Which card a widget lives in, so focusing it can reveal that card.
 *  Keys are matched by prefix first, then exactly. */
function cardForWidget(k: string): string | null {
  if (k.startsWith("fold:")) return k.slice(5);
  if (k.startsWith("theme:")) return "themes";
  if (k.startsWith("ws:")) return "orch";
  if (k === "finderField") return "finder";
  if (k === "act_review" || k === "act_gitlog") return "git";
  if (k.startsWith("act_ws_")) return "orch";
  return null;
}

const LEVEL_MARK: Record<string, string> = {
  "1": "LEVEL 1 · JUST EDIT",
  "2": "LEVEL 2 · IT'S A PROJECT NOW",
  "3": "LEVEL 3 · RUN THE WHOLE SHOP",
};

// ── Small helpers ────────────────────────────────────────────────────

function line(segments: StyledSegment[]): WidgetSpec {
  return raw([styledRow(segments)]);
}

function plain(t: string, fg?: string): WidgetSpec {
  return line([{ text: t, style: fg ? { fg } : undefined }]);
}

/** A bullet whose marker is structure and whose text is content — the
 *  two were the same colour, which threw away the marker as a scanning
 *  aid and painted the sentence as a comment. */
function bullet(t: string): WidgetSpec {
  return line([
    { text: "  · ", style: { fg: C.gutter } },
    { text: t, style: { fg: C.body } },
  ]);
}

function blank(): WidgetSpec {
  return raw([styledRow([{ text: "" }])]);
}

function accel(action: string): string {
  return editor.getKeybindingLabel(action, "normal") ??
    editor.getKeybindingLabel(action, "global") ?? "";
}

/** A keybinding hint, or nothing at all when the action is unbound —
 *  never a stale hardcoded key. */
function accelSpec(action: string): WidgetSpec[] {
  const label = accel(action);
  if (!label) return [];
  return [flexSpacer(), line([{ text: label, style: { fg: C.key } }]), spacer(2)];
}

/** One clickable verb: `▸ Label` at the prose edge, its key at the
 *  right of the measure. The gap is computed rather than flexed — a
 *  `flexSpacer` here stretched `Ctrl+O` 125 columns away from the thing
 *  it belonged to. */
function verb(key: string, label: string, action: string): WidgetSpec {
  const shown = `▸ ${label}`;
  const acc = accel(action);
  const gap = Math.max(2, measure() - 2 - shown.length - acc.length);
  const parts: WidgetSpec[] = [
    spacer(2),
    button(shown, { key, bare: true, hoverStyle: { fg: C.accent } }),
  ];
  if (acc) {
    parts.push(spacer(gap), line([{ text: acc, style: { fg: C.key } }]));
  }
  return row(...parts);
}

/** A section heading: fold arrow at the rail, title, then a leader rule
 *  running out to the hint. A rule is the typographic answer to a wide
 *  gap between a label and its value — and unlike a flex spacer it can
 *  be computed exactly, so the hint never drifts a hundred columns from
 *  the thing it describes. Narrow: the hint goes first, then the rule. */
function heading(id: string, title: string, hint: string): WidgetSpec {
  const open = !folded.has(id);
  const M = measure();
  const segs: StyledSegment[] = [{ text: title, style: { fg: C.accent, bold: true } }];
  const gap = M - 2 - title.length - hint.length - 2;
  if (gap >= 4) {
    segs.push({ text: " " + "─".repeat(gap) + " ", style: { fg: C.frame } });
    segs.push({ text: hint, style: { fg: C.muted } });
  } else {
    const g = M - 2 - title.length - 1;
    if (g >= 2) segs.push({ text: " " + "─".repeat(g), style: { fg: C.frame } });
  }
  return row(
    button(open ? "▾" : "▸", {
      key: `fold:${id}`,
      bare: true,
      hoverStyle: { fg: C.accent },
    }),
    spacer(1),
    line(segs),
  );
}

/** A foldable section.
 *
 *  `framed` draws the box. It is reserved for the sections holding real,
 *  touchable data — the finder, git, themes, the dock — so the frame
 *  means "your data is in here and you can touch it" rather than
 *  "section", which it said ten times in the loudest colour on the page.
 *  Reading material gets the heading and nothing else. */
function card(
  id: string,
  title: string,
  hint: string,
  body: () => WidgetSpec[],
  framed = false,
): WidgetSpec {
  const head = heading(id, title, hint);
  if (folded.has(id)) return head;
  // No internal divider: the heading above the box already separates.
  if (framed) return col(head, toMeasure(col(...body())));
  return col(head, ...body());
}

function banner(level: string, sub: string): WidgetSpec {
  const mark = LEVEL_MARK[level];
  // Computed, not a hardcoded 40: the rule used to stop at column 64 on
  // a wide terminal and overflow the pane on a narrow one. Heavy stroke
  // so the top of the hierarchy is also the strongest horizontal.
  const tail = Math.max(3, measure() - 5 - mark.length - 1);
  return col(
    blank(),
    blank(),
    line([
      { text: "━━━━ ", style: { fg: C.frame } },
      { text: mark, style: { fg: C.title, bold: true } },
      { text: " " + "━".repeat(tail), style: { fg: C.frame } },
    ]),
    line([{ text: "  " + sub, style: { fg: C.body } }]),
    blank(),
  );
}

// ── The wordmark ─────────────────────────────────────────────────────

const ART = [
  "███████╗██████╗ ███████╗███████╗██╗  ██╗",
  "██╔════╝██╔══██╗██╔════╝██╔════╝██║  ██║",
  "█████╗  ██████╔╝█████╗  ███████╗███████║",
  "██╔══╝  ██╔══██╗██╔══╝  ╚════██║██╔══██║",
  "██║     ██║  ██║███████╗███████║██║  ██║",
  "╚═╝     ╚═╝  ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝",
];

/** ANSI-Shadow is a two-material face: `█` block faces and a `╔╗╚╝║═`
 *  bevel. Painting both in one colour flattened the mark into a slab;
 *  recessing the bevel gives it the depth the glyph set was drawn for. */
function artLine(l: string): WidgetSpec {
  const segs: StyledSegment[] = [{ text: "  " }];
  let i = 0;
  while (i < l.length) {
    const face = l[i] === "█";
    let j = i;
    while (j < l.length && (l[j] === "█") === face) j++;
    segs.push({ text: l.slice(i, j), style: { fg: face ? C.art : C.frame } });
    i = j;
  }
  return line(segs);
}

function hero(): WidgetSpec[] {
  const wide = viewportWidth() >= 60;
  const art = wide
    ? ART.map(artLine)
    : [line([{ text: "  fresh", style: { fg: C.art, bold: true } }])];
  return [
    blank(),
    ...art,
    blank(),
    plain(
      viewportWidth() >= 70
        ? "  A terminal text editor and IDE.  It grows when your work does."
        : "  It grows when your work does.",
      C.body,
    ),
    // The chips line carries the off switch on its right. A control for
    // "I don't want this screen" belongs where someone who doesn't want
    // the screen will actually look — the first thing they see — not at
    // the bottom of a page they were never going to scroll.
    ...startupRow(),
  ];
}

/** The chips line, with the startup toggle right-aligned on it when
 *  there is room. Below the two-column fold the toggle drops to its own
 *  line rather than being pushed off the edge. */
function startupRow(): WidgetSpec[] {
  const chips = line([
    { text: "  single static binary", style: { fg: C.muted } },
    { text: "  ·  ", style: { fg: C.gutter } },
    { text: "zero configuration", style: { fg: C.muted } },
    { text: "  ·  ", style: { fg: C.gutter } },
    { text: "open source", style: { fg: C.muted } },
  ]);
  const sw = toggle(showOnStartup(), "Show this screen on startup", {
    key: "startupToggle",
  });
  if (viewportWidth() >= 96) {
    return [row(chips, flexSpacer(), sw, spacer(2))];
  }
  return [chips, blank(), row(spacer(2), sw)];
}

// ── The three doors ──────────────────────────────────────────────────

type Door = { n: string; head: string; sub: string; body: string[] };

const DOORS: Door[] = [
  {
    n: "1",
    head: "[1] JUST EDIT TEXT",
    sub: "Open a file & go",
    body: ["Notes, configs, huge", "logs. Standard keys and", "full mouse. Nothing to", "learn first."],
  },
  {
    n: "2",
    head: "[2] CLASSIC IDE",
    sub: "Code with LSP & git",
    body: ["Completions, goto and", "hover, hunk-level diff", "review, splits, themes,", "plugins."],
  },
  {
    n: "3",
    head: "[3] ORCHESTRATE",
    sub: "Run agents in parallel",
    body: ["One worktree per task.", "claude, codex, aider and", "remotes. Tour the diffs."],
  },
];

/** Bodies are padded to a common height: `labeledSection` sizes to its
 *  own child, so an uneven row of doors closes its boxes at different
 *  rows and reads as broken rather than as three peers. */
const DOOR_BODY_ROWS = Math.max(...DOORS.map((d) => d.body.length));

function doorCard(d: Door): WidgetSpec {
  return labeledSection({
    label: d.head,
    widthPct: pct(measure() / 3),
    key: `door:${d.n}`,
    child: col(
      plain(d.sub, C.body),
      blank(),
      ...d.body.map((b) => plain(b, C.muted)),
      ...Array.from({ length: DOOR_BODY_ROWS - d.body.length }, () => blank()),
      blank(),
      row(
        button("jump ↓", {
          key: `jump:${d.n}`,
          bare: true,
          hoverStyle: { fg: C.accent },
        }),
        flexSpacer(),
        line([{ text: d.n, style: { fg: C.key } }]),
      ),
    ),
  });
}

function doors(): WidgetSpec[] {
  const wide = viewportWidth() >= 96;
  const cards = DOORS.map(doorCard);
  return [
    blank(),
    line([{ text: "  WHAT BRINGS YOU HERE?", style: { fg: C.muted } }]),
    blank(),
    wide ? row(...cards) : col(...cards),
  ];
}

// ── Level 1 ──────────────────────────────────────────────────────────

function fuzzy(query: string, s: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  const t = s.toLowerCase();
  let qi = 0;
  for (let i = 0; i < t.length && qi < q.length; i++) {
    if (t[i] === q[qi]) qi++;
  }
  return qi === q.length;
}

function recomputeHits(): void {
  const files = repoFiles ?? [];
  const out: FinderItem[] = [];
  for (const f of files) {
    if (fuzzy(finderQuery, f)) {
      out.push({ path: f });
      if (out.length >= 200) break;
    }
  }
  finderHits = out;
  if (finderIndex >= finderHits.length) finderIndex = 0;
}

function finderCard(): WidgetSpec {
  return card("finder", "Pick up where you left off", "live — type in it", () => {
    const rows: WidgetSpec[] = [
      blank(),
      textInput(finderQuery, {
        key: "finderField",
        cursorByte: finderCursor,
        label: " find",
        fullWidth: true,
      }),
      blank(),
    ];
    if (repoFiles === null) {
      rows.push(plain(repoFilesLoading ? "  scanning…" : "  not a git repo — Ctrl+P finds files anywhere", C.muted));
    } else if (finderHits.length === 0) {
      rows.push(plain("  no match", C.muted));
    } else {
      // Rendered as rows rather than a List widget: a List's items are
      // emitted at their natural width and the enclosing section's
      // right border cannot be reached from inside one, so every row
      // ended in a `…` clip marker where the frame should be. `raw`
      // rows are padded to the section by the host, so the card stays a
      // card. Selection is ours to track either way — `finderIndex` was
      // already the model.
      for (let i = 0; i < Math.min(finderHits.length, 6); i++) {
        const h = finderHits[i];
        const on = i === finderIndex;
        rows.push(
          line([
            { text: on ? "  ▸ " : "    ", style: { fg: C.accent } },
            { text: h.path, style: { fg: on ? C.value : C.muted } },
          ]),
        );
      }
      if (finderHits.length > 6) {
        rows.push(plain(`    … and ${finderHits.length - 6} more`, C.muted));
      }
    }
    rows.push(blank());
    rows.push(
      plain("  Fresh remembers your cursor position in every file. Hot Exit restores", C.body),
    );
    rows.push(plain("  unsaved buffers after a crash — even unnamed scratch ones.", C.body));
    rows.push(blank());
    return rows;
  }, true);
}

function level1(): WidgetSpec[] {
  return [
    banner("1", "Open a file. Type. Save. Fresh stays out of the way."),
    finderCard(),
    blank(),
    card("ugly", "Built for the ugly files too", "big files, odd encodings", () => [
      blank(),
      bullet("Multi-GB files open without blocking the UI — logs, dumps, CSVs."),
      bullet("Instant startup; text appears as you type. Small memory footprint."),
      bullet("Encodings beyond UTF-8: UTF-16, GBK, Shift-JIS, Latin-1 and more."),
      bullet("Project-wide search & replace with regex — even across unsaved buffers."),
      blank(),
    ]),
    blank(),
    card("editorvar", "Make it your $EDITOR", "shell setup", () => [
      blank(),
      plain("  # Use Fresh for commit messages and rebases", C.muted),
      plain("  git config --global core.editor \"fresh --wait\"", C.value),
      blank(),
      plain("  # Keep a project session alive across terminal disconnects", C.muted),
      plain("  fresh -a myproject", C.value),
      blank(),
    ]),
  ];
}

// ── Level 2 ──────────────────────────────────────────────────────────

const SAMPLE = [
  "```rust",
  "  pub struct UserStore {",
  "      users: HashMap<u64, User>,",
  "  }",
  "",
  "  impl UserStore {",
  "      pub fn active_users(&self) -> impl Iterator<Item = &User> {",
  "          self.users.values().filter(|u| u.is_active)",
  "      }",
  "  }",
  "```",
].join("\n");

function level2(): WidgetSpec[] {
  return [
    banner("2", "Language servers, git review, themes — here the whole time, waiting."),
    card("lsp", "Language smarts, zero setup", "real syntax highlighting", () => [
      blank(),
      line([{ text: "  src/store.rs", style: { fg: C.value } }]),
      blank(),
      text({
        value: SAMPLE,
        rows: 9,
        markdown: true,
        readOnly: true,
        fieldWidth: measure() - 2,
        // Deliberately keyless: a keyed widget joins the Tab cycle, and
        // a read-only sample is something to look at, not a stop on the
        // way to the next control.
      }),
      blank(),
      bullet("Open a file and the language server starts itself. Hover, goto,"),
      line([{ text: "    references, rename, code actions and diagnostics, with no setup.", style: { fg: C.body } }]),
      bullet("Configs shipped for Python, TypeScript, Rust, Go, Java, C/C++ and more."),
      bullet("Run multiple servers per language with merged completions."),
      blank(),
    ]),
    blank(),
    gitCard(),
    blank(),
    themeCard(),
    blank(),
    card("power", "Power tools when your hands get fast", "optional, all of it", () => [
      blank(),
      bullet("Multi-cursor and block selection, keyboard macros, sort lines."),
      bullet("Command palette with prefix routing: > commands · # buffers · : lines."),
      bullet("Vi mode with operators, motions and text objects — if that's your thing."),
      bullet("TypeScript plugins, sandboxed in QuickJS. No node_modules on disk."),
      bullet("Tabs, split panes, integrated terminal, markdown preview."),
      blank(),
    ]),
  ];
}

function gitCard(): WidgetSpec {
  return card("git", "Review your diff before it reviews you", "your working tree", () => {
    const rows: WidgetSpec[] = [blank()];
    if (!gitProbed) {
      rows.push(plain("  reading git status…", C.muted));
    } else if (!gitBranch && gitDirty.length === 0) {
      rows.push(plain("  not a git repo — open one and this card fills in.", C.muted));
    } else {
      rows.push(
        line([
          { text: "  on ", style: { fg: C.muted } },
          { text: gitBranch || "(detached)", style: { fg: C.value } },
          { text: "   ", style: {} },
          {
            text: gitDirty.length === 0 ? "working tree clean" : `${gitDirty.length} changed`,
            style: { fg: gitDirty.length === 0 ? C.ok : C.body },
          },
        ]),
      );
      rows.push(blank());
      for (const f of gitDirty.slice(0, 6)) {
        rows.push(
          line([
            { text: "   " + f.slice(0, 2), style: { fg: statusFg(f.slice(0, 2)) } },
            { text: " " + f.slice(3), style: { fg: C.body } },
          ]),
        );
      }
      if (gitDirty.length > 6) {
        rows.push(plain(`   … and ${gitDirty.length - 6} more`, C.muted));
      }
    }
    rows.push(blank());
    rows.push(
      row(
        spacer(2),
        button("Review the branch diff", { key: "act_review", hoverStyle: { fg: C.accent } }),
        spacer(2),
        button("Git log", { key: "act_gitlog", hoverStyle: { fg: C.accent } }),
      ),
    );
    rows.push(blank());
    rows.push(plain("  Hunk-level stage / unstage / discard. Side-by-side diff, review", C.body));
    rows.push(plain("  notes, git gutter, git grep.", C.body));
    rows.push(blank());
    return rows;
  }, true);
}

function themeCard(): WidgetSpec {
  return card("themes", "Make it yours", "restyles the editor, live", () => {
    const buttons: WidgetSpec[] = [spacer(2)];
    for (const name of themeNames.slice(0, 6)) {
      buttons.push(
        button(name === activeTheme ? `● ${name}` : `  ${name}`, {
          key: `theme:${name}`,
          bare: true,
          hoverStyle: { fg: C.accent },
        }),
        spacer(2),
      );
    }
    return [
      blank(),
      row(...buttons),
      blank(),
      plain("  Live theme editor with \"Inspect Theme at Cursor\". Configurable status", C.body),
      plain("  bar. UI translated to 日本語, 한국어, 中文, Tiếng Việt and more.", C.body),
      blank(),
    ];
  }, true);
}

// ── Level 3 ──────────────────────────────────────────────────────────

/** Idle is a healthy state. It used to be painted `syntax.constant`
 *  amber, which told the reader something was wrong. */
function stateGlyph(w: Workspace): { text: string; fg: string } {
  if (w.kind === "discovered") return { text: "○", fg: C.muted };
  if (w.agentState === "working") return { text: "●", fg: C.ok };
  return { text: "◐", fg: C.muted };
}

function orchestratorCard(): WidgetSpec {
  return card("orch", "The Orchestrator dock", "your workspaces", () => {
    const rows: WidgetSpec[] = [blank()];
    if (workspaces.length === 0) {
      rows.push(plain("  No workspaces yet. Cut one and an agent starts inside it.", C.muted));
    } else {
      for (const w of workspaces.slice(0, 6)) {
        const g = stateGlyph(w);
        rows.push(
          row(
            spacer(2),
            line([{ text: g.text, style: { fg: g.fg } }]),
            spacer(1),
            button(w.name, {
              key: `ws:${w.windowId}`,
              bare: true,
              hoverStyle: { fg: C.accent },
            }),
            flexSpacer(),
            line([{ text: w.branch, style: { fg: C.muted } }]),
            spacer(2),
          ),
        );
      }
      rows.push(blank());
      rows.push(
        line([
          { text: "  ● working   ", style: { fg: C.ok } },
          { text: "◐ idle   ", style: { fg: C.muted } },
          { text: "○ discovered worktree", style: { fg: C.muted } },
        ]),
      );
    }
    rows.push(blank());
    // The first two are the Orchestrator's own handlers. Offer them only
    // when that plugin is actually loaded — a button whose action no
    // plugin defines fails silently in the log, which is worse than an
    // absent button. `Open the dock` is a built-in action and always
    // holds.
    const orchLoaded = editor.getPluginApi("orchestrator") !== null;
    const actions: WidgetSpec[] = [spacer(2)];
    if (orchLoaded) {
      actions.push(
        button("New workspace…", { key: "act_ws_new", hoverStyle: { fg: C.accent } }),
        spacer(2),
        button("Run agent here…", { key: "act_ws_agent", hoverStyle: { fg: C.accent } }),
        spacer(2),
      );
    }
    actions.push(button("Open the dock", { key: "act_ws_dock", hoverStyle: { fg: C.accent } }));
    rows.push(row(...actions));
    rows.push(blank());
    rows.push(plain("  One workspace per git worktree, each with its own terminals and", C.body));
    rows.push(plain("  agent. Sessions resume after a restart. Leave the rest running.", C.body));
    rows.push(blank());
    return rows;
  }, true);
}

function level3(): WidgetSpec[] {
  return [
    banner("3", "One workspace per git worktree. An agent in each. Hop with an arrow key."),
    orchestratorCard(),
    blank(),
    card("remote", "Your other machines are workspaces too", "SSH + detachable daemon", () => [
      blank(),
      plain("  # Edit nginx config on prod — saves transfer only the patch", C.muted),
      plain("  fresh deploy@prod:/etc/nginx/nginx.conf", C.value),
      blank(),
      plain("  # Open a file in an already-running daemon", C.muted),
      plain("  fresh --cmd daemon open-file myproject src/main.rs:42", C.value),
      blank(),
    ]),
  ];
}

// ── Footer ───────────────────────────────────────────────────────────

function footer(): WidgetSpec[] {
  return [
    blank(),
    divider({ style: { fg: C.frame } }),
    blank(),
    plain("  That's the whole ladder. Most days you'll live on rung one — the rest", C.value),
    plain("  keeps up when you climb.", C.value),
    blank(),
  ];
}

// ── Assembly ─────────────────────────────────────────────────────────

/** The page's text column. Long enough for the longest hand-wrapped
 *  line plus air, capped so a very wide terminal doesn't stretch a
 *  paragraph across the room. Without it every card was a 147-column
 *  box around 70 columns of text, and nothing could look composed. */
const MEASURE = 88;

function measure(): number {
  // Less two: a rule computed to exactly the viewport width wraps, and a
  // wrapped rule is a broken one. `raw` rows flow through at their own
  // width, so the pane is the only backstop.
  return Math.min(Math.max(20, viewportWidth() - 2), MEASURE);
}

/** `widthPct` is an integer percent, so this wobbles a column on
 *  resize — close enough for a text column, and it keeps the host as
 *  the one that owns layout. */
function pct(cols: number): number {
  return Math.max(1, Math.min(100, Math.round((cols * 100) / viewportWidth())));
}

/** Constrain a block to the measure. Only `labeledSection` reads
 *  `widthPct`, and an inline `spacer` beside a block child would indent
 *  only the first row, so this is the one available route. */
function toMeasure(child: WidgetSpec): WidgetSpec {
  return row(labeledSection({ child, widthPct: pct(measure()) }));
}

function viewportWidth(): number {
  const vp = editor.getViewport();
  return vp && vp.width > 0 ? vp.width : 100;
}

function buildSpec(): WidgetSpec {
  return col(
    ...hero(),
    ...doors(),
    blank(),
    verb("act_open", "Open file", "open"),
    verb("act_recent", "Find a recent file", "quick_open"),
    verb("act_new", "New buffer", "new"),
    blank(),
    // It teaches keybindings, so the keys should look like keys — the
    // verbs two lines above already paint theirs in `ui.help_key_fg`.
    // And no box: a frame is an alert shape, which is the wrong shape
    // for the one message on the page whose job is to lower a pulse.
    line([
      { text: "  Nothing to learn first. It works like you'd expect:  ", style: { fg: C.body } },
      { text: "Ctrl+S", style: { fg: C.key } },
      { text: " saves,", style: { fg: C.body } },
    ]),
    line([
      { text: "  " },
      { text: "Ctrl+Z", style: { fg: C.key } },
      { text: " undoes, ", style: { fg: C.body } },
      { text: "Ctrl+F", style: { fg: C.key } },
      { text: " finds, ", style: { fg: C.body } },
      { text: "Ctrl+C/V", style: { fg: C.key } },
      { text: " copy-paste — and the mouse just works.", style: { fg: C.body } },
    ]),
    plain("  Click, drag, scroll, select.", C.body),
    blank(),
    plain("  ▼ scroll — the rest is here when you need it", C.muted),
    ...level1(),
    ...level2(),
    ...level3(),
    ...footer(),
  );
}

/** Re-paint the page.
 *
 *  A widget-panel repaint replaces the whole buffer, which parks the
 *  viewport back at line 0 — fine on a panel that fits its pane, wrong
 *  on a document you scroll. So the scroll line is captured and
 *  restored around the repaint. Folding only removes rows *below* a
 *  card's header, so every line above the viewport top is unchanged
 *  and the restore is exact rather than approximate. */
function render(): void {
  if (!panel) return;
  const top = trackedTop();
  panel.set(buildSpec());
  if (top > 0) scrollTopTo(top);
  void resolveLevelLines();
}

/** Where we believe the top of the pane is.
 *
 *  `getViewport().topLine` does not refresh between our own scrolls — a
 *  run of Down presses each recomputed from the same stale zero and
 *  re-issued the same target, so eight presses moved one line. Our own
 *  intent is the authority for relative scrolling; the observed value is
 *  adopted only when it changes, which is how a mouse-wheel scroll (or
 *  anything else that moves the view) gets picked up. */
let desiredTop = 0;
let lastObserved = 0;

function trackedTop(): number {
  const vp = editor.getViewport();
  const t = vp && typeof vp.topLine === "number" ? vp.topLine : null;
  if (t !== null && t !== lastObserved) {
    lastObserved = t;
    desiredTop = t;
  }
  return desiredTop;
}

/** Put `line` at the TOP of the pane.
 *
 *  `scrollBufferToLine` is a *reveal*: it deliberately leaves a third
 *  of the viewport as context above its target, which is right for
 *  "show me this match" and wrong for both of this page's uses — a
 *  level jump wants the banner at the top, and a repaint wants the
 *  reader's exact line back. Compensating here keeps that host
 *  behaviour intact rather than asking for a second scroll verb. */
function scrollTopTo(line: number): void {
  if (bufferId === null) return;
  desiredTop = Math.max(0, line);
  const vp = editor.getViewport();
  const h = vp && vp.height > 0 ? vp.height : 30;
  editor.scrollBufferToLine(bufferId, line + Math.floor(h / 3));
}

/** Read the painted buffer back and record which line each banner
 *  landed on, so `1`/`2`/`3` can scroll there. Layout belongs to the
 *  host; this asks rather than guesses. */
async function resolveLevelLines(): Promise<void> {
  if (bufferId === null) return;
  try {
    const t = await editor.getBufferText(bufferId);
    const lines = t.split("\n");
    for (const k of Object.keys(LEVEL_MARK)) {
      const idx = lines.findIndex((l) => l.includes(LEVEL_MARK[k]));
      if (idx >= 0) levelLines[k] = idx;
    }
    for (const id of Object.keys(CARD_TITLE)) {
      const idx = lines.findIndex((l) => l.includes(CARD_TITLE[id]));
      if (idx >= 0) cardLines[id] = idx;
    }
  } catch (_e) {
    // A read that fails just leaves the jump keys pointing at the last
    // known rows; nothing here is worth an error to the user.
  }
}

// ── Data probes ──────────────────────────────────────────────────────

async function probeRepoFiles(): Promise<void> {
  if (repoFilesLoading) return;
  repoFilesLoading = true;
  try {
    const res = await editor.spawnProcess("git", ["ls-files"], editor.getCwd());
    if (res.exit_code === 0) {
      repoFiles = res.stdout.split("\n").filter((l) => l.length > 0).slice(0, 5000);
      recomputeHits();
    } else {
      repoFiles = null;
    }
  } catch (_e) {
    repoFiles = null;
  }
  repoFilesLoading = false;
  render();
}

async function probeGit(): Promise<void> {
  try {
    const st = await editor.spawnProcess("git", ["status", "--porcelain", "-b"], editor.getCwd());
    if (st.exit_code === 0) {
      const lines = st.stdout.split("\n").filter((l) => l.length > 0);
      const head = lines.find((l) => l.startsWith("## "));
      if (head) {
        gitBranch = head.slice(3).split("...")[0].split(" ")[0];
      }
      gitDirty = lines.filter((l) => !l.startsWith("## "));
    }
  } catch (_e) {
    // Not a repo, or no git on PATH: the card says so.
  }
  gitProbed = true;
  render();
}

/** `getAllThemes()` answers with the registry *object* — canonical key
 *  to theme data — not a list, so the names are its keys. Builtins are
 *  asked for separately and put first: they are the ones every install
 *  has, which is what a welcome screen should offer. */
function probeThemes(): void {
  const names: string[] = [];
  const push = (v: unknown) => {
    if (v && typeof v === "object") {
      for (const k of Object.keys(v as Record<string, unknown>)) {
        if (k.length > 0 && !k.startsWith("_") && !names.includes(k)) names.push(k);
      }
    }
  };
  try {
    push(editor.getBuiltinThemes());
    push(editor.getAllThemes());
  } catch (_e) {
    // A theme registry we can't read just means no swatches on the card.
  }
  themeNames = names;
}

type OrchestratorApi = {
  listWorkspaces?: () => Array<Record<string, unknown>>;
  focusWorkspace?: (id: number) => unknown;
};

function probeWorkspaces(): void {
  try {
    const api = editor.getPluginApi("orchestrator") as OrchestratorApi | null;
    if (!api?.listWorkspaces) return;
    const rows = api.listWorkspaces();
    workspaces = rows.map((r) => ({
      name: String(r.name ?? ""),
      branch: String(r.branch ?? ""),
      agentState: String(r.agentState ?? "idle"),
      kind: String(r.kind ?? "live"),
      active: r.active === true,
      windowId: typeof r.windowId === "number" ? r.windowId : 0,
      dirty: 0,
    }));
  } catch (_e) {
    workspaces = [];
  }
}

// ── Config ───────────────────────────────────────────────────────────

editor.defineConfigBoolean("showOnStartup", {
  default: true,
  description: "Open the welcome screen when Fresh starts with nothing to restore, and after the last buffer is closed.",
});

/** The footer toggle writes plugin global state, which persists across
 *  restarts; the declared config field is the fallback, so the Settings
 *  UI still owns the setting for anyone who never touches the toggle.
 *  Same precedence the dashboard uses for its own auto-open override. */
function showOnStartup(): boolean {
  const override = editor.getGlobalState("showOnStartup");
  if (typeof override === "boolean") return override;
  const cfg = (editor.getPluginConfig() ?? {}) as { showOnStartup?: boolean };
  return cfg.showOnStartup !== false;
}

// ── Lifecycle ────────────────────────────────────────────────────────

function hasRealFiles(): boolean {
  return editor.listBuffers().some((b) => !b.is_virtual && b.path && b.path.length > 0);
}

async function openWelcome(force: boolean): Promise<void> {
  if (bufferId !== null) {
    editor.showBuffer(bufferId);
    return;
  }
  if (opening) return;
  if (force) dismissed = false;
  if (!force && (dismissed || !showOnStartup())) return;
  opening = true;
  try {
    const res = await editor.createVirtualBuffer({
      name: "Welcome",
      mode: "welcome",
      readOnly: true,
      showLineNumbers: false,
      showCursors: false,
      editingDisabled: true,
    });
    bufferId = res.bufferId;
    panel = new WidgetPanel(bufferId);
    // Fresh seeds an empty untitled buffer when it has nothing else to
    // show. The welcome screen is what that seed was standing in for, so
    // retire it rather than leaving a `[No Name]` tab beside this one.
    for (const b of editor.listBuffers()) {
      if (
        b.id !== bufferId && !b.is_virtual && !b.modified &&
        (!b.path || b.path.length === 0)
      ) {
        editor.closeBuffer(b.id);
      }
    }
    engaged = force;
    probeThemes();
    probeWorkspaces();
    render();
    // The panel auto-focuses its first widget, which is now the startup
    // toggle — a stray Enter on open would switch the screen off. Park
    // focus on the first door instead, which is what Enter should do
    // here anyway.
    panel.setFocusKey("jump:1");
    lastFocusedWidget = "jump:1";
    editor.showBuffer(bufferId);
    void probeRepoFiles();
    void probeGit();
  } catch (e) {
    editor.error(`welcome: ${e}`);
  }
  opening = false;
}

/** `dismiss` distinguishes the reader closing the page (stay away)
 *  from the page stepping aside for a file it had nothing to do with
 *  (stay available). */
function closeWelcome(dismiss: boolean): void {
  if (bufferId === null) return;
  const id = bufferId;
  if (dismiss) dismissed = true;
  panel?.unmount();
  panel = null;
  bufferId = null;
  editor.closeBuffer(id, true);
}

registerHandler("welcome_open", () => {
  void openWelcome(true);
});
editor.registerCommand(
  "Welcome",
  "Open the welcome screen",
  "welcome_open",
);

registerHandler("welcomeOnReady", async () => {
  if (!hasRealFiles()) await openWelcome(false);
});
registerHandler("welcomeOnBufferClosed", async (e: { buffer_id: number }) => {
  // The tab's `×` / `Ctrl+W` route: the buffer is gone and we were not
  // the ones who asked, so treat it as the reader dismissing the page.
  if (bufferId !== null && e.buffer_id === bufferId) {
    panel = null;
    bufferId = null;
    dismissed = true;
    return;
  }
  if (!hasRealFiles()) await openWelcome(false);
});
registerHandler("welcomeOnAfterFileOpen", (_e: { buffer_id: number; path: string }) => {
  // An auto-opened screen nobody touched was ambient — step aside. One
  // the reader engaged with is a document they are reading; leave it.
  if (bufferId === null || engaged) return;
  closeWelcome(false);
});
// `viewport_changed` fires on scroll and on every height change as
// well as on resize, and a repaint replaces the buffer's whole
// content — which cancels an open prompt and fights the reader for the
// viewport. So this listens to WIDTH only: width is what the layout
// actually depends on (the three doors fold below 96 columns, the
// wordmark below 60), and every list here pins its own `visibleRows`,
// so height changes have nothing to recompute. Opening the command
// palette shortens the pane by a row; without this guard that repaint
// landed mid-prompt and cancelled it.
let lastW = -1;
registerHandler(
  "welcomeOnViewportChanged",
  (d: { buffer_id: number; width: number }) => {
    if (bufferId === null || d.buffer_id !== bufferId) return;
    if (d.width === lastW) return;
    lastW = d.width;
    render();
  },
);

editor.on("ready", "welcomeOnReady");
editor.on("buffer_closed", "welcomeOnBufferClosed");
editor.on("after_file_open", "welcomeOnAfterFileOpen");
editor.on("viewport_changed", "welcomeOnViewportChanged");

// ── Keyboard ─────────────────────────────────────────────────────────

function dispatch(action: ReturnType<typeof widgetKey>): void {
  engaged = true;
  panel?.command(action);
}

async function jumpTo(level: string): Promise<void> {
  engaged = true;
  if (bufferId === null) return;
  // The banner rows are read back from the painted buffer, which is
  // async: a jump key pressed in the first moments after the page
  // opens would otherwise be a silent no-op. Resolve on demand.
  if (typeof levelLines[level] !== "number") await resolveLevelLines();
  const target = levelLines[level];
  if (typeof target !== "number") return;
  scrollTopTo(target);
}

registerHandler("welcome_jump_1", () => void jumpTo("1"));
registerHandler("welcome_jump_2", () => void jumpTo("2"));
registerHandler("welcome_jump_3", () => void jumpTo("3"));
registerHandler("welcome_jump_top", () => {
  engaged = true;
  if (bufferId !== null) editor.scrollBufferToLine(bufferId, 0);
});
registerHandler("welcome_tab", () => dispatch(widgetKey("Tab")));
registerHandler("welcome_shift_tab", () => dispatch(widgetKey("Shift+Tab")));
registerHandler("welcome_enter", () => {
  engaged = true;
  // A single-line Text widget treats Enter as advance-focus, so a
  // finder that only forwarded the key would move on rather than open
  // what the reader picked. Opening is this field's whole purpose.
  if (finderFocused()) {
    openFinderHit(finderIndex);
    return;
  }
  dispatch(widgetKey("Enter"));
});
registerHandler("welcome_space", () => dispatch(widgetKey("Space")));
function moveFinder(delta: number): void {
  if (finderHits.length === 0) return;
  finderIndex = (finderIndex + delta + finderHits.length) % finderHits.length;
  render();
}
registerHandler("welcome_up", () => {
  engaged = true;
  if (finderFocused()) moveFinder(-1);
  else scrollLine(false);
});
registerHandler("welcome_down", () => {
  engaged = true;
  if (finderFocused()) moveFinder(1);
  else scrollLine(true);
});
/** One line, on the editor's own scroll path — the same one the mouse
 *  wheel takes. `scrollTopTo` is absolute and right for a jump, but its
 *  reveal arithmetic saturates to no movement for a delta this small. */
/** Three lines a press: enough to read by, and the mouse wheel is there
 *  for finer work. */
function scrollLine(down: boolean): void {
  engaged = true;
  scrollTopTo(Math.max(0, trackedTop() + (down ? 3 : -3)));
}

/** A page, computed: the reveal offset is immaterial at this size, and
 *  `move_page_*` would page the cursor rather than the view. */
function pageBy(sign: number): void {
  engaged = true;
  const vp = editor.getViewport();
  const h = vp && vp.height > 0 ? vp.height : 24;
  scrollTopTo(Math.max(0, trackedTop() + sign * Math.max(1, h - 2)));
}

registerHandler("welcome_page_up", () => pageBy(-1));
registerHandler("welcome_page_down", () => pageBy(1));
registerHandler("welcome_left", () => dispatch(widgetKey("Left")));
registerHandler("welcome_right", () => dispatch(widgetKey("Right")));
registerHandler("welcome_backspace", () => dispatch(widgetKey("Backspace")));
registerHandler("welcome_delete", () => dispatch(widgetKey("Delete")));
/** Escape leaves the finder before it leaves the page: a reader who
 *  pressed it to get out of the search field did not ask to close the
 *  document they were reading. */
registerHandler("welcome_close", () => {
  if (finderFocused()) {
    lastFocusedWidget = "";
    panel?.setFocusKey("");
    render();
    return;
  }
  closeWelcome(true);
});

// A mode that declares `allowTextInput` owns the keyboard: the host
// blocks unbound Ctrl-/Alt-modified keys so a focused text field can
// never be hijacked by Open or Save. That is the right default and it
// means the handful of accelerators this page promises have to be
// named. Each one forwards to the real action, so a rebound key keeps
// working — the labels on the page come from the same resolver.
const FORWARDED: Array<[string, string]> = [
  ["C-p", "quick_open"],
  ["C-o", "open"],
  ["C-n", "new"],
  ["C-b", "toggle_file_explorer"],
  ["M-o", "toggle_dock_focus"],
  ["M-/", "open_live_grep"],
  ["F1", "show_help"],
];
for (const [, action] of FORWARDED) {
  // Deliberately does NOT mark the page engaged: reaching past it for
  // the palette is the ambient case, not a reader settling in. If the
  // key you pressed opens a file, this screen should still step aside.
  registerHandler(`welcome_do_${action}`, () => editor.executeAction(action));
}

/** `/` puts focus in the finder from anywhere on the page, so reaching
 *  it never means counting Tab stops. `setFocusKey` is the widget
 *  runtime's own focus mutation, so the host stays the single owner of
 *  focus. */
registerHandler("welcome_focus_find", () => {
  engaged = true;
  if (!panel) return;
  if (folded.has("finder")) {
    folded.delete("finder");
    render();
  }
  lastFocusedWidget = "finderField";
  panel.setFocusKey("finderField");
  void jumpTo("1");
});

registerHandler("mode_text_input", (args: { text: string }) => {
  if (!panel || !args?.text) return;
  // A bare digit with nothing focused is a level jump; once the finder
  // has focus the same key is just a character, which the widget
  // runtime decides by owning focus.
  engaged = true;
  panel.command(textInputChar(args.text));
});

editor.defineMode(
  "welcome",
  [
    ["1", "welcome_jump_1"],
    ["2", "welcome_jump_2"],
    ["3", "welcome_jump_3"],
    ["0", "welcome_jump_top"],
    ["Tab", "welcome_tab"],
    ["S-Tab", "welcome_shift_tab"],
    ["Return", "welcome_enter"],
    ["Space", "welcome_space"],
    ["Up", "welcome_up"],
    ["Down", "welcome_down"],
    ["PageUp", "welcome_page_up"],
    ["PageDown", "welcome_page_down"],
    ["Left", "welcome_left"],
    ["Right", "welcome_right"],
    ["Backspace", "welcome_backspace"],
    ["Delete", "welcome_delete"],
    ["/", "welcome_focus_find"],
    ["Escape", "welcome_close"],
    ...FORWARDED.map(([k, action]) => [k, `welcome_do_${action}`] as [string, string]),
  ],
  true,
  true,
);

// ── Widget events ────────────────────────────────────────────────────

function activateKey(k: string): void {
  engaged = true;
  if (k.startsWith("fold:")) {
    const id = k.slice(5);
    if (folded.has(id)) folded.delete(id);
    else folded.add(id);
    render();
    return;
  }
  if (k.startsWith("jump:")) {
    void jumpTo(k.slice(5));
    return;
  }
  if (k.startsWith("theme:")) {
    const name = k.slice(6);
    editor.applyTheme(name);
    activeTheme = name;
    render();
    return;
  }
  if (k.startsWith("ws:")) {
    const id = Number(k.slice(3));
    const api = editor.getPluginApi("orchestrator") as OrchestratorApi | null;
    api?.focusWorkspace?.(id);
    return;
  }
  switch (k) {
    case "act_open":
      editor.executeAction("open");
      return;
    case "act_recent":
      editor.executeAction("quick_open");
      return;
    case "act_new":
      editor.executeAction("new");
      return;
    case "act_review":
      editor.executeAction("start_review_branch");
      return;
    case "act_gitlog":
      // The git-log plugin's handler is `show_git_log`; `git_log` is
      // only the palette label. An unknown name is dispatched as a
      // plugin action, finds no handler in any context, and fails in
      // the log rather than on screen — so the name has to be right.
      editor.executeAction("show_git_log");
      return;
    case "act_ws_new":
      editor.executeAction("orchestrator_new");
      return;
    case "act_ws_agent":
      editor.executeAction("orchestrator_run_agent");
      return;
    case "act_ws_dock":
      editor.executeAction("toggle_dock_focus");
      return;
    case "startupToggle":
      return;
    default:
      return;
  }
}

/** Bring `line` into view, but only when it is not already on screen —
 *  a focus move that lands somewhere visible should not yank the page. */
function revealLine(line: number): void {
  const vp = editor.getViewport();
  if (!vp) return;
  const top = typeof vp.topLine === "number" ? vp.topLine : 0;
  if (line >= top && line < top + vp.height - 2) return;
  scrollTopTo(Math.max(0, line - 1));
}

function openFinderHit(index: number): void {
  const hit = finderHits[index];
  if (!hit) return;
  editor.openFile(hit.path);
}

editor.on("widget_event", (args) => {
  if (!panel || args.panel_id !== panel.id()) return;
  engaged = true;
  const k = typeof args.widget_key === "string" ? args.widget_key : "";
  if (k.length > 0) lastFocusedWidget = k;

  // Tab / Shift+Tab move focus; the host scrolls the pane only for a
  // focused text widget, so a focused button further down this
  // document would otherwise be invisible. Reveal its card.
  if (args.event_type === "focus") {
    const id = cardForWidget(k);
    if (id !== null && typeof cardLines[id] === "number") revealLine(cardLines[id]);
    else if (k.startsWith("jump:") || k.startsWith("act_")) scrollTopTo(0);
    else if (k === "startupToggle") scrollTopTo(0);
    return;
  }

  if (args.event_type === "change" && k === "finderField") {
    const payload = args.payload as { value?: string; cursorByte?: number } | undefined;
    if (typeof payload?.value !== "string") return;
    finderQuery = payload.value;
    finderCursor = typeof payload.cursorByte === "number" ? payload.cursorByte : finderQuery.length;
    recomputeHits();
    render();
    return;
  }

  if (args.event_type === "toggle" && k === "startupToggle") {
    const payload = args.payload as { checked?: boolean } | undefined;
    const next = payload?.checked === true;
    editor.setGlobalState("showOnStartup", next);
    render();
    return;
  }

  if (args.event_type === "activate") {
    if (k === "finderField") {
      openFinderHit(finderIndex);
      return;
    }
    activateKey(k);
  }
});

editor.debug("Welcome screen plugin loaded");
