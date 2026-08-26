/// <reference path="./lib/fresh.d.ts" />
import {
  button,
  col,
  divider,
  flexSpacer,
  labeledSection,
  list,
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
  value: "syntax.string",
  muted: "syntax.comment",
  key: "ui.help_key_fg",
  frame: "ui.popup_border_fg",
  ok: "ui.file_status_added_fg",
  warn: "syntax.constant",
  err: "diagnostic.error_fg",
};

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
  return lastFocusedWidget === "finderField" || lastFocusedWidget === "finderList";
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
  if (k === "finderField" || k === "finderList") return "finder";
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

/** One clickable verb: `▸ Label      Ctrl+X`. */
function verb(key: string, label: string, action: string): WidgetSpec {
  return row(
    spacer(2),
    button(`▸ ${label}`, { key, bare: true, hoverStyle: { fg: C.accent } }),
    ...accelSpec(action),
  );
}

/** A foldable card. The gutter arrow is a bare button, so folding is a
 *  click or a Tab-and-Enter away, and the card collapses to its header. */
function card(
  id: string,
  title: string,
  hint: string,
  body: () => WidgetSpec[],
): WidgetSpec {
  const open = !folded.has(id);
  const header = row(
    button(open ? "▾" : "▸", {
      key: `fold:${id}`,
      bare: true,
      hoverStyle: { fg: C.accent },
    }),
    spacer(1),
    line([{ text: title, style: { fg: C.accent, bold: true } }]),
    flexSpacer(),
    line([{ text: hint, style: { fg: C.muted } }]),
    spacer(1),
  );
  if (!open) return labeledSection({ child: header, key: `card:${id}` });
  return labeledSection({
    child: col(header, divider({ style: { fg: C.frame } }), ...body()),
    key: `card:${id}`,
  });
}

