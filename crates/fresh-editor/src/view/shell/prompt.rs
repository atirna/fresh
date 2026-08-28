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
use fresh_ui::{col, row, text, Key, Node, Sizing};

use crate::app::shell_host::shell_theme::pair;

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

/// One row of the list, as content. No geometry: the columns are placed by
/// layout, and which of them survives a narrow row is `priority`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SuggestionRow {
    pub name: String,
    pub keybinding: Option<String>,
    pub description: Option<String>,
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
        return pair("editor.line_number_fg", bg);
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
            out.push(column(disabled, st, "editor.line_number_fg"));
        }
    }
    out
}

/// One row's four columns, in paint order, each carrying the priority that says
/// when it yields.
fn node_row(index: usize, r: &SuggestionRow, st: RowState) -> Node<UiMsg> {
    let t = theme(r.disabled, st);
    let mut cells: Vec<Node<UiMsg>> = vec![text(r.name.clone())
        .theme(t.clone())
        .key(name_key(index))
        .priority(yields_last::NAME)];

    // A flexible gap rather than padding: it is what puts the trailing columns
    // at the right edge, and `min_w` keeps one cell of air when the row is
    // tight — the same floor the explorer's rows use.
    cells.push(row().flex(1).min_w(1));

    // The description carries no colour of its own: the painter draws it in
    // `base_style`, so it is the row.
    if let Some(d) = &r.description {
        cells.push(
            text(d.clone())
                .theme(t.clone())
                .priority(yields_last::DESCRIPTION),
        );
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
                .theme(column(r.disabled, st, "editor.line_number_fg"))
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
            for half in name.split('/') {
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
