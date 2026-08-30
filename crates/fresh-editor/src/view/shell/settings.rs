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

use std::rc::Rc;

use fresh_ui::{
    col, gesture, layout_reader, row, text, Align, Anchor, Event, GestureKind, LayoutInfo,
    Modality, MouseButton, Node, Place, PointerMode, Scrim, Sizing,
};

use crate::app::shell_host::shell_theme::{attrs, pair};

use super::msg::UiFact;

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

/// What a press on one of the settings dialogs asks for.
///
/// **These were computed twice.** The painter laid each dialog out, and the
/// mouse handler laid it out *again* to find the button — `get_confirm_dialog_
/// button_at` carries the comment "same as in `render_confirm_dialog`" and
/// "must match `render_confirm_dialog`", which is the duplication stated
/// outright. The nodes are the buttons now, and the fact says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The unsaved-changes prompt: save, discard, cancel.
    Confirm(usize),
    /// The reset prompt: reset, cancel.
    Reset(usize),
}

/// One `label   description` pair of the help overlay. An empty `desc` is a
/// section heading.
#[derive(Clone, Debug, PartialEq)]
pub struct HelpLine {
    pub key: String,
    pub desc: String,
    pub heading: bool,
}

/// A choice prompt: a question, the changes it is about, and the buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub title: String,
    pub prompt: String,
    /// One line per pending change, listed under the prompt.
    pub changes: Vec<String>,
    pub buttons: Vec<String>,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub help: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Confirm(Choice),
    Reset(Choice),
    Help { title: String, lines: Vec<HelpLine> },
}

pub fn dialog_key() -> fresh_ui::Key {
    fresh_ui::Key::Str("settings_dialog".into())
}

pub fn button_key(i: usize) -> fresh_ui::Key {
    fresh_ui::Key::Pair("settings_dialog_button".into(), i as u64)
}

/// A dialog as a layer over the settings box.
///
/// `apply_dimming(frame, modal_area)` before each is the scrim; `within` the
/// box is what "centre it in the modal, not the frame" means, and it is the
/// same `parent_area` every one of these was handed.
pub fn dialog_layer(d: &Dialog) -> Node<UiMsg> {
    let d = d.clone();
    fresh_ui::layer()
        .within(key())
        .anchor(Anchor::Screen(Align::Center))
        .place(Place::Over)
        .modality(Modality::Exclusive)
        .scrim(Some(Scrim::Dim))
        .child(layout_reader(move |info: LayoutInfo| {
            // 50 wide, and as tall as it needs within 20 — the painter's own
            // two lines, with its `saturating_sub(4)` margin.
            let w = 50.min(info.constraints.max_w.saturating_sub(4));
            let want = match &d {
                Dialog::Help { .. } => 20,
                Dialog::Confirm(c) | Dialog::Reset(c) => (7 + c.changes.len() as u16).min(20),
            };
            let h = want.min(info.constraints.max_h.saturating_sub(4));
            let (ring, node) = match &d {
                Dialog::Help { title, lines } => (
                    pair("ui.menu_highlight_fg", "ui.popup_bg"),
                    help_box(title, lines),
                ),
                Dialog::Confirm(c) => (
                    pair("ui.status_warning_fg", "ui.popup_bg"),
                    choice_box(c, |i| Target::Confirm(i)),
                ),
                Dialog::Reset(c) => (
                    pair("ui.status_warning_fg", "ui.popup_bg"),
                    choice_box(c, |i| Target::Reset(i)),
                ),
            };
            col()
                .theme(ring)
                .border()
                .w(Sizing::Cells(w))
                .h(Sizing::Cells(h))
                .key(dialog_key())
                .children([node])
        }))
}

fn ink() -> String {
    pair("ui.popup_text_fg", "ui.popup_bg")
}

fn line(s: String, theme: String) -> Node<UiMsg> {
    text(s).theme(theme).h(Sizing::Cells(1))
}

fn rule() -> Node<UiMsg> {
    layout_reader(|info: LayoutInfo| {
        text("─".repeat(info.constraints.max_w.max(1) as usize))
            .theme(pair("ui.split_separator_fg", "ui.popup_bg"))
    })
    .h(Sizing::Cells(1))
}