function banner(level: string, sub: string): WidgetSpec {
  return col(
    blank(),
    line([
      { text: "──── ", style: { fg: C.frame } },
      { text: LEVEL_MARK[level], style: { fg: C.title, bold: true } },
      { text: " " + "─".repeat(40), style: { fg: C.frame } },
    ]),
    line([{ text: sub, style: { fg: C.muted } }]),
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

function hero(): WidgetSpec[] {
  const wide = viewportWidth() >= 60;
  const art = wide
    ? ART.map((l) => line([{ text: "  " + l, style: { fg: C.art } }]))
    : [line([{ text: "  fresh", style: { fg: C.art, bold: true } }])];
  return [
    blank(),
    ...art,
    blank(),
    plain("  A terminal text editor and IDE.  It grows when your work does.", C.value),
    line([
      { text: "  single static binary", style: { fg: C.muted } },
      { text: "  ·  ", style: { fg: C.frame } },
      { text: "zero configuration", style: { fg: C.muted } },
      { text: "  ·  ", style: { fg: C.frame } },
      { text: "open source", style: { fg: C.muted } },
    ]),
  ];
}

// ── The three doors ──────────────────────────────────────────────────

type Door = { n: string; head: string; sub: string; body: string[] };

const DOORS: Door[] = [
  {
    n: "1",
    head: "[1] JUST EDIT TEXT",
    sub: "Open a file & go",
    body: ["Notes, configs, huge logs.", "Standard keys, full mouse —", "nothing to learn first."],
  },
  {
    n: "2",
    head: "[2] CLASSIC IDE",
    sub: "Code with LSP & git",
    body: ["Completions, goto & hover,", "hunk-level diff review,", "splits, themes, plugins."],
  },
  {
    n: "3",
    head: "[3] ORCHESTRATE",
    sub: "Run agents in parallel",
    body: ["One workspace per worktree —", "claude, codex, aider and", "remotes. Tour the diffs."],
  },
];

function doorCard(d: Door): WidgetSpec {
  return labeledSection({
    label: d.head,
    widthPct: 33,
    key: `door:${d.n}`,
    child: col(
      plain(d.sub, C.value),
      blank(),
      ...d.body.map((b) => plain(b, C.muted)),
      blank(),
      button(`jump ↓  ·  press ${d.n}`, {
        key: `jump:${d.n}`,
        bare: true,
        hoverStyle: { fg: C.accent },
      }),
    ),
  });
}

function doors(): WidgetSpec[] {
  const wide = viewportWidth() >= 96;
  const cards = DOORS.map(doorCard);
  return [
    blank(),
    line([{ text: "  ── WHAT BRINGS YOU HERE? ──", style: { fg: C.muted } }]),
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
  return card("finder", "Pick up where you left off", "this box is live — type in it", () => {
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
      rows.push(
        list({
          items: finderHits.slice(0, 60).map((h, i) =>
            styledRow([
              { text: i === finderIndex ? " ▸ " : "   ", style: { fg: C.accent } },
              { text: h.path, style: { fg: i === finderIndex ? C.value : C.muted } },
            ])
          ),
          selectedIndex: finderIndex,
          visibleRows: Math.min(6, finderHits.length),
          key: "finderList",
        }),
      );
    }
    rows.push(blank());
    rows.push(
      plain("  Fresh remembers your cursor position in every file. Hot Exit restores", C.muted),
    );
    rows.push(plain("  unsaved buffers after a crash — even unnamed scratch ones.", C.muted));
    rows.push(blank());
    return rows;
  });
}

function level1(): WidgetSpec[] {
  return [
    banner("1", "Open a file. Type. Save. Fresh stays out of the way."),
    finderCard(),
    blank(),
    card("ugly", "Built for the ugly files too", "click ▾ to fold any card", () => [
      blank(),
      plain("  · Multi-GB files open without blocking the UI — logs, dumps, CSVs.", C.muted),
      plain("  · Instant startup; text appears as you type. Small memory footprint.", C.muted),
      plain("  · Encodings beyond UTF-8: UTF-16, GBK, Shift-JIS, Latin-1 and more.", C.muted),
      plain("  · Project-wide search & replace with regex — even across unsaved buffers.", C.muted),
      blank(),
    ]),
    blank(),
    card("editorvar", "Make it your $EDITOR", "quality-of-life from day one", () => [
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
  "pub struct UserStore {",
  "    users: HashMap<u64, User>,",
  "}",
  "",
  "impl UserStore {",
  "    pub fn active_users(&self) -> impl Iterator<Item = &User> {",
  "        self.users.values().filter(|u| u.is_active)",
  "    }",
  "}",
  "```",
].join("\n");

function level2(): WidgetSpec[] {
  return [
    banner("2", "Language servers, git review, themes — here the whole time, waiting."),
    card("lsp", "Language smarts, zero setup", "this block is really highlighted", () => [
      blank(),
      text({
        value: SAMPLE,
        rows: 11,
        markdown: true,
        readOnly: true,
        fullWidth: true,
        // Deliberately keyless: a keyed widget joins the Tab cycle, and
        // a read-only sample is something to look at, not a stop on the
        // way to the next control.
      }),
      blank(),
      plain("  · Open a file and the language server starts itself. Hover, goto,", C.muted),
      plain("    references, rename, code actions and diagnostics, with no setup.", C.muted),
      plain("  · Configs shipped for Python, TypeScript, Rust, Go, Java, C/C++ and more.", C.muted),
      plain("  · Run multiple servers per language with merged completions.", C.muted),
      blank(),
    ]),
    blank(),
    gitCard(),
    blank(),
    themeCard(),
    blank(),
    card("power", "Power tools when your hands get fast", "optional, all of it", () => [
      blank(),
      plain("  · Multi-cursor and block selection, keyboard macros, sort lines.", C.muted),
      plain("  · Command palette with prefix routing: > commands · # buffers · : lines.", C.muted),
      plain("  · Vi mode with operators, motions and text objects — if that's your thing.", C.muted),
      plain("  · TypeScript plugins, sandboxed in QuickJS. No node_modules on disk.", C.muted),
      plain("  · Tabs, split panes, integrated terminal, markdown preview.", C.muted),
      blank(),
    ]),
  ];
}

function gitCard(): WidgetSpec {
  return card("git", "Review your diff before it reviews you", "your real working tree", () => {
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
            style: { fg: gitDirty.length === 0 ? C.ok : C.warn },
          },
        ]),
      );
      rows.push(blank());
      for (const f of gitDirty.slice(0, 6)) {
        rows.push(
          line([
            { text: "   " + f.slice(0, 2), style: { fg: C.warn } },
            { text: " " + f.slice(3), style: { fg: C.muted } },
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
    rows.push(plain("  Hunk-level stage / unstage / discard. Side-by-side diff, review", C.muted));
    rows.push(plain("  notes, git gutter, git grep.", C.muted));
    rows.push(blank());
    return rows;
  });
}

function themeCard(): WidgetSpec {
  return card("themes", "Make it yours", "these restyle the editor, live", () => {
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
      plain("  Live theme editor with \"Inspect Theme at Cursor\". Configurable status", C.muted),
      plain("  bar. UI translated to 日本語, 한국어, 中文, Tiếng Việt and more.", C.muted),
      blank(),
    ];
  });
}

// ── Level 3 ──────────────────────────────────────────────────────────

function stateGlyph(w: Workspace): { text: string; fg: string } {
  if (w.kind === "discovered") return { text: "○", fg: C.muted };
  if (w.agentState === "working") return { text: "●", fg: C.ok };
  return { text: "◐", fg: C.warn };
}

function orchestratorCard(): WidgetSpec {
  return card("orch", "The Orchestrator dock", "your real workspaces", () => {
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
          { text: "◐ idle   ", style: { fg: C.warn } },
          { text: "○ discovered worktree", style: { fg: C.muted } },
        ]),
      );
    }
    rows.push(blank());
    rows.push(
      row(
        spacer(2),
        button("New workspace…", { key: "act_ws_new", hoverStyle: { fg: C.accent } }),
        spacer(2),
        button("Run agent here…", { key: "act_ws_agent", hoverStyle: { fg: C.accent } }),
        spacer(2),
        button("Open the dock", { key: "act_ws_dock", hoverStyle: { fg: C.accent } }),
      ),
    );
    rows.push(blank());
    rows.push(plain("  One workspace per git worktree, each with its own terminals and", C.muted));
    rows.push(plain("  agent. Sessions resume after a restart. Leave the rest running.", C.muted));
    rows.push(blank());
    return rows;
  });
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
    row(
      spacer(2),
      toggle(showOnStartup(), "Show this screen on startup", { key: "startupToggle" }),
    ),
    blank(),
  ];
}

