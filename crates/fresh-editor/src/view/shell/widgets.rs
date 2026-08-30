//! Plugin widgets as descriptions — the first half of C.1.
//!
//! `crates/fresh-editor/src/widgets/` is a complete widget runtime: seventeen
//! thousand lines that lay a `WidgetSpec` out, paint it into
//! `TextPropertyEntry` rows, record a `HitArea` per interactive range and a
//! `LayoutBox` arena beside it, and hit-test a click by scanning byte ranges.
//! It is the largest thing this migration has left, and goal 5 — one source of
//! geometry — is what it is in tension with.
//!
//! **What moves and what does not.** The runtime's *formatting* is domain
//! knowledge and stays: `render_hint_bar` knows what a hint row looks like,
//! `Raw` is entries the plugin wrote, a `List`'s items arrive pre-rendered.
//! What moves is layout, paint and hit — the three things the tree does. So a
//! variant's migration is usually "call the same formatter, carry its row as
//! runs" rather than a rewrite, which is why this is far less than seventeen
//! thousand lines of new code.
//!
//! **How it is checked.** Every variant here is asserted equal to
//! `widgets::render_spec`'s own answer, over the shapes that runtime branches
//! on — the same arrangement that made the split separators a safe swap
//! (`the_dividers_are_where_the_separators_are`). The runtime is the oracle
//! while it is still the implementation, so a variant cannot be migrated
//! wrongly without a red test, and the oracle goes when the last variant does.
//!
//! **Coverage is explicit** ([`covered`]) because a panel is either described
//! or painted, never half of each: a spec using a variant this module has not
//! reached yet takes the old path whole. That is the same seam as a `Host`
//! leaf, and it is temporary in the same way.

use std::borrow::Cow;

use fresh_core::api::{OverlayColorSpec, OverlayOptions, WidgetSpec};
use fresh_core::text_property::TextPropertyEntry;
use fresh_ui::{col, row, text_runs, Node, Run, Sizing};

use crate::app::shell_host::shell_theme::{Attrs, Ink, Paint};

use super::msg::UiMsg;

/// The panel surface's own colours, which every row starts from.
const BASE_FG: &str = "ui.suggestion_fg";
const BASE_BG: &str = "ui.suggestion_bg";

/// Which panel a description belongs to.
///
/// The view layer's own spelling of `app::PanelSlot`, mirrored the way
/// `modal::Slot` is, so a description carries no app types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Dock,
    Floating,
}

/// What a panel's widgets need beyond their spec.
///
/// All of it is host state the runtime read off a `RenderContext`: which
/// widget has the panel's focus, which one the pointer is on, whether the
/// focus-marker gutter is reserved. Passed down rather than looked up,
/// because a description is a pure function of what it is handed.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub slot: Slot,
    /// The panel's focused widget key, or empty.
    pub focus_key: String,
    /// The widget key the pointer is over, if any.
    pub hovered_key: Option<String>,
    /// Whether focusable controls reserve the `▸ ` gutter.
    pub marker_gutter: bool,
}

impl Ctx {
    fn is_focused(&self, key: Option<&str>) -> bool {
        key.is_some_and(|k| !k.is_empty() && k == self.focus_key)
    }

    fn is_hovered(&self, key: Option<&str>) -> bool {
        match (key, self.hovered_key.as_deref()) {
            (Some(k), Some(h)) => !k.is_empty() && k == h,
            _ => false,
        }
    }
}

/// Whether every node of this spec is a variant this module describes.
///
/// A panel is described or painted, never half of each — a `Row` of migrated
/// children with one unmigrated child among them has nothing sensible to be.
/// So the whole tree is asked, and the answer gates the panel.
pub fn covered(spec: &WidgetSpec) -> bool {
    match spec {
        WidgetSpec::Row { children, .. } | WidgetSpec::Col { children, .. } => {
            children.iter().all(covered)
        }
        WidgetSpec::LabeledSection { child, .. } => covered(child),
        WidgetSpec::Button { .. } | WidgetSpec::Toggle { .. } => true,
        WidgetSpec::Spacer { .. }
        | WidgetSpec::Divider { .. }
        | WidgetSpec::HintBar { .. }
        | WidgetSpec::Raw { .. } => true,
        _ => false,
    }
}

