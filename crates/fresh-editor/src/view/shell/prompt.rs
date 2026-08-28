//! The prompt's suggestion list, as a description.
//!
//! The first surface where the migration is mostly *deletion by concept*. The
//! ledger (`docs/internal/fresh-ui-parity-ledger-prompt.md`) enumerated eleven
//! rules the painter and the chrome component enforce; ten of them are things
//! `fresh-ui` already says, so the work is naming the concept rather than
//! porting the code:
//!
//! ```text
//!   the visible window around the selection   -> list().windowed(..)
//!   hover reports a row, click selects it     -> list().on_select(..)
//!   double-click confirms                     -> list().on_activate(..)
//!   the selected row is highlighted           -> list().selected(i)
//!   a scrollbar that jumps and drags          -> list().scrollbar()   (hit.rs owns the drag)
//! ```
//!
//! And the column budget — the reason `Node::priority` exists. The row is four
//! columns and the rule is a *yield order*: names are never truncated while
//! room remains, the description absorbs the squeeze first, the source column
//! last. `flex` cannot say that; it resolves children against what is left, in
//! declaration order, which is placement rather than precedence. The status bar
//! had already written that rule out by hand as `left_budget`. Two surfaces
//! needing it is what made it a library concept instead of a second budget
//! function here.

use std::rc::Rc;

use fresh_ui::widgets::RowState;
use fresh_ui::{col, row, text, text_runs, Elide, Key, Node, Run, Sizing};

use crate::app::shell_host::shell_theme::{attrs, pair, with_bg, with_fg};

use super::msg::{UiFact, UiMsg};

/// How many rows the list shows at once. The painter's own constant, kept
/// where the description can see it.
pub use crate::view::prompt::MAX_VISIBLE_SUGGESTIONS;

/// Which column yields first when the row runs out of room.
///
/// The numbers are only an order — `Node::priority` compares them and nothing
/// else. Named rather than inlined because the *order* is the rule the painter
/// enforced in prose ("names are never truncated while room remains") and a
/// bare `.priority(3)` at each call site would lose it again.
mod yields_last {
    /// Sized first: a command palette that hides the command name has failed.
    pub const NAME: u8 = 3;
    /// The shortcut is short and fixed; it is not worth squeezing.
    pub const KEYBINDING: u8 = 2;
    /// Where a command came from — useful, but the first thing to lose after
    /// the description.
    pub const SOURCE: u8 = 1;
    /// Absorbs the squeeze. Default, stated for symmetry with the others.
    pub const DESCRIPTION: u8 = 0;
}

/// A piece of a description a plugin styled itself.
///
/// Already in the grammar rather than in colours: `fg` and `bg` are theme-key
/// names or `#rrggbb` literals, which is exactly what `shell_theme` reads. The
/// painter resolved a plugin's `OverlayColorSpec` to a concrete `Color` here
/// and lost its provenance on the way; a name keeps it, and a span that names
/// only one half inherits the other from the row it sits on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DescriptionSpan {
    pub text: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    /// `shell_theme`'s attribute names: `bold`, `italic`, `underline`,
    /// `strikethrough`.
    pub attrs: Vec<&'static str>,
}

/// One row of the list, as content. No geometry: the columns are placed by
/// layout, and which of them survives a narrow row is `priority`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SuggestionRow {
    pub name: String,
    pub keybinding: Option<String>,
    pub description: Option<String>,
    /// A description a plugin styled piece by piece. Wins over `description`,
    /// the same way `push_description_column` checks it first.
    pub description_spans: Option<Vec<DescriptionSpan>>,
    pub source: Option<String>,
    pub disabled: bool,
}

/// The list itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub rows: Vec<SuggestionRow>,
    /// Which row is selected, if any. Controlled: the editor holds it.
    pub selected: Option<usize>,
}

impl Suggestions {
    /// Which end of a name survives a narrow row.
    ///
    /// `ColumnLayout::names_are_paths` decided this from the shape of the list
    /// rather than from a flag: a list with neither keybindings nor sources is
    /// a file finder, and a path keeps its filename. A command palette keeps
    /// its head — "Toggle Compose/Preview (All Files)" contains a slash and is
    /// still a command name, which is the bug that rule was written for.
    fn name_elide(&self) -> Elide {
        let has = |f: fn(&SuggestionRow) -> bool| self.rows.iter().any(f);
        if has(|r| r.keybinding.is_some()) || has(|r| r.source.is_some()) {
            Elide::Tail
        } else {
            Elide::Head
        }
    }
}