// ── Assembly ─────────────────────────────────────────────────────────

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
    labeledSection({
      key: "reassure",
      child: col(
        plain("Nothing to learn first. It works like you'd expect: Ctrl+S saves,", C.value),
        plain("Ctrl+Z undoes, Ctrl+F finds, Ctrl+C/V copy-paste — and the mouse", C.value),
        plain("just works. Click, drag, scroll, select.", C.value),
      ),
    }),
    blank(),
    plain("            ▼ scroll — the rest is here when you need it ▼", C.muted),
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
  const before = editor.getViewport();
  const top = before && typeof before.topLine === "number" ? before.topLine : 0;
  panel.set(buildSpec());
  if (top > 0) scrollTopTo(top);
  void resolveLevelLines();
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
  if (!force && !showOnStartup()) return;
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
    editor.showBuffer(bufferId);
    void probeRepoFiles();
    void probeGit();
  } catch (e) {
    editor.error(`welcome: ${e}`);
  }
  opening = false;
}

function closeWelcome(): void {
  if (bufferId === null) return;
  const id = bufferId;
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
  if (bufferId !== null && e.buffer_id === bufferId) {
    panel = null;
    bufferId = null;
    return;
  }
  if (!hasRealFiles()) await openWelcome(false);
});
registerHandler("welcomeOnAfterFileOpen", (_e: { buffer_id: number; path: string }) => {
  // An auto-opened screen nobody touched was ambient — step aside. One
  // the reader engaged with is a document they are reading; leave it.
  if (bufferId === null || engaged) return;
  closeWelcome();
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
registerHandler("welcome_enter", () => dispatch(widgetKey("Enter")));
registerHandler("welcome_space", () => dispatch(widgetKey("Space")));
registerHandler("welcome_up", () => {
  engaged = true;
  if (finderFocused()) dispatch(widgetKey("Up"));
  else editor.executeAction("scroll_up");
});
registerHandler("welcome_down", () => {
  engaged = true;
  if (finderFocused()) dispatch(widgetKey("Down"));
  else editor.executeAction("scroll_down");
});
/** Page keys scroll the view, they do not move a cursor. `move_page_*`
 *  would page the *cursor*, and this buffer's cursor is parked wherever
 *  the widget runtime last put it — paging from there jumps somewhere
 *  the reader never was. */
function pageBy(sign: number): void {
  engaged = true;
  const vp = editor.getViewport();
  if (!vp) return;
  const top = typeof vp.topLine === "number" ? vp.topLine : 0;
  const step = Math.max(1, vp.height - 2);
  scrollTopTo(Math.max(0, top + sign * step));
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
  closeWelcome();
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
      editor.executeAction("git_log");
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
    else if (k === "startupToggle") pageBy(1);
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

  if (args.event_type === "select" && k === "finderList") {
    const payload = args.payload as { index?: number; via?: string } | undefined;
    if (typeof payload?.index !== "number") return;
    finderIndex = payload.index;
    if (payload.via === "click") openFinderHit(finderIndex);
    else render();
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
    if (k === "finderList" || k === "finderField") {
      openFinderHit(finderIndex);
      return;
    }
    activateKey(k);
  }
});

editor.debug("Welcome screen plugin loaded");