/// The description for a covered spec.
///
/// `width` is the panel's inner content width, which two variants need before
/// layout can run: a `Divider` is as wide as the panel by definition, and the
/// runtime pads rows to it. Passing it in rather than reading it back is the
/// rule §4.4 states — this is *content* resolved from a known extent, not
/// geometry recorded from a paint.
pub fn node(spec: &WidgetSpec, width: u16, cx: &Ctx) -> Node<UiMsg> {
    match spec {
        WidgetSpec::Row { children, wrap, .. } => {
            let r = row().children(
                children
                    .iter()
                    .map(|c| node(c, width, cx))
                    .collect::<Vec<_>>(),
            );
            match wrap {
                true => r.wrap_children(),
                false => r,
            }
        }
        WidgetSpec::Col { children, .. } => col().children(
            children
                .iter()
                .map(|c| node(c, width, cx))
                .collect::<Vec<_>>(),
        ),
        // `flex` fills the row's remainder; `cols` is a fixed gap. The runtime
        // spells the first one by handing the row a width to divide, which is
        // what `Sizing::Flex` is.
        WidgetSpec::Spacer { cols, flex, .. } => match flex {
            true => row().flex(1),
            false => row().w(Sizing::Cells(*cols as u16)),
        },
        // Full width by definition — "so the separator always matches the
        // rendered width, including a user-dragged dock, without the plugin
        // computing the width itself".
        WidgetSpec::Divider { ch, style, .. } => {
            let glyph = match ch.is_empty() {
                true => "─",
                false => ch.as_str(),
            };
            let n = width as usize / glyph.chars().count().max(1);
            let ink = match style {
                Some(o) => ink_of(o, &Ink::new(Paint::key(BASE_FG), Paint::key(BASE_BG))),
                None => Ink::new(Paint::key(BASE_FG), Paint::key(BASE_BG)),
            };
            text_runs([Run::themed(glyph.repeat(n), ink.to_string())]).h(Sizing::Cells(1))
        }
        // The formatter is the runtime's own: what a hint row *says* is domain
        // knowledge and does not move.
        WidgetSpec::HintBar { entries, .. } => entry_row(&crate::widgets::render_hint_bar(entries)),
        // Entries the plugin wrote, inlined without interpretation. That is
        // the variant's whole contract, and it is one row per entry.
        WidgetSpec::Raw { entries, .. } => {
            col().children(entries.iter().map(entry_row).collect::<Vec<_>>())
        }
        // **A hit that is not the whole row.** Form layout (`label: [v]`)
        // restricts the press to the chip so a click on the label does not
        // flip the value — the settings dialog's long-standing contract — and
        // the runtime said that with a pair of byte offsets it compared a
        // clicked byte against. `entry_row_hit` splits the row there instead,
        // so the restriction is where the nodes are.
        WidgetSpec::Toggle {
            checked,
            label,
            focused,
            indeterminate,
            label_first,
            label_width,
            key,
        } => {
            let key = key.as_deref();
            let is_focused = match key.is_some_and(|k| !k.is_empty()) {
                true => cx.is_focused(key),
                false => *focused,
            };
            let (mut entry, chip) = match label_first {
                true => crate::widgets::render_toggle_form(
                    *checked,
                    *indeterminate,
                    label,
                    is_focused,
                    *label_width,
                    width as u32,
                    cx.marker_gutter,
                ),
                false => {
                    let e = crate::widgets::render_toggle(
                        *checked,
                        label,
                        is_focused,
                        cx.marker_gutter,
                    );
                    let end = e.text.len();
                    (e, (0, end))
                }
            };
            // The pointer lights the whole chip and label the way it lights a
            // button. Focus paints its own band, so hover only shows where
            // focus is not.
            if cx.is_hovered(key) && !is_focused {
                crate::widgets::apply_hover_band(&mut entry);
            }
            entry_row_hit(
                &entry,
                chip,
                cx.slot,
                crate::widgets::HitArea {
                    row_target: false,
                    context_click: false,
                    overlay: false,
                    widget_key: key.unwrap_or("").to_string(),
                    widget_kind: "toggle",
                    buffer_row: 0,
                    byte_start: chip.0,
                    byte_end: chip.1,
                    payload: serde_json::json!({ "checked": !checked }),
                    event_type: "toggle",
                    owner_key: None,
                },
            )
        }
        // **The first interactive variant, and the seam the rest ride.**
        //
        // Its *text* is the runtime's own — `render_button` and
        // `render_bare_button` know what a framed action looks like, and that
        // is domain knowledge. What moves is the hit: the runtime recorded a
        // `HitArea` spanning the row's bytes and a click was resolved by
        // scanning those ranges; the node carries the same `HitArea` and hands
        // it over when it is pressed, so everything downstream —
        // `deliver_widget_hit`, the kind's `on_pointer`, the plugin's
        // `widget_event` — is untouched. The byte range stops being a
        // hit-test and becomes what it always was: a payload.
        //
        // A disabled button has no hit at all, matching the runtime: the
        // renderer excludes it from the tab cycle, so a click that focused and
        // activated it would be acting on a stale focus.
        WidgetSpec::Button {
            label,
            focused,
            intent,
            key,
            disabled,
            bare,
            full_width,
            hover_style,
            ..
        } => {
            let key = key.as_deref();
            let is_focused = !disabled
                && match key.is_some_and(|k| !k.is_empty()) {
                    true => cx.is_focused(key),
                    false => *focused,
                };
            // A `hover_style` applies only while the pointer is on this
            // widget, and never to a disabled one — an inert control
            // advertising itself as live would lie.
            let hovered = !disabled && cx.is_hovered(key);
            let hover = hover_style.as_ref().filter(|_| hovered);
            // Stretched by padding the *label*, before the chrome goes on, so
            // the finished control is exactly `width` columns and the focus
            // band spans the row rather than hugging the word.
            let filled = full_width.then(|| {
                crate::widgets::fill_button_label(label, *bare, cx.marker_gutter, width as u32)
            });
            let label = filled.as_deref().unwrap_or(label);
            let entry = match bare {
                true => crate::widgets::render_bare_button(
                    label, is_focused, *intent, *disabled, hover, hovered,
                ),
                false => crate::widgets::render_button(
                    label,
                    is_focused,
                    *intent,
                    *disabled,
                    cx.marker_gutter,
                    hover,
                    hovered,
                ),
            };
            let n = entry_row(&entry);
            match disabled {
                true => n,
                false => hit_node(
                    n,
                    cx.slot,
                    crate::widgets::HitArea {
                        row_target: false,
                        context_click: false,
                        overlay: false,
                        widget_key: key.unwrap_or("").to_string(),
                        widget_kind: "button",
                        buffer_row: 0,
                        byte_start: 0,
                        byte_end: entry.text.len(),
                        payload: serde_json::json!({}),
                        event_type: "activate",
                        owner_key: None,
                    },
                ),
            }
        }
        // **A border, drawn as a border.** The runtime draws this one as
        // *text*: `render_section_top_border` writes `╭─ label ─…─╮` into an
        // entry and `wrap_in_side_border` wraps every child row in `│ … │`,
        // because entries are all it has. So this is the first variant whose
        // migration is not cell-for-cell — the tree has a border, and using it
        // is the point.
        //
        // What is preserved exactly is the *geometry*, which is what anything
        // downstream depends on: one column of ring plus one of padding on
        // each side, so the child gets `panel_width - 4`, offset a row down
        // and two columns in. That is `inner_width` and `shift_channels`'
        // translation, stated as layout instead of as an arithmetic shift
        // applied to six recorded channels.
        WidgetSpec::LabeledSection { label, child, .. } => {
            let ring = Ink::new(Paint::key(BASE_FG), Paint::key(BASE_BG)).to_string();
            let framed = col().theme(ring.clone()).border().pad(1, 0).child(node(
                child,
                width.saturating_sub(4).max(1),
                cx,
            ));
            match label.is_empty() {
                true => framed,
                // The legend rides the top edge, the way every other titled
                // frame in the shell does it — a transparent strip stacked
                // over the box rather than text spliced into the ring.
                false => fresh_ui::stack().children([
                    framed,
                    col()
                        .pointer_mode(fresh_ui::PointerMode::Transparent)
                        .children([
                            row().h(Sizing::Cells(1)).children([
                                row().w(Sizing::Cells(2)),
                                text_runs([Run::themed(format!(" {label} "), ring)]),
                            ]),
                            row().flex(1),
                        ]),
                ]),
            }
        }
        // `covered` gates this; reaching it is a bug in the caller rather than
        // a spec the plugin got wrong, so it is loud in debug and empty in
        // release rather than silently dropping a panel's content.
        other => {
            debug_assert!(false, "widget variant not covered: {other:?}");
            row().h(Sizing::Cells(0))
        }
    }
}

