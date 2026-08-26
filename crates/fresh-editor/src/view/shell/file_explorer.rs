//! The file explorer sidebar as a description.
//!
//! The biggest surface migrated so far, and the first with real *content*
//! rather than a row of controls: a bordered panel, a title that doubles as a
//! search box, one row per visible tree node, and a status slot on the right
//! of each row whose position nobody was able to state without computing it
//! twice.
//!
//! # What the tree measures
//!
//! Each row is `row([ left_runs, gap.flex(1), trailing?, error? ])`. The gap is
//! a flex spacer, so the trailing slot is pushed to the right edge by *layout*.
//! That deletes `FileExplorerRenderer::trailing_slot_screen_bounds` — 45 lines
//! that re-derived the slot's column from the indicator width, the leading
//! slot's width, the compact chain's width, the name's width and the padding
//! rule, purely so a hover could find it. The slot is a keyed node now, and its
//! rectangle is read back with [`slot_rect`].
//!
//! # What it does not measure
//!
//! **Which rows are visible.** `FileTreeView::viewport_display_indices()`
//! already windows the tree — including its sticky-ancestor rows — and the
//! scroll offset is app state that survives rebuilds. Handing the tree a
//! million-row list and a `Viewport` would be the wrong trade here: the
//! windowing is a model concern (which ancestors are sticky, what the search
//! filter admits), not a layout one.
//!
//! **The drag itself.** The panel's rightmost column is a native grip — it
//! answers its own press — but what that press starts is still the legacy
//! drag: `mouse_state.dragging_file_explorer`, motion routed by
//! `chrome::pointer_grab`, and `handle_file_explorer_border_drag` doing the
//! arithmetic. Pointer capture replaces all three, and that is its own change.
//! The grip is here rather than on a chrome box because it *can* be: an
//! overlay strip whose spacers pass presses through is exactly what
//! `pointer_mode` on an ordinary container made expressible.
//!
//! # Colour
//!
//! Every colour here is a real theme key except two: `ExplorerSlot`'s `fg` and
//! the name-colour hint, which arrive already resolved to a `Color` because
//! `resolve_overlay_color` collapses a plugin's `OverlayColorSpec` long before
//! a description exists. Those are written as `#rrggbb` literals — see
//! [`crate::app::shell_host::shell_theme`], which documents the literal as an
//! interim and names what replaces it.

use std::rc::Rc;

use fresh_ui::{
    col, gesture, row, stack, text, text_runs, Event, GestureKind, Key, Node, PointerMode, Run,
    Sizing,
};

use crate::app::shell_host::shell_theme::{attrs, pair};
use crate::app::types::HoverTarget;

use super::msg::{UiFact, UiMsg};

/// A `(text, theme name)` pair — the same shape the menu bar's labels use.
pub type Runs = Vec<(String, String)>;

/// One visible row of the tree.
///
/// `index` is the **viewport** index, which is what
/// `FileTreeView::get_display_node_at_viewport_row` takes — so a row's key,
/// its hit answer and the model's lookup are all the same number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub index: usize,
    /// The row's own ground: selection, multi-selection or the panel's.
    pub theme: String,
    /// Indicator, leading slot, compact chain and name, in order.
    pub left: Runs,
    /// The status slot pushed to the right edge, if the providers gave one.
    pub trailing: Option<Slot>,
    /// `" [Error]"` for a node that failed to load.
    pub error: Option<(String, String)>,
}

/// A row's trailing status slot: what it says, how it looks, and which path's
/// tooltip it opens.
///
/// The path travels with the slot because the *slot* is what the pointer
/// enters — the old walk had to find the row, then re-derive the slot's
/// columns, then look the node up again to get the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub text: String,
    pub theme: String,
    pub path: std::path::PathBuf,
}

/// What fills the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// The tree is still being built (initial async build, or expand-to-path).
    /// The panel's chrome is already final — that is the point of this state,
    /// so a slow remote build never paints the window in two stages.
    Loading(String),
    Rows(Vec<Row>),
}

/// The sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explorer {
    /// Width in columns, already resolved against the frame.
    pub cols: u16,
    pub on_left: bool,
    /// `" File Explorer (Ctrl+E) "`, `" [host] "`, or `" /query "` while the
    /// incremental search is open.
    pub title: String,
    pub title_theme: String,
    pub border_theme: String,
    pub close_theme: String,
    pub body: Body,
    /// The viewport row the caret sits on, when the panel owns the keyboard.
    pub caret_row: Option<usize>,
}

