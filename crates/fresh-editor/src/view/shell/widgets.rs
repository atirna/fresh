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
pub fn node(spec: &WidgetSpec, width: u16) -> Node<UiMsg> {
    match spec {
        WidgetSpec::Row { children, wrap, .. } => {
            let r = row().children(children.iter().map(|c| node(c, width)).collect::<Vec<_>>());
            match wrap {
                true => r.wrap_children(),
                false => r,
            }
        }
        WidgetSpec::Col { children, .. } => {
            col().children(children.iter().map(|c| node(c, width)).collect::<Vec<_>>())
        }
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
        // `covered` gates this; reaching it is a bug in the caller rather than
        // a spec the plugin got wrong, so it is loud in debug and empty in
        // release rather than silently dropping a panel's content.
        other => {
            debug_assert!(false, "widget variant not covered: {other:?}");
            row().h(Sizing::Cells(0))
        }
    }
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

    if normalized.inline_overlays.is_empty() || text.is_empty() {
        return text_runs([Run::themed(text, base.to_string())]).h(Sizing::Cells(1));
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
        .chain(
            normalized
                .inline_overlays
                .iter()
                .flat_map(|o| [snap(o.start), snap(o.end)]),
        )
        .collect::<std::collections::BTreeSet<usize>>()
        .into_iter()
        .collect();

    let mut runs: Vec<Run> = Vec::with_capacity(bounds.len());
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
        runs.push(Run::themed(&text[a..b], ink.to_string()));
    }
    text_runs(runs).h(Sizing::Cells(1))
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
    use super::*;
    use fresh_core::api::HintEntry;
    use fresh_ui::{Size, Ui};

    const WIDTH: u16 = 40;

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
        use fresh_ui::Draw;
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(node(spec, WIDTH), Size::new(WIDTH, 24));
        let mut rows: Vec<(i32, i32, String)> = Vec::new();
        for item in ui.spec().in_flow() {
            if let Draw::Lines(lines) = &item.draw {
                for (i, l) in lines.iter().enumerate() {
                    rows.push((item.rect.y + i as i32, item.rect.x, l.to_string()));
                }
            }
        }
        rows.sort_by_key(|(y, x, _)| (*y, *x));
        // A row is every piece painted on one line, left to right — which is
        // what the runtime's single entry per row is.
        let mut out: Vec<String> = Vec::new();
        let mut at: Option<i32> = None;
        for (y, _, s) in rows {
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

        let uncovered = WidgetSpec::Button {
            label: "Go".into(),
            focused: false,
            intent: Default::default(),
            key: None,
            disabled: false,
            focusable: true,
            bare: false,
            full_width: false,
            hover_style: None,
        };
        assert!(!covered(&uncovered));
        assert!(
            !covered(&col_of(vec![covered_leaf, uncovered])),
            "a column with one unmigrated child is not covered"
        );
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