/// Wrap a widget's node so a press on it delivers the widget's own hit.
///
/// This is what replaces the byte-range scan. `deliver_widget_hit` — the
/// dispatch all three frontends share — takes a `HitArea` and does the rest:
/// focus the owner, run the kind's `on_pointer`, fire the plugin's event. It
/// does not change; what changes is that the tree *finds* the widget, by
/// hit-testing a rectangle it laid out, instead of the host reconstructing it
/// from a row and a byte offset.
fn hit_node(n: Node<UiMsg>, slot: Slot, hit: crate::widgets::HitArea) -> Node<UiMsg> {
    fresh_ui::gesture(n).on(
        fresh_ui::GestureKind::Press,
        std::rc::Rc::new(move |e: &fresh_ui::Event| {
            if e.button != fresh_ui::MouseButton::Left {
                return None;
            }
            e.stop();
            Some(UiMsg::Ui(super::msg::UiFact::WidgetHit {
                slot,
                hit: hit.clone(),
            }))
        }),
    )
}

/// One styled row, from a `TextPropertyEntry`.
///
/// **The load-bearing helper**: most variants of the runtime end in an entry,
/// so most of them migrate through here. It is the span walk
/// `render_widget_entry_line` does — split at inline-overlay boundaries, merge
/// overlapping overlays per property in declaration order — with the theme
/// *names* kept instead of resolved colours, because the fold resolves them
/// and that is what makes the row inspectable and the web able to paint it.
pub fn entry_row(entry: &TextPropertyEntry) -> Node<UiMsg> {
    text_runs(entry_runs(entry, &[]).into_iter().map(|(_, r)| r)).h(Sizing::Cells(1))
}