impl Explorer {
    /// The panel's ground — the background every row and the border sit on.
    pub fn panel() -> String {
        pair("editor.fg", "editor.bg")
    }
}

/// The keys the readers below look elements up by.
pub fn row_key(index: usize) -> Key {
    Key::Pair("explorer_row".into(), index as u64)
}

pub fn slot_key(index: usize) -> Key {
    Key::Pair("explorer_slot".into(), index as u64)
}

pub fn close_key() -> Key {
    Key::Str("explorer_close".into())
}

pub fn grip_key() -> Key {
    Key::Str("explorer_grip".into())
}

fn hover_msg(t: Option<HoverTarget>) -> fresh_ui::Handler<UiMsg> {
    Rc::new(move |_: &Event| Some(UiMsg::Ui(UiFact::Hover(t.clone()))))
}

fn runs_of(runs: &Runs) -> Vec<Run> {
    runs.iter()
        .map(|(t, theme)| Run::themed(t.clone(), theme))
        .collect()
}

/// The sidebar as a description.
///
/// A `stack` of two: the bordered panel with its rows, and the title strip that
/// sits *on* the top border line — which is where a ratatui `Block` draws its
/// title too. The strip is one cell high, so it covers the border row and
/// nothing else; the rows below it stay reachable by the pointer.
pub fn explorer(e: &Explorer) -> Node<UiMsg> {
    stack().children([panel(e), overlay(e)])
}

/// Everything drawn *on* the panel's border: the title, the close button and
/// the resize grip.
///
/// It covers the whole panel, so every part of it that is not a control says
/// it is not a pointer target — otherwise the strip swallows every click on
/// the rows beneath. That is one attribute per container rather than a
/// rectangle each control has to be hit-tested against by hand.
fn overlay(e: &Explorer) -> Node<UiMsg> {
    col()
        .pointer_mode(PointerMode::Transparent)
        .children([title_strip(e), grip_strip()])
}

/// The one-column drag handle on the panel's right edge, below the title line.
///
/// Below, because the title line's rightmost three cells are the close
/// button's — which is the precedence the old hover walk had (it tested the
/// close button first), and the opposite of the one its *click* walk had (it
/// tested the border first). The two disagreed: hovering the top-right corner
/// lit the close button while clicking it started a resize. They agree now,
/// and they agree because there is one description instead of two walks.
fn grip_strip() -> Node<UiMsg> {
    let grip = gesture(row().w(Sizing::Cells(1)))
        .key(grip_key())
        .on(
            GestureKind::Press,
            Rc::new(|e: &Event| {
                if e.button != fresh_ui::MouseButton::Left {
                    return None;
                }
                e.stop();
                Some(UiMsg::Ui(UiFact::ExplorerResizeBegin {
                    x: e.pos.x.max(0) as u16,
                    y: e.pos.y.max(0) as u16,
                }))
            }),
        )
        .on_enter(hover_msg(Some(HoverTarget::FileExplorerBorder)))
        .on_leave(hover_msg(None));
    row()
        .flex(1)
        .pointer_mode(PointerMode::Transparent)
        .children([row().flex(1).pointer_mode(PointerMode::Transparent), grip])
}

fn panel(e: &Explorer) -> Node<UiMsg> {
    let mut b = col()
        .border()
        // Border ink over the panel's ground: the fill draws spaces, so only
        // this key's background reaches the eye inside the box.
        .theme(e.border_theme.clone());
    b = match &e.body {
        Body::Loading(text_) => b.child(
            text(text_.clone())
                .theme(pair("editor.line_number_fg", "editor.bg"))
                .h(Sizing::Cells(1)),
        ),
        Body::Rows(rows) => b.children(rows.iter().map(|r| node_row(e, r))),
    };
    b
}