fn help_box(title: &str, lines: &[HelpLine]) -> Node<UiMsg> {
    let mut rows: Vec<Node<UiMsg>> = vec![line(
        format!(" {title} "),
        attrs("ui.menu_highlight_fg", "ui.popup_bg", &["bold"]),
    )];
    for l in lines {
        rows.push(match l.heading {
            true => line(
                l.key.clone(),
                attrs("ui.popup_text_fg", "ui.popup_bg", &["bold"]),
            ),
            false => row().h(Sizing::Cells(1)).children([
                text(format!("  {:14}", l.key)).theme(attrs(
                    "ui.help_key_fg",
                    "ui.popup_bg",
                    &["bold"],
                )),
                text(l.desc.clone()).theme(ink()),
            ]),
        });
    }
    // The list can be taller than the box on a short frame, and the painter
    // clipped it; a viewport says there is more.
    fresh_ui::viewport(col().children(rows)).scrollbar().flex(1)
}

fn choice_box(c: &Choice, target: impl Fn(usize) -> Target + 'static) -> Node<UiMsg> {
    let mut rows: Vec<Node<UiMsg>> = vec![
        line(
            format!(" {} ", c.title),
            attrs("ui.status_warning_fg", "ui.popup_bg", &["bold"]),
        ),
        line(c.prompt.clone(), ink()),
        blank(),
    ];
    // The changes, in a window: the painter clipped the list at the dialog's
    // height, which is capped at twenty however many there are.
    rows.push(
        fresh_ui::viewport(
            col().children(
                c.changes
                    .iter()
                    .map(|d| line(format!("  {d}"), ink()))
                    .collect::<Vec<_>>(),
            ),
        )
        .scrollbar()
        .flex(1),
    );
    rows.push(rule());
    rows.push(buttons(c, target));
    rows.push(line(
        c.help.clone(),
        pair("ui.line_number_fg", "ui.popup_bg"),
    ));
    col().flex(1).children(rows)
}

fn blank() -> Node<UiMsg> {
    row().h(Sizing::Cells(1))
}