/// One styled row whose `range` of bytes answers a press with `hit`.
///
/// **A byte range becomes a rectangle here.** The runtime kept the range and
/// compared a clicked byte against it; a toggle in form layout (`label: [v]`)
/// restricts its hit to the chip so clicking the label does not flip the
/// value, and that restriction was a pair of byte offsets. The row is split at
/// those offsets into up to three pieces and the middle one is the gesture, so
/// the same rule is expressed as where the nodes are.
pub fn entry_row_hit(
    entry: &TextPropertyEntry,
    range: (usize, usize),
    slot: Slot,
    hit: crate::widgets::HitArea,
) -> Node<UiMsg> {
    let runs = entry_runs(entry, &[range.0, range.1]);
    let mut before: Vec<Run> = Vec::new();
    let mut inside: Vec<Run> = Vec::new();
    let mut after: Vec<Run> = Vec::new();
    for (at, run) in runs {
        match () {
            _ if at.end <= range.0 => before.push(run),
            _ if at.start >= range.1 => after.push(run),
            _ => inside.push(run),
        }
    }
    let piece = |rs: Vec<Run>| text_runs(rs).h(Sizing::Cells(1));
    let mut kids: Vec<Node<UiMsg>> = Vec::new();
    if !before.is_empty() {
        kids.push(piece(before));
    }
    kids.push(hit_node(piece(inside), slot, hit));
    if !after.is_empty() {
        kids.push(piece(after));
    }
    row().h(Sizing::Cells(1)).children(kids)
}

/// The styled pieces of an entry, each with the byte range it covers.
///
/// **The load-bearing helper**: most variants of the runtime end in an entry,
/// so most of them migrate through here. It is the span walk
/// `render_widget_entry_line` does — split at inline-overlay boundaries, snap
/// each to a grapheme cluster, merge overlapping overlays per property in
/// declaration order so a later one can set `bg` without wiping an earlier
/// one's italic — with the theme **names** kept rather than resolved to
/// colours, because the fold resolves them and that is what makes the row
/// inspectable and lets the web paint it.
///
/// `extra` are additional byte offsets to split at, for a caller that needs a
/// piece boundary the overlays do not provide.
fn entry_runs(entry: &TextPropertyEntry, extra: &[usize]) -> Vec<(std::ops::Range<usize>, Run)> {
    let mut normalized = entry.clone();
    normalized.normalize_widths();
    let mut text = normalized.text.clone();
    while text.ends_with('\n') {
        text.pop();
    }

    let base = match normalized.style.as_ref() {
        Some(o) => ink_of(o, &Ink::new(Paint::key(BASE_FG), Paint::key(BASE_BG))),
        None => Ink::new(Paint::key(BASE_FG), Paint::key(BASE_BG)),
    };

    if text.is_empty() {
        return vec![(0..0, Run::themed("", base.to_string()))];
    }

    // Snap every boundary to a grapheme cluster. An overlay offset can land
    // mid-codepoint after a row is truncated with a multi-byte `…` — the
    // overlay's end is not re-clamped to the new text — and slicing there
    // panics. The runtime floors to the previous boundary; so does this.
    let snap = |i: usize| {
        let i = i.min(text.len());
        match text.is_char_boundary(i) {
            true => i,
            false => crate::primitives::grapheme::prev_grapheme_boundary(&text, i),
        }
    };
    let bounds: Vec<usize> = std::iter::once(0)
        .chain(std::iter::once(text.len()))
        .chain(extra.iter().map(|i| snap(*i)))
        .chain(
            normalized
                .inline_overlays
                .iter()
                .flat_map(|o| [snap(o.start), snap(o.end)]),
        )
        .collect::<std::collections::BTreeSet<usize>>()
        .into_iter()
        .collect();

    let mut out: Vec<(std::ops::Range<usize>, Run)> = Vec::with_capacity(bounds.len());
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        // Merge, do not replace: a later overlay overrides individual
        // properties without wiping the earlier one's others. The text-input
        // renderer relies on it — a placeholder sets fg + italic and the
        // focused overlay sets bg only, and replacing would clear the italic.
        let mut ink = base.clone();
        for o in &normalized.inline_overlays {
            let (os, oe) = (o.start.min(text.len()), o.end.min(text.len()));
            if a >= os && b <= oe && oe > os {
                ink = ink_of(&o.style, &ink);
            }
        }
        out.push((a..b, Run::themed(&text[a..b], ink.to_string())));
    }
    out
}