fn node_row(e: &Explorer, r: &Row) -> Node<UiMsg> {
    let mut children: Vec<Node<UiMsg>> = vec![
        text_runs(runs_of(&r.left)),
        // **The padding rule, as layout.** The old walk computed
        // `content_width - left_side_width - total_right_width` and a second
        // function computed it again to find the slot; a flex spacer states it
        // once and both the cells and the rectangle come out of it — including
        // the `min_gap = 1` floor, which is `min_w` rather than a `max()` in
        // two places.
        row().flex(1).min_w(1),
    ];
    if let Some(slot) = &r.trailing {
        let path = slot.path.clone();
        children.push(
            gesture(text(slot.text.clone()).theme(slot.theme.clone()))
                // Keyed so a caller can ask layout where the slot ended up
                // rather than re-deriving the column.
                .key(slot_key(r.index))
                // The slot answers its own hover, so the tooltip opens on the
                // cells that actually carry the status — no bounds function in
                // between. It does not claim: a press here still selects the
                // row, because the row's handler is up the same path.
                .on_enter(hover_msg(Some(HoverTarget::FileExplorerStatusIndicator(
                    path.clone(),
                ))))
                .on_leave(hover_msg(None)),
        );
    }
    if let Some((t, theme)) = &r.error {
        children.push(text(t.clone()).theme(theme.clone()));
    }
    let index = r.index;
    let caret = e.caret_row == Some(index);
    let body = row()
        .theme(r.theme.clone())
        .h(Sizing::Cells(1))
        .children(children);
    // The caret indicator the panel paints under the hardware cursor when it
    // owns the keyboard. It replaces the left-most cell of the row, which is
    // what the old `Paragraph::new("▌")` overwrote.
    let body = if caret {
        stack().h(Sizing::Cells(1)).children([
            body,
            row().h(Sizing::Cells(1)).children([text("▌")
                .theme(pair("editor.cursor", "editor.bg"))
                .w(Sizing::Cells(1))]),
        ])
    } else {
        body
    };
    gesture(body)
        .key(row_key(index))
        // Left only, and it stops: the press selects and opens, which is what
        // the chrome component reported `Consumed` for. A right press is the
        // context menu's, and a modifier-less right press must still reach the
        // theme inspector's pre-band, so it is answered separately below.
        .on(
            GestureKind::Press,
            Rc::new(move |e: &Event| {
                if e.button != fresh_ui::MouseButton::Left {
                    return None;
                }
                e.stop();
                Some(UiMsg::Ui(UiFact::ExplorerRowPress {
                    index,
                    clicks: e.clicks,
                }))
            }),
        )
        // The context menu opens on the **press**, which is when
        // `MouseEventKind::Down(Right)` opened it before.
        //
        // Except with Ctrl held. Ctrl+Right-click is the theme inspector's
        // gesture, and the inspector rides the very top of the legacy bands
        // precisely so it can be reached under any surface — but the tree now
        // runs *before* those bands, so "above everything" has to be said here,
        // by declining, instead of by rank. Declining is also not claiming, so
        // the press travels on untouched.
        .on(
            GestureKind::Press,
            Rc::new(move |e: &Event| {
                if e.button != fresh_ui::MouseButton::Right || e.mods.ctrl {
                    return None;
                }
                e.stop();
                Some(UiMsg::Ui(UiFact::ExplorerRowContext {
                    index,
                    x: e.pos.x.max(0) as u16,
                    y: e.pos.y.max(0) as u16,
                }))
            }),
        )
        // The panel scrolls its own viewport — the surface's wheel, with the
        // surface. `stop()` claims it, as the component's `Consumed` did.
        .on(
            GestureKind::Wheel,
            Rc::new(move |e: &Event| {
                e.stop();
                Some(UiMsg::Ui(UiFact::ExplorerScroll {
                    delta: e.delta,
                    x: e.pos.x.max(0) as u16,
                    y: e.pos.y.max(0) as u16,
                }))
            }),
        )
}

/// The title line: the title text at the left of the top border, and the close
/// button's three cells at its right.
fn title_strip(e: &Explorer) -> Node<UiMsg> {
    let close = gesture(
        text("×")
            .theme(e.close_theme.clone())
            // Three cells, matching the region the old hit test claimed
            // (`close_button_x .. area.x + width`); the glyph is drawn at the
            // first of them, exactly where `render_close_button` put it.
            .w(Sizing::Cells(3)),
    )
    .key(close_key())
    .on(
        GestureKind::Press,
        Rc::new(|ev: &Event| {
            if ev.button != fresh_ui::MouseButton::Left {
                return None;
            }
            ev.stop();
            Some(UiMsg::Ui(UiFact::ExplorerClose))
        }),
    )
    .on_enter(hover_msg(Some(HoverTarget::FileExplorerCloseButton)))
    .on_leave(hover_msg(None));
    let cells: Vec<Node<UiMsg>> = vec![
        // One cell of border before the title, which is where ratatui's
        // `Block` starts a left-aligned title.
        row()
            .w(Sizing::Cells(1))
            .pointer_mode(PointerMode::Transparent),
        text(e.title.clone())
            .theme(e.title_theme.clone())
            // The title is decoration. Pressing it used to select the panel's
            // first row — `row.saturating_sub(area.y + 1)` clamps to 0 on the
            // title line — while the right-click and double-click paths both
            // guarded the row out explicitly. Saying it is not a target makes
            // all three agree.
            .pointer_mode(PointerMode::Transparent),
        row().flex(1).pointer_mode(PointerMode::Transparent),
        close,
    ];
    row()
        .h(Sizing::Cells(1))
        .pointer_mode(PointerMode::Transparent)
        .children(cells)
}

