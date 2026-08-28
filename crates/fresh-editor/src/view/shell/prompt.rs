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

fn theme(disabled: bool, selected: bool) -> String {
    match (disabled, selected) {
        (true, _) => pair("ui.suggestion_disabled_fg", "ui.suggestion_bg"),
        (false, true) => pair("ui.suggestion_selected_fg", "ui.suggestion_selected_bg"),
        (false, false) => pair("ui.suggestion_fg", "ui.suggestion_bg"),
    }
}

/// One row's four columns, in paint order, each carrying the priority that says
/// when it yields.
fn node_row(index: usize, r: &SuggestionRow, selected: bool) -> Node<UiMsg> {
    let t = theme(r.disabled, selected);
    let mut cells: Vec<Node<UiMsg>> = vec![text(r.name.clone())
        .theme(t.clone())
        .key(name_key(index))
        .priority(yields_last::NAME)];

    // A flexible gap rather than padding: it is what puts the trailing columns
    // at the right edge, and `min_w` keeps one cell of air when the row is
    // tight — the same floor the explorer's rows use.
    cells.push(row().flex(1).min_w(1));

    if let Some(d) = &r.description {
        cells.push(
            text(d.clone())
                .theme(pair("ui.suggestion_description_fg", "ui.suggestion_bg"))
                .priority(yields_last::DESCRIPTION),
        );
    }
    if let Some(k) = &r.keybinding {
        cells.push(
            text(k.clone())
                .theme(pair("ui.suggestion_keybinding_fg", "ui.suggestion_bg"))
                .priority(yields_last::KEYBINDING),
        );
    }
    if let Some(s) = &r.source {
        cells.push(
            text(s.clone())
                .theme(pair("ui.suggestion_source_fg", "ui.suggestion_bg"))
                .priority(yields_last::SOURCE),
        );
    }

    row().h(Sizing::Cells(1)).theme(t).children(cells)
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

    let mut list = fresh_ui::widgets::List::windowed(
        rows_for_key.len(),
        move |i| row_key(i),
        move |i| match rows_for_row.get(i) {
            Some(r) => node_row(i, r, selected == Some(i)),
            None => row().h(Sizing::Cells(1)),
        },
    )
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