/// Apply an overlay's properties over an existing ink.
///
/// A colour the overlay does not set is inherited, which is the merge the
/// painter does. A `ThemeKey` stays a name; an `Rgb` becomes a literal, which
/// is the one thing in the display list with no theme entry behind it and is
/// honest about that (F.2).
fn ink_of(o: &OverlayOptions, under: &Ink) -> Ink {
    let paint = |c: &OverlayColorSpec| match c {
        OverlayColorSpec::ThemeKey(k) => Paint::key(Cow::Owned(k.clone())),
        OverlayColorSpec::Rgb(r, g, b) => Paint::Lit(ratatui::style::Color::Rgb(*r, *g, *b)),
    };
    let mut attrs = under.attrs;
    for (on, a) in [
        (o.bold, Attrs::BOLD),
        (o.italic, Attrs::ITALIC),
        (o.underline, Attrs::UNDERLINE),
        (o.strikethrough, Attrs::STRIKETHROUGH),
    ] {
        if on {
            attrs = attrs | a;
        }
    }
    Ink {
        fg: o.fg.as_ref().map(paint).unwrap_or_else(|| under.fg.clone()),
        bg: o.bg.as_ref().map(paint).unwrap_or_else(|| under.bg.clone()),
        attrs,
    }
}

#[cfg(test)]
mod tests {
    use super::super::msg::UiFact;
    use super::*;
    use fresh_core::api::HintEntry;
    use fresh_ui::{Size, Ui};

    const WIDTH: u16 = 40;

    fn cx() -> Ctx {
        Ctx {
            slot: Slot::Floating,
            focus_key: String::new(),
            hovered_key: None,
            marker_gutter: false,
        }
    }

    /// What the runtime says this spec renders as: one string per row, with
    /// the trailing newlines its entries carry stripped.
    ///
    /// This is the oracle. It is the implementation still, which is what makes
    /// it worth asserting against: a variant cannot be migrated wrongly here
    /// without the two disagreeing.
    fn runtime_rows(spec: &WidgetSpec) -> Vec<String> {
        let out = crate::widgets::render_spec(spec, &Default::default(), "", WIDTH as u32);
        out.entries
            .iter()
            .map(|e| {
                let mut n = e.clone();
                n.normalize_widths();
                n.text.trim_end_matches('\n').to_string()
            })
            .collect()
    }

    /// What the tree says, laid out at the same width: the text of each row of
    /// the display list, in paint order.
    fn tree_rows(spec: &WidgetSpec) -> Vec<String> {
        tree_text(spec, &cx())
    }

    fn hint(keys: &str, label: &str) -> HintEntry {
        HintEntry {
            keys: keys.into(),
            label: label.into(),
        }
    }

    fn raw(text: &str) -> TextPropertyEntry {
        TextPropertyEntry::text(text)
    }

    fn col_of(children: Vec<WidgetSpec>) -> WidgetSpec {
        WidgetSpec::Col {
            children,
            key: None,
        }
    }

    /// Every covered variant, in the shapes the runtime branches on, asserted
    /// against the runtime itself.
    #[test]
    fn the_covered_variants_render_what_the_runtime_renders() {
        let cases: Vec<(&str, WidgetSpec)> = vec![
            (
                "one raw row",
                col_of(vec![WidgetSpec::Raw {
                    entries: vec![raw("hello")],
                    key: None,
                }]),
            ),
            (
                "several raw rows",
                col_of(vec![WidgetSpec::Raw {
                    entries: vec![raw("one"), raw("two"), raw("three")],
                    key: None,
                }]),
            ),
            (
                "an empty raw",
                col_of(vec![WidgetSpec::Raw {
                    entries: vec![],
                    key: None,
                }]),
            ),
            (
                "a hint bar",
                col_of(vec![WidgetSpec::HintBar {
                    entries: vec![hint("Tab", "next"), hint("Esc", "cancel")],
                    key: None,
                }]),
            ),
            (
                "a hint bar with one entry",
                col_of(vec![WidgetSpec::HintBar {
                    entries: vec![hint("Enter", "submit")],
                    key: None,
                }]),
            ),
            (
                "a default divider",
                col_of(vec![WidgetSpec::Divider {
                    ch: "─".into(),
                    style: None,
                    key: None,
                }]),
            ),
            (
                "a divider with another glyph",
                col_of(vec![WidgetSpec::Divider {
                    ch: "=".into(),
                    style: None,
                    key: None,
                }]),
            ),
            (
                "rows and dividers together",
                col_of(vec![
                    WidgetSpec::Raw {
                        entries: vec![raw("above")],
                        key: None,
                    },
                    WidgetSpec::Divider {
                        ch: "─".into(),
                        style: None,
                        key: None,
                    },
                    WidgetSpec::Raw {
                        entries: vec![raw("below")],
                        key: None,
                    },
                ]),
            ),
        ];
        for (label, spec) in cases {
            assert!(covered(&spec), "{label} should be covered");
            assert_eq!(tree_rows(&spec), runtime_rows(&spec), "{label}");
        }
    }