// -- the styles, as names ----------------------------------------------------

/// Title and border for the panel chrome.
///
/// Same three cases `FileExplorerRenderer::panel_chrome_styles` had: a
/// disconnected remote shouts, a focused panel inverts its title and accents
/// its border, and a blurred one recedes.
pub fn chrome_themes(remote_disconnected: bool, focused: bool) -> (String, String) {
    if remote_disconnected {
        (
            attrs(
                "ui.status_error_indicator_fg",
                "ui.status_error_indicator_bg",
                &["bold"],
            ),
            pair("ui.status_error_indicator_bg", "editor.bg"),
        )
    } else if focused {
        (
            attrs("editor.bg", "editor.fg", &["bold"]),
            pair("editor.cursor", "editor.bg"),
        )
    } else {
        (
            pair("editor.line_number_fg", "editor.bg"),
            pair("ui.split_separator_fg", "editor.bg"),
        )
    }
}

/// The close button's own colour.
pub fn close_theme(hovered: bool) -> String {
    if hovered {
        pair("ui.tab_close_hover_fg", "editor.bg")
    } else {
        pair("editor.line_number_fg", "editor.bg")
    }
}

/// A row's ground.
///
/// The old painter said this twice — `ListItem::style` for the item and
/// `List::highlight_style` for the cursor row — and the two disagreed for a
/// blurred multi-selection. Stated once here, matching what the pair actually
/// produced on screen.
pub fn row_theme(is_cursor: bool, is_multi: bool, focused: bool) -> String {
    if is_cursor && focused {
        pair("editor.fg", "editor.selection_bg")
    } else if is_cursor {
        pair("editor.fg", "editor.current_line_bg")
    } else if is_multi && focused {
        pair("editor.fg", "editor.selection_bg")
    } else {
        Explorer::panel()
    }
}

/// The foreground a node's name takes when nothing overrides it: hidden files
/// recede, symlinks take the type colour, directories the keyword colour.
pub fn neutral_key(is_hidden: bool, is_symlink: bool, is_dir: bool) -> &'static str {
    if is_hidden {
        "editor.line_number_fg"
    } else if is_symlink {
        "syntax.type"
    } else if is_dir {
        "syntax.keyword"
    } else {
        "editor.fg"
    }
}

// -- reading the layout back -------------------------------------------------

fn rect_of(
    ui: &fresh_ui::Ui<UiMsg>,
    key: &Key,
    size: ratatui::layout::Rect,
) -> Option<ratatui::layout::Rect> {
    let e = ui.find_by_key(key)?;
    let r = ui.rect_of(e);
    (r.w > 0 && r.h > 0).then(|| ratatui::layout::Rect {
        x: size.x.saturating_add(r.x.max(0) as u16),
        y: size.y.saturating_add(r.y.max(0) as u16),
        width: r.w,
        height: r.h,
    })
}

/// Where layout put a row.
pub fn row_rect(
    ui: &fresh_ui::Ui<UiMsg>,
    index: usize,
    size: ratatui::layout::Rect,
) -> Option<ratatui::layout::Rect> {
    rect_of(ui, &row_key(index), size)
}

/// Where layout put a row's trailing status slot.
///
/// This is the whole of what `trailing_slot_screen_bounds` computed, and the
/// reason that function could exist at all was that the padding rule lived in
/// two places. It lives in the flex spacer now.
pub fn slot_rect(
    ui: &fresh_ui::Ui<UiMsg>,
    index: usize,
    size: ratatui::layout::Rect,
) -> Option<ratatui::layout::Rect> {
    rect_of(ui, &slot_key(index), size)
}