/// The centred `[ label ]` row. The painter summed the labels' widths and
/// divided; a row of naturally-sized children between two flexible gaps is
/// the same centring, and each button is where its own press lands.
fn buttons(c: &Choice, target: impl Fn(usize) -> Target + 'static) -> Node<UiMsg> {
    let target = Rc::new(target);
    let mut kids: Vec<Node<UiMsg>> = vec![row().flex(1)];
    for (i, label) in c.buttons.iter().enumerate() {
        let theme = match (i == c.selected, c.hovered == Some(i)) {
            (true, _) => attrs("ui.menu_highlight_fg", "ui.menu_highlight_bg", &["bold"]),
            (false, true) => pair("ui.menu_hover_fg", "ui.menu_hover_bg"),
            (false, false) => ink(),
        };
        let marker = match i == c.selected {
            true => ">",
            false => " ",
        };
        let t = target.clone();
        kids.push(
            gesture(text(format!("{marker}[ {label} ]")).theme(theme))
                .key(button_key(i))
                .on(
                    GestureKind::Press,
                    Rc::new(move |e: &Event| {
                        if e.button != MouseButton::Left {
                            return None;
                        }
                        e.stop();
                        Some(UiMsg::Ui(UiFact::SettingsDialog(t(i))))
                    }),
                )
                .on_enter({
                    let t = target.clone();
                    Rc::new(move |_: &Event| {
                        Some(UiMsg::Ui(UiFact::SettingsDialogHover(Some(t(i)))))
                    })
                })
                .on_leave(Rc::new(move |_: &Event| {
                    Some(UiMsg::Ui(UiFact::SettingsDialogHover(None)))
                })),
        );
        kids.push(text("  ").theme(ink()));
    }
    kids.push(row().flex(1));
    row().h(Sizing::Cells(1)).children(kids)
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

    fn choice(n: usize, selected: usize, hovered: Option<usize>) -> Choice {
        Choice {
            title: "Unsaved changes".into(),
            prompt: "Save before closing?".into(),
            changes: (0..n).map(|i| format!("editor.setting{i} → {i}")).collect(),
            buttons: vec!["Save".into(), "Discard".into(), "Abandon".into()],
            selected,
            hovered,
            help: "←/→/Tab: Select".into(),
        }
    }

    fn with_dialog(d: Dialog, w: u16, h: u16) -> Ui<UiMsg> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                settings: true,
                settings_dialog: Some(d),
                modal: Some(Slot::Settings),
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(w, h),
        );
        ui
    }

    fn facts(got: fresh_ui::Dispatch<UiMsg>) -> Vec<UiFact> {
        got.msgs
            .into_iter()
            .filter_map(|m| match m {
                UiMsg::Ui(f) => Some(f),
                _ => None,
            })
            .collect()
    }

    /// **Fifty wide, as tall as it needs within twenty, centred in the box** —
    /// the painter's own two lines, and the same `parent_area` it was handed.
    #[test]
    fn a_prompt_is_centred_in_the_box_at_its_documented_size() {
        let ui = with_dialog(Dialog::Confirm(choice(3, 0, None)), 200, 60);
        let d = ui.rect_of(ui.find_by_key(&dialog_key()).expect("the dialog"));
        let b = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        assert_eq!(d.w, 50);
        assert_eq!(d.h, 10, "seven plus one per change");
        assert_eq!(d.x, b.x + (b.w as i32 - 50) / 2, "centred in the box");
    }

    /// A long change list does not make the dialog grow past twenty — the
    /// painter's `.min(20)` — and the list scrolls inside it instead of being
    /// drawn past the bottom edge.
    #[test]
    fn a_long_change_list_is_capped_and_scrolls() {
        let ui = with_dialog(Dialog::Confirm(choice(40, 0, None)), 200, 60);
        let d = ui.rect_of(ui.find_by_key(&dialog_key()).expect("the dialog"));
        assert_eq!(d.h, 20);
        assert!(
            ui.spec()
                .items
                .iter()
                .any(|i| matches!(i.draw, fresh_ui::Draw::Scrollbar { .. })),
            "forty changes in a dialog of twenty scroll"
        );
    }

    /// **Each button answers its own press.** The arm behind them re-derived
    /// the painter's layout — "must match `render_confirm_dialog`" — to work
    /// out which one a cell was on.
    #[test]
    fn each_button_answers_its_own_press() {
        for i in 0..3 {
            let mut ui = with_dialog(Dialog::Confirm(choice(2, 0, None)), 200, 60);
            let r = ui.rect_of(ui.find_by_key(&button_key(i)).expect("a button"));
            let got = facts(ui.dispatch(fresh_ui::Input::press(
                fresh_ui::Point::new(r.x + 1, r.y),
                fresh_ui::MouseButton::Left,
                fresh_ui::Mods::NONE,
            )));
            assert!(
                got.contains(&UiFact::SettingsDialog(Target::Confirm(i))),
                "button {i}: {got:?}"
            );
        }
    }

    /// The reset prompt's two say `Reset`, not `Confirm` — the same buttons,
    /// a different question.
    #[test]
    fn the_reset_prompts_buttons_are_its_own() {
        let mut c = choice(1, 0, None);
        c.buttons = vec!["Reset".into(), "Cancel".into()];
        let mut ui = with_dialog(Dialog::Reset(c), 200, 60);
        let r = ui.rect_of(ui.find_by_key(&button_key(0)).expect("reset"));
        let got = facts(ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(r.x + 1, r.y),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        )));
        assert!(
            got.contains(&UiFact::SettingsDialog(Target::Reset(0))),
            "{got:?}"
        );
    }

    /// The buttons are centred as a group, which is what summing their widths
    /// and dividing did — said as two flexible gaps.
    #[test]
    fn the_buttons_are_centred_as_a_group() {
        let ui = with_dialog(Dialog::Confirm(choice(2, 0, None)), 200, 60);
        let d = ui.rect_of(ui.find_by_key(&dialog_key()).expect("the dialog"));
        let first = ui.rect_of(ui.find_by_key(&button_key(0)).expect("first"));
        let last = ui.rect_of(ui.find_by_key(&button_key(2)).expect("last"));
        let left = first.x - d.x;
        let right = (d.x + d.w as i32) - (last.x + last.w as i32);
        assert!(
            (left - right).abs() <= 3,
            "left {left} right {right} in a dialog {} wide",
            d.w
        );
    }

    /// **Nothing behind it is interactive**, which is what dimming the modal
    /// and swallowing every event meant.
    #[test]
    fn a_press_on_the_backdrop_does_nothing() {
        let mut ui = with_dialog(Dialog::Confirm(choice(2, 0, None)), 200, 60);
        let got = facts(ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(1, 1),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        )));
        assert!(
            !got.iter().any(|f| matches!(f, UiFact::SettingsDialog(_))),
            "{got:?}"
        );
    }

    /// The help overlay lists its shortcuts, and scrolls when the box is too
    /// short for all fifteen lines.
    #[test]
    fn the_help_overlay_lists_its_shortcuts() {
        let help = || Dialog::Help {
            title: "Keyboard Shortcuts".into(),
            lines: (0..15)
                .map(|i| HelpLine {
                    key: format!("k{i}"),
                    desc: format!("does {i}"),
                    heading: i % 5 == 0,
                })
                .collect(),
        };
        let ui = with_dialog(help(), 200, 60);
        let painted: Vec<String> = ui
            .spec()
            .layers()
            .iter()
            .filter_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) => {
                    Some(l.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect();
        assert!(painted.iter().any(|r| r.contains("does 1")), "{painted:?}");
    }
}