pub fn row_key(index: usize) -> Key {
    Key::Pair("suggestion".into(), index as u64)
}

/// The name column of a row. Keyed so the width rule can be read back off the
/// tree — the same way the status bar's segments and the explorer's slots are.
pub fn name_key(index: usize) -> Key {
    Key::Pair("suggestion_name".into(), index as u64)
}

/// The painter's own ladder, in the painter's own keys.
///
/// **Read off `suggestion_style`, not invented.** `shell_theme`'s contract is
/// that a name is real theme keys — both halves go through
/// `Theme::resolve_theme_key`, and a name it does not know falls back to the
/// base style *silently*. An earlier draft of this used
/// `ui.suggestion_selected_fg`, `ui.suggestion_description_fg` and three more
/// that exist nowhere, which would have painted every row in the default
/// colour with nothing to show for it. `every_theme_name_is_a_real_key` is the
/// guard.
///
/// The state comes from `List` and the names come from here — that is what
/// `row_theme` is for. Without it the widget stamps its own vocabulary
/// (`list.row.selected`), which this editor's theme has no entry for, so every
/// row would have painted in the base style and the highlight would have
/// vanished. Hover included: `hovered` lives in `ListState`, so the ladder's
/// `menu_hover_*` arm is only reachable by being *told* the state.
///
/// One deliberate difference. The painter greys a disabled row with a
/// hardcoded `Color::DarkGray` + `DIM`, which no theme can reach;
/// `editor.line_number_fg` is the theme's own muted foreground and is what the
/// other migrated surfaces use for the same job. That makes a colour
/// themeable that was not.
fn row_bg(disabled: bool, st: RowState) -> &'static str {
    match st {
        RowState::Selected | RowState::SelectedBlur => "ui.suggestion_selected_bg",
        // A disabled row ignores hover — the painter's `row_base_style` picks
        // its background from `is_selected` alone.
        RowState::Hover if !disabled => "ui.menu_hover_bg",
        _ => "ui.suggestion_bg",
    }
}

/// The row's own style, and the default for every column that does not name
/// one — the painter's `base_style`.
fn theme(disabled: bool, st: RowState) -> String {
    let bg = row_bg(disabled, st);
    if disabled {
        // `dim` is new to the grammar and is what makes this a faithful port
        // rather than a near one: the painter reached for `Modifier::DIM`
        // directly, which no name could carry and no theme could override.
        return attrs("editor.line_number_fg", bg, &["dim"]);
    }
    match st {
        RowState::Selected | RowState::SelectedBlur => pair("ui.popup_selection_fg", bg),
        RowState::Hover => pair("ui.menu_hover_fg", bg),
        RowState::Normal => pair("ui.popup_text_fg", bg),
    }
}

/// A column with a foreground of its own. Disabled wins over all of them: the
/// painter returns `base_style` unchanged from every column's ladder, so a
/// greyed row is grey the whole way across.
fn column(disabled: bool, st: RowState, fg: &str) -> String {
    if disabled {
        theme(disabled, st)
    } else {
        pair(fg, row_bg(disabled, st))
    }
}

/// The source label is always dimmed — `source_style`'s three arms differ only
/// in background.
fn source_theme(disabled: bool, st: RowState) -> String {
    if disabled {
        return theme(disabled, st);
    }
    attrs("editor.line_number_fg", row_bg(disabled, st), &["dim"])
}

/// The keybinding reads as a shortcut on a row the eye is on, and recedes to
/// the muted foreground otherwise — `keybinding_style`'s three arms.
fn keybinding_theme(disabled: bool, st: RowState) -> String {
    let fg = match st {
        RowState::Normal => "editor.line_number_fg",
        _ => "ui.help_key_fg",
    };
    column(disabled, st, fg)
}

/// Every name this module can hand to `shell_theme`. The guard test walks it;
/// nothing else should need it.
#[cfg(test)]
fn every_theme_name() -> Vec<String> {
    let states = [
        RowState::Normal,
        RowState::Hover,
        RowState::Selected,
        RowState::SelectedBlur,
    ];
    let mut out = Vec::new();
    for st in states {
        for disabled in [false, true] {
            out.push(theme(disabled, st));
            out.push(keybinding_theme(disabled, st));
            out.push(source_theme(disabled, st));
        }
    }
    out
}

