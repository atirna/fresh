//! The settings dialog's box.
//!
//! Eleven modules and twenty thousand lines behind it; what moves here is the
//! outermost of the twenty-odd rectangles its painter records, which is the
//! one every other one is measured from. Same order as C.6 and the keybinding
//! editor: the frame first, the interior after.
//!
//! Ninety percent of the area, capped at 160 columns, centred with `area.x`
//! and `area.y` added back — the comment beside that addition says what it was
//! for: "centring with bare `area.width / 2` placed the modal at the FRAME
//! origin, where the dock then over-drew its left edge — hiding the title bar
//! and clipping the rounded top-left corner". Naming the region the layer may
//! occupy is that, said where the placing happens.
//!
//! **A rectangle, not a surface.** The interior is hit-tested against the
//! rectangles the painter records, and the modal slot is what routes a press
//! to it. A layer is offered the pointer before the ones below it and the
//! first with a path at the point wins, so a box that merely existed here
//! would swallow every click in the dialog. `PointerMode::Ignore` is what
//! keeps it geometry.

use fresh_ui::{layout_reader, row, Align, Anchor, LayoutInfo, Node, Place, PointerMode, Sizing};

use super::msg::UiMsg;

/// Never wider than this, however wide the area is.
pub const MAX_WIDTH: u16 = 160;
/// Below this the dialog does not open at all — the painter writes
/// "[Terminal too small for settings]" instead.
pub const MIN_AREA: (u16, u16) = (40, 10);

pub fn key() -> fresh_ui::Key {
    fresh_ui::Key::Str("settings_modal".into())
}

/// The box's size in an area of `info`'s extent, or `None` when the area is
/// too small for the dialog to open.
pub fn fit(info: LayoutInfo) -> Option<(u16, u16)> {
    let (w, h) = (info.constraints.max_w, info.constraints.max_h);
    if w < MIN_AREA.0 || h < MIN_AREA.1 {
        return None;
    }
    Some(((w * 90 / 100).min(MAX_WIDTH), h * 90 / 100))
}

/// The dialog's box as a layer: centred beside the dock, invisible to the
/// pointer.
pub fn layer() -> Node<UiMsg> {
    fresh_ui::layer()
        .within(super::frame::chrome_key())
        .anchor(Anchor::Screen(Align::Center))
        .place(Place::Over)
        .pointer_mode(PointerMode::Ignore)
        .child(layout_reader(|info: LayoutInfo| {
            let (w, h) = fit(info).unwrap_or((0, 0));
            row()
                .w(Sizing::Cells(w))
                .h(Sizing::Cells(h))
                .pointer_mode(PointerMode::Ignore)
                .key(key())
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::shell::frame::{frame_tree, Frame};
    use crate::view::shell::modal::Slot;
    use crate::view::shell::msg::UiFact;
    use fresh_ui::{Size, Ui};

    fn laid_out(w: u16, h: u16, dock: Option<u16>) -> Ui<UiMsg> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                settings: true,
                modal: Some(Slot::Settings),
                dock,
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(w, h),
        );
        ui
    }

    fn boxed(ui: &Ui<UiMsg>) -> fresh_ui::Rect {
        ui.rect_of(ui.find_by_key(&key()).expect("the box"))
    }

    /// Ninety percent, capped at 160, centred.
    #[test]
    fn the_box_is_ninety_percent_capped_and_centred() {
        let ui = laid_out(200, 60, None);
        let r = boxed(&ui);
        assert_eq!(r.w, MAX_WIDTH, "capped however wide the frame is");
        assert_eq!(r.h, 54);
        assert_eq!(r.x, (200 - MAX_WIDTH as i32) / 2);
    }

    /// **Beside the dock.** The painter added `area.x` back by hand because
    /// centring on the frame put the modal's left edge under the dock, which
    /// over-drew its title bar and clipped its rounded corner.
    #[test]
    fn the_box_centres_beside_the_dock() {
        let ui = laid_out(200, 60, Some(40));
        let r = boxed(&ui);
        assert!(r.x >= 40, "clear of the dock, at {}", r.x);
        assert_eq!(r.x, 40 + (160 - (160 * 90 / 100)) / 2);
    }

    /// An area below the guard has no dialog in it — the painter writes that
    /// it is too small instead.
    #[test]
    fn an_area_below_the_guard_has_no_box() {
        let ui = laid_out(30, 8, None);
        assert_eq!(boxed(&ui).w, 0, "nothing to place");
    }

    /// **A rectangle, not a surface**: a press inside it reaches the slot that
    /// routes to `handle_settings_mouse`, which hit-tests the interior's own
    /// recorded rectangles.
    #[test]
    fn a_press_inside_the_box_reaches_the_modal_router() {
        let mut ui = laid_out(200, 60, None);
        let r = boxed(&ui);
        let got = ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(r.x + 4, r.y + 4),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        ));
        assert!(
            got.msgs
                .iter()
                .any(|m| matches!(m, UiMsg::Ui(UiFact::ModalPointer(Slot::Settings)))),
            "the slot behind it answers: {:?}",
            got.msgs
        );
    }
}