    /// **The coverage gate is the point of `covered`.** A panel is described
    /// or painted, never half of each, so one unmigrated child takes the whole
    /// spec down the old path.
    #[test]
    fn one_uncovered_child_makes_the_whole_spec_uncovered() {
        let covered_leaf = WidgetSpec::Raw {
            entries: vec![raw("x")],
            key: None,
        };
        assert!(covered(&covered_leaf));

        // Any variant this module has not reached yet. `WindowEmbed` is the
        // one that never will — it is a `Host` leaf by G's rule — so it stays
        // a valid example of "not described here" for the life of C.1.
        let uncovered = WidgetSpec::WindowEmbed {
            window_id: 1,
            rows: 3,
            key: None,
        };
        assert!(!covered(&uncovered));
        assert!(
            !covered(&col_of(vec![covered_leaf, uncovered])),
            "a column with one unmigrated child is not covered"
        );
    }

    /// The tree's rows for a spec under a context.
    ///
    /// **Grouped by row, because a styled row is many items.** A `text_runs`
    /// node emits one display item per run — that is how a run carries its own
    /// theme — so a toggle whose chip is styled differently from its label
    /// arrives as two items on one line. The runtime's unit is the entry,
    /// which is a line, so the comparison has to be made at that unit.
    fn tree_text(spec: &WidgetSpec, c: &Ctx) -> Vec<String> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(spec, WIDTH, c), Size::new(WIDTH, 24));
        rows_of(&ui)
    }

    /// Every line the display list paints, left to right, top to bottom.
    fn rows_of(ui: &Ui<UiMsg>) -> Vec<String> {
        let mut pieces: Vec<(i32, i32, String)> = Vec::new();
        for item in ui.spec().in_flow() {
            if let fresh_ui::Draw::Lines(lines) = &item.draw {
                for (i, l) in lines.iter().enumerate() {
                    pieces.push((item.rect.y + i as i32, item.rect.x, l.to_string()));
                }
            }
        }
        pieces.sort_by_key(|(y, x, _)| (*y, *x));
        let mut out: Vec<String> = Vec::new();
        let mut at: Option<i32> = None;
        for (y, _, s) in pieces {
            match at {
                Some(prev) if prev == y => out.last_mut().unwrap().push_str(&s),
                _ => {
                    out.push(s);
                    at = Some(y);
                }
            }
        }
        out
    }

    /// The runtime's, under the same context.
    fn runtime_text(spec: &WidgetSpec, c: &Ctx) -> Vec<String> {
        crate::widgets::render_spec_with_options(
            spec,
            &Default::default(),
            WIDTH as u32,
            crate::widgets::RenderOptions {
                prev_focus_key: &c.focus_key,
                auto_focus_first: false,
                marker_gutter: c.marker_gutter,
                hover_key: c.hovered_key.as_deref().unwrap_or(""),
                ..Default::default()
            },
        )
        .entries
        .iter()
        .map(|e| {
            let mut n = e.clone();
            n.normalize_widths();
            n.text.trim_end_matches('\n').to_string()
        })
        .collect()
    }

    fn button(label: &str, key: Option<&str>, disabled: bool, bare: bool) -> WidgetSpec {
        WidgetSpec::Button {
            label: label.into(),
            focused: false,
            intent: Default::default(),
            key: key.map(|k| k.into()),
            disabled,
            focusable: true,
            bare,
            full_width: false,
            hover_style: None,
        }
    }

    /// A button says what the runtime says it says — framed, bare, disabled,
    /// focused, and stretched. The label is `render_button`'s to decide and
    /// stays that way; only the hit moved.
    #[test]
    fn a_button_renders_what_the_runtime_renders() {
        let cases: Vec<(&str, WidgetSpec, Ctx)> = vec![
            ("framed", button("Go", Some("go"), false, false), cx()),
            ("bare", button("×", Some("x"), false, true), cx()),
            ("disabled", button("Go", Some("go"), true, false), cx()),
            ("keyless", button("Go", None, false, false), cx()),
            (
                "focused",
                button("Go", Some("go"), false, false),
                Ctx {
                    focus_key: "go".into(),
                    ..cx()
                },
            ),
            (
                "hovered",
                button("Go", Some("go"), false, false),
                Ctx {
                    hovered_key: Some("go".into()),
                    ..cx()
                },
            ),
            (
                "with the marker gutter",
                button("Go", Some("go"), false, false),
                Ctx {
                    marker_gutter: true,
                    ..cx()
                },
            ),
            (
                "full width",
                {
                    let mut b = button("Go", Some("go"), false, false);
                    if let WidgetSpec::Button { full_width, .. } = &mut b {
                        *full_width = true;
                    }
                    b
                },
                cx(),
            ),
        ];
        for (label, spec, c) in cases {
            assert!(covered(&spec));
            assert_eq!(tree_text(&spec, &c), runtime_text(&spec, &c), "{label}");
        }
    }

    /// **The seam.** A press delivers the widget's own hit — the same
    /// `HitArea` the runtime recorded — so `deliver_widget_hit` behind it does
    /// not change. What changed is that the tree found the widget.
    #[test]
    fn pressing_a_button_delivers_the_hit_the_runtime_recorded() {
        let spec = button("Go", Some("go"), false, false);
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(&spec, WIDTH, &cx()), Size::new(WIDTH, 24));
        let got: Vec<UiFact> = ui
            .dispatch(fresh_ui::Input::press(
                fresh_ui::Point::new(1, 0),
                fresh_ui::MouseButton::Left,
                fresh_ui::Mods::NONE,
            ))
            .msgs
            .into_iter()
            .filter_map(|m| match m {
                UiMsg::Ui(f) => Some(f),
                _ => None,
            })
            .collect();
        let UiFact::WidgetHit { slot, hit } = got.first().expect("a hit") else {
            panic!("expected a widget hit, got {got:?}");
        };
        assert_eq!(*slot, Slot::Floating);
        assert_eq!(hit.widget_key, "go");
        assert_eq!(hit.widget_kind, "button");
        assert_eq!(hit.event_type, "activate");
    }

    /// A disabled button has no hit at all — the runtime excludes it from the
    /// tab cycle, so a click that focused and activated it would be acting on
    /// a stale focus. The node simply is not a gesture.
    #[test]
    fn a_disabled_button_answers_no_press() {
        let spec = button("Go", Some("go"), true, false);
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(&spec, WIDTH, &cx()), Size::new(WIDTH, 24));
        let got = ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(1, 0),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        ));
        assert!(
            !got.msgs
                .iter()
                .any(|m| matches!(m, UiMsg::Ui(UiFact::WidgetHit { .. }))),
            "a disabled button is inert"
        );
    }

    fn toggle(label: &str, checked: bool, label_first: bool) -> WidgetSpec {
        WidgetSpec::Toggle {
            checked,
            label: label.into(),
            focused: false,
            indeterminate: false,
            label_first,
            label_width: 0,
            key: Some("t".into()),
        }
    }

    /// A toggle says what the runtime says it says, in both layouts and both
    /// states, focused and hovered.
    #[test]
    fn a_toggle_renders_what_the_runtime_renders() {
        let mut cases: Vec<(String, WidgetSpec, Ctx)> = Vec::new();
        for label_first in [false, true] {
            for checked in [false, true] {
                cases.push((
                    format!("label_first={label_first} checked={checked}"),
                    toggle("wrap", checked, label_first),
                    cx(),
                ));
            }
        }
        cases.push((
            "focused".into(),
            toggle("wrap", false, false),
            Ctx {
                focus_key: "t".into(),
                ..cx()
            },
        ));
        cases.push((
            "hovered".into(),
            toggle("wrap", false, false),
            Ctx {
                hovered_key: Some("t".into()),
                ..cx()
            },
        ));
        cases.push((
            "indeterminate".into(),
            {
                let mut t = toggle("wrap", false, true);
                if let WidgetSpec::Toggle { indeterminate, .. } = &mut t {
                    *indeterminate = true;
                }
                t
            },
            cx(),
        ));
        for (label, spec, c) in cases {
            assert!(covered(&spec));
            assert_eq!(tree_text(&spec, &c), runtime_text(&spec, &c), "{label}");
        }
    }

    /// **The chip, and only the chip.** In form layout a click on the label
    /// must not flip the value — the settings dialog's contract, which the
    /// runtime kept as a byte range and this keeps as where the nodes are.
    #[test]
    fn a_form_toggle_answers_on_its_chip_and_not_on_its_label() {
        let spec = toggle("wrap", false, true);
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(&spec, WIDTH, &cx()), Size::new(WIDTH, 24));
        let hit_at = |ui: &mut Ui<UiMsg>, x: i32| -> Option<UiFact> {
            ui.dispatch(fresh_ui::Input::press(
                fresh_ui::Point::new(x, 0),
                fresh_ui::MouseButton::Left,
                fresh_ui::Mods::NONE,
            ))
            .msgs
            .into_iter()
            .find_map(|m| match m {
                UiMsg::Ui(f @ UiFact::WidgetHit { .. }) => Some(f),
                _ => None,
            })
        };
        // The label is first, so column 0 is on it.
        assert!(
            hit_at(&mut ui, 0).is_none(),
            "a press on the label does not flip the value"
        );
        // The chip is at the end of the row; find its column from the runtime's
        // own byte range rather than guessing.
        let out = crate::widgets::render_spec_with_options(
            &spec,
            &Default::default(),
            WIDTH as u32,
            crate::widgets::RenderOptions {
                prev_focus_key: "",
                auto_focus_first: false,
                ..Default::default()
            },
        );
        let h = out.hits.first().expect("a hit");
        let chip_col = out.entries[0].text[..h.byte_start].chars().count() as i32;
        let got = hit_at(&mut ui, chip_col).expect("a press on the chip is the toggle's");
        let UiFact::WidgetHit { hit, .. } = got else {
            unreachable!()
        };
        assert_eq!(hit.widget_kind, "toggle");
        assert_eq!(hit.event_type, "toggle");
    }

    /// **The variant whose parity is geometric, not textual.** The runtime
    /// draws this frame as text — `╭─ label ─…─╮` in an entry, `│ … │` around
    /// every child row — because entries are all it has. The tree has a
    /// border and uses it, so the cells differ on purpose.
    ///
    /// What must not differ is the content rectangle, because everything
    /// downstream is addressed against it: the runtime gives the child
    /// `panel_width - 4` and then shifts six recorded channels by one row and
    /// the `│ ` prefix. Layout says the same thing once, and this is the
    /// assertion that it says the same thing.
    #[test]
    fn a_labeled_section_gives_its_child_the_rectangle_the_runtime_gave_it() {
        let inner_key = fresh_ui::Key::Str("ls_child".into());
        for label in ["", "Options"] {
            let spec = WidgetSpec::LabeledSection {
                label: label.into(),
                child: Box::new(WidgetSpec::Raw {
                    entries: vec![raw("body")],
                    key: None,
                }),
                width_pct: None,
                key: None,
            };
            assert!(covered(&spec));
            let mut ui: Ui<UiMsg> = Ui::new();
            ui.frame(
                node(&spec, WIDTH, &cx()).key(fresh_ui::Key::Str("ls".into())),
                Size::new(WIDTH, 24),
            );
            // The child is the only text the section contains besides the
            // legend, so find it by content rather than by index — the strip
            // changes the shape of the tree when a label is present.
            let body = ui
                .spec()
                .in_flow()
                .iter()
                .find_map(|i| match &i.draw {
                    fresh_ui::Draw::Lines(l) if l.iter().any(|s| &**s == "body") => Some(i.rect),
                    _ => None,
                })
                .expect("the child's row");
            assert_eq!(
                (body.x, body.y),
                (2, 1),
                "a column of ring and a column of padding, label={label:?}"
            );
            let _ = &inner_key;
        }
    }

    /// The child is laid out at `panel_width - 4`, which is `inner_width` —
    /// the number the runtime hands down before it starts shifting channels.
    #[test]
    fn a_labeled_sections_child_is_four_columns_narrower_than_the_panel() {
        let spec = WidgetSpec::LabeledSection {
            label: "L".into(),
            child: Box::new(WidgetSpec::Divider {
                ch: "─".into(),
                style: None,
                key: None,
            }),
            width_pct: None,
            key: None,
        };
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(&spec, WIDTH, &cx()), Size::new(WIDTH, 24));
        // A divider is as wide as the width it was given, so its glyph count
        // reports that width back.
        let rule = ui
            .spec()
            .in_flow()
            .iter()
            .find_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) if l.iter().any(|s| s.starts_with('─')) => {
                    Some(l[0].chars().count())
                }
                _ => None,
            })
            .expect("the rule");
        assert_eq!(rule, (WIDTH - 4) as usize);
    }

    /// An entry's inline overlays become runs, split at the overlay
    /// boundaries and merged in declaration order — the walk the painter does,
    /// with the theme *names* kept so the fold resolves them.
    #[test]
    fn inline_overlays_become_runs_at_their_boundaries() {
        use fresh_core::text_property::InlineOverlay;
        let mut e = raw("abcdef");
        e.inline_overlays = vec![InlineOverlay {
            start: 2,
            end: 4,
            style: OverlayOptions {
                bold: true,
                ..Default::default()
            },
            properties: Default::default(),
            unit: Default::default(),
        }];
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(entry_row(&e), Size::new(WIDTH, 4));
        let texts: Vec<String> = ui
            .spec()
            .in_flow()
            .iter()
            .filter_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) => {
                    Some(l.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(
            texts,
            vec!["ab".to_string(), "cd".to_string(), "ef".to_string()],
            "three runs, split where the overlay starts and ends"
        );
    }
}