/// A plugin's span as a run, layered over the row's own name.
///
/// Half-named on purpose: `styled_span_style` started from the row's style and
/// set only what the span mentioned, so a span that names a foreground keeps
/// the selection's background under it. `with_fg` and `with_bg` are that, in
/// the grammar.
fn span_run(sp: &DescriptionSpan, row: &str) -> Run {
    let mut name = row.to_string();
    if let Some(fg) = &sp.fg {
        name = with_fg(&name, fg);
    }
    if let Some(bg) = &sp.bg {
        name = with_bg(&name, bg);
    }
    if !sp.attrs.is_empty() {
        let (fg, bg) = name.split_once('/').unwrap_or((name.as_str(), ""));
        let bg = bg.split('+').next().unwrap_or(bg);
        name = attrs(fg, bg, &sp.attrs);
    }
    Run::themed(sp.text.clone(), name)
}

/// One row's four columns, in paint order, each carrying the priority that says
/// when it yields.
fn node_row(index: usize, r: &SuggestionRow, st: RowState, name_elide: Elide) -> Node<UiMsg> {
    let t = theme(r.disabled, st);
    let mut cells: Vec<Node<UiMsg>> = vec![text(r.name.clone())
        .theme(t.clone())
        .key(name_key(index))
        .elide(name_elide)
        .priority(yields_last::NAME)];

    // A flexible gap rather than padding: it is what puts the trailing columns
    // at the right edge, and `min_w` keeps one cell of air when the row is
    // tight — the same floor the explorer's rows use.
    cells.push(row().flex(1).min_w(1));

    // The description carries no colour of its own: the painter draws it in
    // `base_style`, so it is the row. A plugin's styled description is the
    // same run with its pieces named — one node either way, because the pieces
    // are one logical string and must wrap and elide as one.
    match (&r.description_spans, &r.description) {
        (Some(spans), _) => cells.push(
            text_runs(spans.iter().map(|sp| span_run(sp, &t)))
                .theme(t.clone())
                .elide(Elide::Tail)
                .priority(yields_last::DESCRIPTION),
        ),
        (None, Some(d)) => cells.push(
            text(d.clone())
                .theme(t.clone())
                .elide(Elide::Tail)
                .priority(yields_last::DESCRIPTION),
        ),
        (None, None) => {}
    }
    if let Some(k) = &r.keybinding {
        cells.push(
            text(k.clone())
                .theme(keybinding_theme(r.disabled, st))
                .priority(yields_last::KEYBINDING),
        );
    }
    if let Some(sc) = &r.source {
        cells.push(
            text(sc.clone())
                .theme(source_theme(r.disabled, st))
                .elide(Elide::Tail)
                .priority(yields_last::SOURCE),
        );
    }

    // No theme on the row itself: `row_theme` names it, and a name here would
    // be overwritten anyway.
    row().h(Sizing::Cells(1)).children(cells)
}

/// The suggestion list as a description.
///
/// `windowed` is what replaces the painter's `scroll_offset` bookkeeping: the
/// library asks for the rows it can show and the editor resolves each index
/// against its own storage, so no window is stored on either side.
pub fn suggestions(s: &Suggestions) -> Node<UiMsg> {
    let rows: Vec<SuggestionRow> = s.rows.clone();
    let selected = s.selected;
    let rows_for_row = Rc::new(rows);
    let rows_for_key = rows_for_row.clone();
    let rows_for_theme = rows_for_row.clone();
    let name_elide = s.name_elide();

    // `List` reports the state it holds; the names are this module's. Both the
    // row builder and `row_theme` need the state, and only the latter is given
    // it — so the builder paints the columns and the row's own fill comes from
    // here. `selected` is consulted rather than the widget's state because an
    // empty selection is a real prompt state and `List` has no way to say "no
    // row": with none set it falls back to row 0.
    let hover_state = move |i: usize, st: RowState| -> RowState {
        match st {
            RowState::Selected | RowState::SelectedBlur if selected != Some(i) => RowState::Normal,
            other => other,
        }
    };

    let mut list = fresh_ui::widgets::List::windowed(
        rows_for_key.len(),
        move |i| row_key(i),
        move |i| match rows_for_row.get(i) {
            Some(r) => node_row(
                i,
                r,
                if selected == Some(i) {
                    RowState::Selected
                } else {
                    RowState::Normal
                },
                name_elide,
            ),
            None => row().h(Sizing::Cells(1)),
        },
    )
    .row_theme(move |i, st| {
        let st = hover_state(i, st);
        theme(rows_for_theme.get(i).is_some_and(|r| r.disabled), st)
    })
    .scrollbar()
    // A click reports the row; what that *means* is the prompt type's
    // business — `select_suggestion` confirms when `click_confirms()` says a
    // click commits, and otherwise syncs the input. That decision was already
    // editor-side; what the list removes is the coordinate hit-test in front
    // of it (`handle_click_suggestions` recovering an index the row knew).
    //
    // `on_activate` is deliberately not set. The widget fires it on a *single*
    // click and lets it win over `on_select`, so setting both would confirm
    // every click — see the double-click gap in the parity ledger.
    .on_select(|i| UiMsg::Ui(UiFact::SuggestionSelect(i)));
    if let Some(i) = selected {
        list = list.selected(i);
    }

    col().children([fresh_ui::ComponentExt::node(list)])
}

