//! The split grid, as a description.
//!
//! **One implementation of where the panes are.** `SplitNode` carried its own
//! layout engine — `get_leaves_with_rects` recursing over ratios and reserving
//! a cell per separator — and everything downstream was keyed on the
//! rectangles it produced: the separator drags, the per-pane scrollbars, the
//! tab strips, click-to-byte. That engine is this description now, and the
//! model's queries are reads of it (`SplitManager::get_visible_buffers`).
//!
//! The rule itself does not move. `split_rect_ext` converts a ratio to cells
//! and pins the first child so its sibling keeps `MIN_PANE_{WIDTH,HEIGHT}` —
//! app logic keyed on the available extent, the same shape as the dock's
//! bail-out. What `layout_reader` adds is the extent: `build()` cannot read
//! geometry, and this is the library's answer for app logic that needs it.
//!
//! No gestures and no paint here: the nodes carry keys and nothing else, so
//! this description can be laid out by the model with `M = ()` as easily as by
//! the shell. The dividers' drags and the panes' content are the editor's, and
//! are added where the grid is mounted.

use fresh_ui::{col, layout_reader, row, Key, LayoutInfo, Node, Sizing};

use crate::model::event::{ContainerId, LeafId, SplitDirection};
use crate::view::split::{split_rect_ext, SplitNode};

/// A pane's key, by the leaf it shows.
pub fn leaf_key(id: LeafId) -> Key {
    Key::Pair("pane".into(), id.0 .0 as u64)
}

/// A divider's key, by the container it splits.
pub fn divider_key(id: ContainerId) -> Key {
    Key::Pair("divider".into(), id.0 .0 as u64)
}

/// The grid for a split tree, with `maximized` taking the whole box when set.
pub fn grid<M: 'static>(root: &SplitNode, maximized: Option<LeafId>) -> Node<M> {
    if let Some(id) = maximized {
        if let Some(SplitNode::Leaf { split_id, .. }) = root.find(id.into()) {
            // A maximized pane is the whole box and there are no separators —
            // the same two facts `get_visible_buffers` and `get_separators`
            // state separately.
            return pane(*split_id);
        }
    }
    node_of(root)
}

fn pane<M: 'static>(id: LeafId) -> Node<M> {
    row().key(leaf_key(id))
}