/// The list as an overlay above the prompt row.
///
/// **Placement is declared, not computed.** The painter measured the row
/// count, subtracted it from the prompt line's `y`, and flipped the box below
/// when it would not fit above — arithmetic that had to agree with a second
/// copy in `chrome::Prompt::collect` for the click rail to hit the right
/// cells. `Anchor::Node` names the row it sits on and `Place::Above` says
/// which side, with `Fit::FLIP` for the case that used to be an `if`.
pub fn suggestions_layer(s: &Suggestions) -> Node<UiMsg> {
    use fresh_ui::{layer, Anchor, Fit, Place};
    layer()
        .key(LAYER_KEY.with(|k| k.clone()))
        .anchor(Anchor::Node(super::frame::region_key(
            super::frame::HostRegion::PromptLine,
        )))
        .place(Place::Above)
        .fit(Fit::FLIP.or(Fit::CLAMP))
        // Not modal. The old encoding covered the frame below z15 to stop a
        // click reaching the *body*, which is a rule about a host leaf rather
        // than about this layer — see the ledger's withdrawn finding D.
        .child(suggestions(s))
}

thread_local! {
    static LAYER_KEY: Key = Key::Str("prompt_suggestions".into());
}

/// Where the list landed, read back off the laid-out tree.
///
/// The partner of `frame::regions_of` for a surface that is not a host region,
/// the same shape as `context_menu::menu_rect`. It replaces
/// `ChromeLayout::suggestions_outer_area`, which `render` recorded and the
/// click rail and the web `Scene` read back.
pub fn suggestions_rect(spec: &fresh_ui::LayoutSpec) -> Option<fresh_ui::Rect> {
    let key = LAYER_KEY.with(|k| k.clone());
    spec.index
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, r)| spec.items.get(r.start).map(|i| i.rect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fresh_ui::{Input, Mods, MouseButton, Point, Size, Ui};

    fn rows(n: usize) -> Vec<SuggestionRow> {
        (0..n)
            .map(|i| SuggestionRow {
                name: format!("command-{i}"),
                ..SuggestionRow::default()
            })
            .collect()
    }

    fn laid_out(s: Suggestions, w: u16, h: u16) -> Ui<UiMsg> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(suggestions(&s), Size::new(w, h));
        ui
    }

    /// **Every name is a real theme key.**
    ///
    /// `shell_theme::resolve` splits a name on `/`, sends each half through
    /// `Theme::resolve_theme_key`, and falls back to the editor's plain ground
    /// when either half is unknown — silently. An earlier draft of this module
    /// used five keys that exist nowhere (`ui.suggestion_selected_fg` among
    /// them); nothing failed, every row would simply have painted in the
    /// default colour. This is the only thing that catches that.
    #[test]
    fn every_theme_name_is_a_real_key() {
        use crate::view::theme::Theme;
        let theme = Theme::from_json(r#"{"name":"test"}"#).expect("defaults");
        for name in every_theme_name() {
            // `names` is the grammar's own reader: it drops the `+attrs` tail
            // and reports each half only when it is a name rather than a
            // `#rrggbb` literal. Nothing here should be a literal.
            let (fg, bg) = crate::app::shell_host::shell_theme::names(&name);
            for half in [fg, bg] {
                let half = half.unwrap_or_else(|| panic!("{name:?} has an unnamed half"));
                assert!(
                    theme.resolve_theme_key(half).is_some(),
                    "{half:?} (in {name:?}) is not a theme key"
                );
            }
        }
    }

    /// **Ledger rule 2: clicking a row selects it — by index, not by
    /// coordinate.** `handle_click_suggestions` hit-tested a recorded
    /// rectangle to recover an index the list already had.
    ///
    /// A press *and* a release, because that is what `fresh_ui::widgets::List`
    /// derives a click from — and the web frontend now sends both for a chrome
    /// control (`sendClick`). Asserting on the press alone would have passed
    /// only by changing the library to match a host bug.
    #[test]
    fn a_click_on_a_row_reports_that_row() {
        let mut ui = laid_out(
            Suggestions {
                rows: rows(5),
                selected: Some(0),
            },
            40,
            8,
        );
        let r = ui.rect_of(ui.find_by_key(&row_key(2)).expect("row 2"));
        let at = Point::new(r.x + 1, r.y);
        let mut msgs = ui
            .dispatch(Input::press(at, MouseButton::Left, Mods::NONE))
            .msgs;
        msgs.extend(
            ui.dispatch(Input::release(at, MouseButton::Left, Mods::NONE))
                .msgs,
        );
        assert!(
            msgs.iter()
                .any(|m| matches!(m, UiMsg::Ui(UiFact::SuggestionSelect(2)))),
            "got {msgs:?}"
        );
    }

    /// **Ledger rule 1: at most `MAX_VISIBLE_SUGGESTIONS` rows exist.** The
    /// painter kept a `scroll_offset` window by hand; `windowed` is the
    /// concept, and a list far longer than the viewport must not build a node
    /// per item.
    #[test]
    fn a_long_list_builds_only_the_rows_it_can_show() {
        let ui = laid_out(
            Suggestions {
                rows: rows(1000),
                selected: Some(0),
            },
            40,
            MAX_VISIBLE_SUGGESTIONS as u16,
        );
        let built = (0..1000)
            .filter(|i| ui.find_by_key(&row_key(*i)).is_some())
            .count();
        assert!(
            built <= MAX_VISIBLE_SUGGESTIONS + 2,
            "windowed list built {built} rows for a {}-row viewport",
            MAX_VISIBLE_SUGGESTIONS
        );
    }

    /// **Ledger rule 5: the list sits above the prompt row.**
    ///
    /// The painter measured the row count, subtracted it from the prompt
    /// line's `y`, and a second copy of that arithmetic in
    /// `chrome::Prompt::collect` had to agree for clicks to land. Here it is
    /// `Anchor::Node` + `Place::Above`, and this reads the answer back off the
    /// laid-out tree rather than off a recorded rectangle.
    #[test]
    fn the_list_is_placed_above_the_prompt_row() {
        use crate::view::shell::frame::{frame_tree, region_key, Frame, HostRegion};
        let mut ui: Ui<UiMsg> = Ui::new();
        let spec = ui
            .frame(
                frame_tree(Frame {
                    prompt_line: true,
                    suggestions: Some(Suggestions {
                        rows: rows(3),
                        selected: Some(0),
                    }),
                    ..Frame::default()
                }),
                Size::new(60, 20),
            )
            .clone();
        let list = suggestions_rect(&spec).expect("the list was placed");
        let prompt = ui.rect_of(ui.find_by_key(&region_key(HostRegion::PromptLine)).unwrap());
        assert!(
            list.bottom() <= prompt.y,
            "the list must sit above the prompt row: list {list:?}, prompt {prompt:?}"
        );
    }

    /// **A path keeps its filename; a command keeps its head.**
    ///
    /// `ColumnLayout::names_are_paths` read this off the shape of the list —
    /// neither keybindings nor sources means a file finder — and the two ends
    /// are not interchangeable: `truncate_head_ellipsis` exists so a long path
    /// still shows what file it is, and the tail form exists because "Toggle
    /// Compose/Preview (All Files)" contains a slash and is still a command
    /// name. That was the bug the rule was written for.
    #[test]
    fn a_path_gives_up_its_head_and_a_command_its_tail() {
        let painted = |r: SuggestionRow| {
            let s = Suggestions {
                rows: vec![r],
                selected: Some(0),
            };
            let ui = laid_out(s, 16, 4);
            let spec = ui.spec();
            let id = ui.find_by_key(&name_key(0)).expect("the name column");
            let rect = ui.rect_of(id);
            spec.items
                .iter()
                .find(|i| i.rect == rect && matches!(&i.draw, fresh_ui::Draw::Lines(_)))
                .and_then(|i| match &i.draw {
                    fresh_ui::Draw::Lines(l) => l.first().map(|s| s.to_string()),
                    _ => None,
                })
                .expect("the name painted")
        };
        // Neither keybinding nor source: a file finder. The filename survives.
        let path = painted(SuggestionRow {
            name: "src/view/shell/prompt.rs".into(),
            ..SuggestionRow::default()
        });
        assert!(
            path.ends_with("prompt.rs") && path.starts_with('…'),
            "a path must keep its filename, got {path:?}"
        );
        // A keybinding makes it a command palette. The head survives.
        let cmd = painted(SuggestionRow {
            name: "Toggle Compose/Preview (All Files)".into(),
            keybinding: Some("^P".into()),
            ..SuggestionRow::default()
        });
        assert!(
            cmd.starts_with("Toggle") && cmd.ends_with('…'),
            "a command name must keep its head, got {cmd:?}"
        );
    }

    /// **A plugin's styled description keeps its pieces, and inherits the
    /// rest of the row.**
    ///
    /// `styled_span_style` started from the row's style and set only what the
    /// span mentioned, so a span naming a foreground still sat on the
    /// selection's background. It also resolved the plugin's colour to a
    /// concrete `Color` on the way, which the theme inspector could not
    /// explain afterwards; a `ThemeKey` spec now stays a key.
    #[test]
    fn a_styled_span_names_only_what_it_overrides() {
        let s = Suggestions {
            rows: vec![SuggestionRow {
                name: "cmd".into(),
                description_spans: Some(vec![
                    DescriptionSpan {
                        text: "hit".into(),
                        fg: Some("editor.warning_fg".into()),
                        attrs: vec!["bold"],
                        ..DescriptionSpan::default()
                    },
                    DescriptionSpan {
                        text: " rest".into(),
                        ..DescriptionSpan::default()
                    },
                ]),
                ..SuggestionRow::default()
            }],
            selected: Some(0),
        };
        let ui = laid_out(s, 40, 4);
        let themes: Vec<(String, String)> = ui
            .spec()
            .items
            .iter()
            .filter_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) => l
                    .first()
                    .map(|t| (t.to_string(), i.theme.as_str().to_string())),
                _ => None,
            })
            .collect();
        let of = |needle: &str| {
            themes
                .iter()
                .find(|(t, _)| t.contains(needle))
                .map(|(_, k)| k.clone())
                .unwrap_or_else(|| panic!("no run for {needle:?} in {themes:?}"))
        };
        // The span's own foreground, the row's background, the row's selected
        // state — all three, from one name.
        assert_eq!(
            of("hit"),
            "editor.warning_fg/ui.suggestion_selected_bg+bold",
            "a span keeps the row under it"
        );
        // A span that overrides nothing is the row.
        assert_eq!(of("rest"), theme(false, RowState::Selected));
    }

    /// **Ledger finding A: the column yield order.** The name is sized before
    /// the description, so a row too narrow for both keeps the whole command
    /// name and truncates the description — never the other way round. This is
    /// what `left_budget` says for the status bar and what `Node::priority`
    /// replaced for both.
    #[test]
    fn the_description_yields_before_the_name() {
        let one = |w: u16| {
            let s = Suggestions {
                rows: vec![SuggestionRow {
                    name: "a-long-command-name".into(),
                    description: Some("a-long-description".into()),
                    ..SuggestionRow::default()
                }],
                selected: Some(0),
            };
            let ui = laid_out(s, w, 4);
            ui.rect_of(ui.find_by_key(&name_key(0)).expect("the name column"))
                .w
        };
        // Wide enough for both: the name is whole.
        assert_eq!(one(60), 19, "the name fits at its natural width");
        // Too narrow for both: the name is still whole — the description gave
        // up its cells first.
        assert_eq!(one(24), 19, "the name kept its width; the description paid");
    }
}