fn node_of<M: 'static>(n: &SplitNode) -> Node<M> {
    match n {
        SplitNode::Leaf { split_id, .. } => pane(*split_id),
        SplitNode::Grouped { layout, .. } => node_of(layout),
        SplitNode::Split {
            direction,
            first,
            second,
            ratio,
            split_id,
            fixed_first,
            fixed_second,
        } => {
            let (dir, ratio) = (*direction, *ratio);
            let (ff, fs, id) = (*fixed_first, *fixed_second, *split_id);
            let (a, b) = (first.clone(), second.clone());
            // The cell counts need the extent, and `build` has none. The rule
            // is `split_rect_ext`'s — one copy, the model's own.
            layout_reader(move |info: LayoutInfo| {
                let whole = ratatui::layout::Rect::new(
                    0,
                    0,
                    info.constraints.max_w,
                    info.constraints.max_h,
                );
                let (ra, rb) = split_rect_ext(whole, dir, ratio, ff, fs);
                let (first, second) = (node_of::<M>(&a), node_of::<M>(&b));
                let divider = row().key(divider_key(id));
                match dir {
                    SplitDirection::Vertical => row().children([
                        first.w(Sizing::Cells(ra.width)),
                        divider.w(Sizing::Cells(1)),
                        second.w(Sizing::Cells(rb.width)),
                    ]),
                    SplitDirection::Horizontal => col().children([
                        first.h(Sizing::Cells(ra.height)),
                        divider.h(Sizing::Cells(1)),
                        second.h(Sizing::Cells(rb.height)),
                    ]),
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::event::BufferId;
    use fresh_core::SplitId;
    use fresh_ui::{Size, Ui};
    use ratatui::layout::Rect;

    fn leaf(n: usize) -> SplitNode {
        SplitNode::leaf(BufferId(n), SplitId(n))
    }

    fn split(dir: SplitDirection, a: SplitNode, b: SplitNode, ratio: f32, id: usize) -> SplitNode {
        SplitNode::split(dir, a, b, ratio, SplitId(id))
    }

    /// Every shape worth checking: a lone pane, each direction, an uneven
    /// ratio, nesting both ways, and a `Grouped` node standing in for its own
    /// layout.
    fn shapes() -> Vec<SplitNode> {
        use SplitDirection::{Horizontal, Vertical};
        vec![
            leaf(0),
            split(Vertical, leaf(0), leaf(1), 0.5, 10),
            split(Horizontal, leaf(0), leaf(1), 0.5, 10),
            split(Vertical, leaf(0), leaf(1), 0.25, 10),
            split(Horizontal, leaf(0), leaf(1), 0.8, 10),
            split(
                Vertical,
                split(Horizontal, leaf(0), leaf(1), 0.5, 11),
                leaf(2),
                0.5,
                10,
            ),
            split(
                Horizontal,
                leaf(0),
                split(
                    Vertical,
                    leaf(1),
                    split(Horizontal, leaf(2), leaf(3), 0.4, 12),
                    0.6,
                    11,
                ),
                0.3,
                10,
            ),
        ]
    }

    fn tree_rects(root: &SplitNode, at: Rect) -> Vec<(LeafId, Rect)> {
        let mut ui: Ui<()> = Ui::new();
        ui.frame(grid::<()>(root, None), Size::new(at.width, at.height));
        let mut out: Vec<(LeafId, Rect)> = Vec::new();
        for (id, _, _) in root.reference_leaves_with_rects(at) {
            let e = ui.find_by_key(&leaf_key(id));
            let r = e.map(|e| ui.rect_of(e)).unwrap_or_default();
            out.push((
                id,
                Rect::new(at.x + r.x.max(0) as u16, at.y + r.y.max(0) as u16, r.w, r.h),
            ));
        }
        out
    }

    /// **The tree lays the grid out exactly as the model always did.**
    ///
    /// This is the swap's whole safety argument: `get_leaves_with_rects` is a
    /// layout engine, and it is being replaced by one. It shares the rule
    /// (`split_rect_ext`), so a divergence here would be the *structure*
    /// disagreeing — a reserved separator cell, or which child takes the
    /// remainder.
    #[test]
    fn the_grid_places_every_pane_where_the_model_does() {
        for (i, root) in shapes().iter().enumerate() {
            for (w, h) in [(80u16, 24u16), (200, 60), (40, 12), (31, 9), (120, 40)] {
                let at = Rect::new(0, 0, w, h);
                let want: Vec<(LeafId, Rect)> = root
                    .reference_leaves_with_rects(at)
                    .into_iter()
                    .map(|(id, _, r)| (id, r))
                    .collect();
                assert_eq!(tree_rects(root, at), want, "shape {i} at {w}x{h}");
            }
        }
    }

    /// And at an offset: the model partitions the rectangle it is given, so
    /// the tree's answer is its own rectangle plus the frame's origin.
    #[test]
    fn an_offset_box_moves_every_pane_with_it() {
        let root = split(SplitDirection::Vertical, leaf(0), leaf(1), 0.5, 10);
        let at = Rect::new(7, 3, 60, 20);
        let want: Vec<(LeafId, Rect)> = root
            .reference_leaves_with_rects(at)
            .into_iter()
            .map(|(id, _, r)| (id, r))
            .collect();
        assert_eq!(tree_rects(&root, at), want);
    }

    /// A maximized pane is the whole box, and the separators go with it —
    /// two facts `get_visible_buffers` and `get_separators` state apart.
    #[test]
    fn a_maximized_pane_takes_the_whole_box() {
        let root = split(SplitDirection::Vertical, leaf(0), leaf(1), 0.5, 10);
        let mut ui: Ui<()> = Ui::new();
        ui.frame(
            grid::<()>(&root, Some(LeafId(SplitId(1)))),
            Size::new(80, 24),
        );
        let r = ui.rect_of(
            ui.find_by_key(&leaf_key(LeafId(SplitId(1))))
                .expect("the pane"),
        );
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 80, 24));
        assert!(
            ui.find_by_key(&leaf_key(LeafId(SplitId(0)))).is_none(),
            "the other pane is not shown at all"
        );
    }

    /// What one grid layout costs — the number the swap turns on, since the
    /// model's queries become this and some of them are per-frame.
    ///
    /// Reported, not bounded tightly: a wall-clock threshold is a flake
    /// waiting for a loaded runner. `--nocapture` to read it.
    #[test]
    fn a_grid_layout_is_cheap_enough_to_be_the_query() {
        use std::time::Instant;
        let root = split(
            SplitDirection::Horizontal,
            leaf(0),
            split(SplitDirection::Vertical, leaf(1), leaf(2), 0.5, 11),
            0.3,
            10,
        );
        const N: u32 = 2_000;
        let t = Instant::now();
        for _ in 0..N {
            let mut ui: Ui<()> = Ui::new();
            ui.frame(grid::<()>(&root, None), Size::new(200, 60));
            std::hint::black_box(ui.find_by_key(&leaf_key(LeafId(SplitId(2)))));
        }
        let per = t.elapsed() / N;
        println!("grid layout (3 panes, cold): {per:?}");
        assert!(per.as_millis() < 10, "a grid layout took {per:?}");
    }

    /// The dividers land on the cells the model reserves for them.
    #[test]
    fn the_dividers_are_where_the_separators_are() {
        for (i, root) in shapes().iter().enumerate() {
            for (w, h) in [(80u16, 24u16), (200, 60), (41, 13)] {
                let at = Rect::new(0, 0, w, h);
                let mut ui: Ui<()> = Ui::new();
                ui.frame(grid::<()>(root, None), Size::new(w, h));
                for (id, dir, x, y, len) in root.get_separators_with_ids(at) {
                    let e = ui
                        .find_by_key(&divider_key(id))
                        .unwrap_or_else(|| panic!("shape {i}: no divider for {id:?}"));
                    let r = ui.rect_of(e);
                    let want = match dir {
                        SplitDirection::Horizontal => (x, y, len, 1),
                        SplitDirection::Vertical => (x, y, 1, len),
                    };
                    assert_eq!(
                        (r.x as u16, r.y as u16, r.w, r.h),
                        want,
                        "shape {i} at {w}x{h}, divider {id:?}"
                    );
                }
            }
        }
    }
}
