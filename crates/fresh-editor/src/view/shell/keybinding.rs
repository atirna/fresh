//! The keybinding editor's box.
//!
//! **The frame first, the interior after** — the same order the floating
//! plugin panel took (C.6). The editor is a table with its own scrollbar, its
//! own double-click semantics and ten rectangles its painter records for a
//! mouse handler to compare against; what moves here is the outermost of them,
//! which is the one both the painter and the handler used.
//!
//! `keybinding_modal_area` was four lines of arithmetic — ninety percent of
//! the area it was handed, capped at 120 columns, floored at 60 by 20, then
//! centred with `area.x`/`area.y` added back so it lands beside the dock
//! rather than under it. The floor and the cap are the *rule* and they stay;
//! the centring and the offsets are what a layer does, and naming the region
//! it may occupy is what "beside the dock" means.
//!
//! The cap has no property to be: `min_w` exists and `max_w` does not, so the
//! width is resolved from the extent the way §4.4 sanctions — a
//! `layout_reader`, which is content resolved from a *known* extent rather
//! than geometry recorded from a paint. `view::shell::calibration` does the
//! same thing for the same reason.

use std::rc::Rc;

use fresh_ui::{
    col, gesture, layout_reader, row, text, Align, Anchor, Event, GestureKind, LayoutInfo,
    Modality, MouseButton, Node, Place, PointerMode, Scrim, Sizing,
};

use crate::app::shell_host::shell_theme::{attrs, pair};

use super::msg::UiFact;

use super::msg::UiMsg;

/// Never wider than this, however wide the area is.
pub const MAX_WIDTH: u16 = 120;
/// And never smaller than this, however small it is.
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 20;

/// What a press on one of the editor's dialogs asks for.
///
/// Each was a rectangle the painter filed in `KeybindingEditorLayout` and the
/// mouse arm compared a cell against, in a chain of `point_in_rect`. They are
/// where the nodes are now, and the fact says what was pressed rather than
/// where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The edit dialog's key field: focus it and start recording.
    KeyField,
    /// Its action field: focus it and start editing.
    ActionField,
    /// Its context field: focus it and start editing.
    ContextField,
    /// Its `[ Save ]`, which applies the edit.
    Save,
    /// Its `[ Cancel ]`, which closes it.
    Cancel,
    /// The unsaved-changes confirmation's three.
    ConfirmSave,
    ConfirmDiscard,
    ConfirmCancel,
}

/// One `key  description` row of the help overlay. An empty `desc` is a
/// section heading, which is how the painter's `is_header` flag read.
#[derive(Clone, Debug, PartialEq)]
pub struct HelpLine {
    pub key: String,
    pub desc: String,
    pub heading: bool,
}

/// The help overlay: a static list of bindings, and nothing else.
#[derive(Clone, Debug, PartialEq)]
pub struct Help {
    pub title: String,
    pub lines: Vec<HelpLine>,
}

/// One field of the edit dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub label: String,
    pub value: String,
    /// A hint shown beside the value while the field has focus.
    pub hint: Option<String>,
    pub focused: bool,
    /// The value reads as an error — the action name did not resolve.
    pub invalid: bool,
    /// A caret is drawn after the value, for a field being typed into.
    pub caret: bool,
    pub target: Target,
}

/// The edit / add binding dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub title: String,
    pub instructions: String,
    pub key_field: Field,
    pub action_field: Field,
    /// The resolved action's human-readable form, when it differs from what
    /// was typed.
    pub action_description: Option<String>,
    pub context_field: Field,
    /// The action-name error, shown above the conflicts.
    pub error: Option<String>,
    pub conflicts_label: String,
    pub conflicts: Vec<String>,
    pub save_label: String,
    pub cancel_label: String,
    /// Which button is focused, when the buttons are.
    pub focused_button: Option<usize>,
    /// The action field's autocomplete, when it is open.
    pub autocomplete: Option<Autocomplete>,
}

/// The action field's suggestion list.
#[derive(Clone, Debug, PartialEq)]
pub struct Autocomplete {
    pub suggestions: Vec<String>,
    pub selected: Option<usize>,
}

/// How many suggestions the popup shows at once.
pub const AUTOCOMPLETE_VISIBLE: usize = 8;

/// The unsaved-changes confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    /// Save, discard, cancel — with the selected one marked.
    pub buttons: Vec<String>,
    pub selected: usize,
}

/// The editor's dialogs, when any is open. At most one is: the painter
/// returned early on the help overlay and the input handler gates the other
/// two the same way.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Help(Help),
    Edit(Edit),
    Confirm(Confirm),
}

pub fn key() -> fresh_ui::Key {
    fresh_ui::Key::Str("keybinding_modal".into())
}

/// The box's size in an area of `info`'s extent.
///
/// `keybinding_modal_area`'s own two lines, kept because they are the rule
/// rather than the placement: ninety percent, capped, floored, and never
/// wider than the area less the two columns it keeps clear.
pub fn fit(info: LayoutInfo) -> (u16, u16) {
    let (w, h) = (info.constraints.max_w, info.constraints.max_h);
    let width = ((w as f32 * 0.90) as u16)
        .min(MAX_WIDTH)
        .max(MIN_WIDTH)
        .min(w.saturating_sub(2));
    let height = ((h as f32 * 0.90) as u16)
        .max(MIN_HEIGHT)
        .min(h.saturating_sub(2));
    (width, height)
}

/// The editor's box as a layer: centred beside the dock, and **invisible to
/// the pointer**.
///
/// It paints nothing — the interior still does, and a layer is in the overlay
/// band, so anything drawn here would land on top of the painter that owns the
/// surface. What it contributes is the rectangle, which the painter reads back
/// instead of computing.
///
/// **And only the rectangle.** `hit_paths` returns the first layer with any
/// path at the point, so a box that merely *existed* over the modal slot would
/// take every press inside itself and the slot behind it — the one that routes
/// to `handle_keybinding_editor_mouse` — would never be asked. That is not a
/// missing handler, it is a surface that swallows every click in the editor it
/// is standing in for. `PointerMode::Ignore` takes the subtree out of
/// hit-testing entirely, which is what "a rectangle, not a surface" means.
///
/// The exclusivity is the slot's, not this layer's, for the same reason: two
/// claims to the same modality is one too many, and the slot is the one that
/// carries the routing.
pub fn layer() -> Node<UiMsg> {
    fresh_ui::layer()
        .within(super::frame::chrome_key())
        .anchor(Anchor::Screen(Align::Center))
        .place(Place::Over)
        // On the layer, not only on the box inside it: the layer node is a box
        // of its own and would produce the path by itself.
        .pointer_mode(PointerMode::Ignore)
        .child(layout_reader(|info: LayoutInfo| {
            let (w, h) = fit(info);
            row()
                .w(Sizing::Cells(w))
                .h(Sizing::Cells(h))
                .pointer_mode(PointerMode::Ignore)
                .key(key())
        }))
}

/// The dialogs as layers over the editor's box.
///
/// **They are layers because of paint order.** The editor's interior is still
/// the painter's, and the tree's overlay band is folded after every legacy
/// painter — so a described dialog lands on top of the table the painter drew,
/// which is exactly where it was drawn before. The other direction would not
/// work: describing the *table* first would have covered the painter's
/// dialogs. That ordering is why these go first.
///
/// `apply_dimming(frame, modal_area)` before the edit and confirm dialogs is
/// the scrim; the help overlay had none and still has none.
pub fn dialog_layer(d: &Dialog) -> Node<UiMsg> {
    let (w, h, node, scrim) = match d {
        Dialog::Help(x) => (52, 22, help_box(x), None),
        Dialog::Edit(x) => (56, 18, edit_box(x), Some(Scrim::Dim)),
        Dialog::Confirm(x) => (44, 7, confirm_box(x), Some(Scrim::Dim)),
    };
    fresh_ui::layer()
        .within(key())
        .anchor(Anchor::Screen(Align::Center))
        .place(Place::Over)
        .modality(Modality::Exclusive)
        .scrim(scrim)
        .child(layout_reader(move |info: LayoutInfo| {
            // The painter's own `min(area - 4)`, against the box it centres in.
            let node = node.clone();
            node.w(Sizing::Cells(
                w.min(info.constraints.max_w.saturating_sub(4)),
            ))
            .h(Sizing::Cells(
                h.min(info.constraints.max_h.saturating_sub(4)),
            ))
        }))
}

fn ring() -> String {
    pair("ui.popup_border_fg", "ui.popup_bg")
}

fn ink() -> String {
    pair("ui.popup_text_fg", "ui.popup_bg")
}

fn line(s: String, theme: String) -> Node<UiMsg> {
    text(s).theme(theme).h(Sizing::Cells(1))
}

fn blank() -> Node<UiMsg> {
    row().h(Sizing::Cells(1))
}

fn titled(s: &str, theme: &str) -> Node<UiMsg> {
    line(format!(" {s} "), attrs(theme, "ui.popup_bg", &["bold"]))
}

fn help_box(h: &Help) -> Node<UiMsg> {
    let mut rows: Vec<Node<UiMsg>> = vec![titled(&h.title, "ui.popup_border_fg")];
    for l in &h.lines {
        rows.push(match l.heading {
            true => line(
                l.key.clone(),
                attrs("ui.popup_text_fg", "ui.popup_bg", &["bold"]),
            ),
            false => row().h(Sizing::Cells(1)).children([
                text(format!("{:16}", l.key)).theme(attrs(
                    "ui.help_key_fg",
                    "ui.popup_bg",
                    &["bold"],
                )),
                text(l.desc.clone()).theme(ink()),
            ]),
        });
    }
    // The list is longer than the box on a short frame, and the painter simply
    // clipped it. A viewport says there is more.
    col()
        .theme(ring())
        .border()
        .child(fresh_ui::viewport(col().children(rows)).scrollbar().flex(1))
}

/// One field row: a padded label, the value, and an optional hint — with the
/// whole row taking the focused background when it has focus, which is what
/// the painter's "paint an empty `Paragraph` in `field_bg` first" was.
fn field_row(f: &Field) -> Node<UiMsg> {
    let bg = match f.focused {
        true => "ui.popup_selection_bg",
        false => "ui.popup_bg",
    };
    let label = match f.focused {
        true => attrs("ui.help_key_fg", bg, &["bold"]),
        false => pair("ui.popup_text_fg", bg),
    };
    let value = match f.invalid {
        true => pair("ui.diagnostic_error_fg", bg),
        false => pair("ui.popup_text_fg", bg),
    };
    let mut spans: Vec<Node<UiMsg>> = vec![
        text(format!("   {:9}", f.label)).theme(label),
        text(f.value.clone()).theme(value),
    ];
    if f.caret {
        spans.push(text("_").theme(pair("ui.cursor", bg)));
    }
    if let Some(hint) = &f.hint {
        spans.push(text(format!("  {hint}")).theme(pair("ui.popup_text_fg", bg)));
    }
    let target = f.target;
    gesture(
        row()
            .h(Sizing::Cells(1))
            .theme(pair("ui.popup_text_fg", bg))
            .children(spans),
    )
    .on(
        GestureKind::Press,
        Rc::new(move |e: &Event| {
            if e.button != MouseButton::Left {
                return None;
            }
            e.stop();
            Some(UiMsg::Ui(UiFact::KeybindingDialog(target)))
        }),
    )
}

fn button(label: &str, focused: bool, target: Target) -> Node<UiMsg> {
    let theme = match focused {
        true => attrs("ui.popup_bg", "ui.help_key_fg", &["bold"]),
        false => ink(),
    };
    gesture(text(format!(" {label} ")).theme(theme)).on(
        GestureKind::Press,
        Rc::new(move |e: &Event| {
            if e.button != MouseButton::Left {
                return None;
            }
            e.stop();
            Some(UiMsg::Ui(UiFact::KeybindingDialog(target)))
        }),
    )
}

fn edit_box(e: &Edit) -> Node<UiMsg> {
    let mut info: Vec<Node<UiMsg>> = Vec::new();
    if let Some(err) = &e.error {
        info.push(line(
            format!("   ✗ {err}"),
            attrs("ui.diagnostic_error_fg", "ui.popup_bg", &["bold"]),
        ));
    }
    if !e.conflicts.is_empty() {
        info.push(line(
            format!("   {}", e.conflicts_label),
            attrs("ui.status_warning_fg", "ui.popup_bg", &["bold"]),
        ));
        for c in &e.conflicts {
            info.push(line(
                format!("     {c}"),
                pair("ui.status_warning_fg", "ui.popup_bg"),
            ));
        }
    }

    let described = match &e.action_description {
        Some(d) => line(
            format!("            → {d}"),
            attrs("ui.popup_text_fg", "ui.popup_bg", &["italic"]),
        ),
        None => blank(),
    };

    // The action field, with its suggestion list hanging off it. The painter
    // placed the popup at `action_field.x + 12, action_field.y + 1` — under
    // the field, past the label — which is `Place::Below` and an offset.
    let action = match &e.autocomplete {
        None => field_row(&e.action_field),
        Some(a) => row()
            .h(Sizing::Cells(1))
            .children([field_row(&e.action_field), autocomplete_layer(a)]),
    };

    col().theme(ring()).border().children([
        titled(&e.title, "ui.popup_border_fg"),
        line(format!(" {}", e.instructions), ink()),
        blank(),
        field_row(&e.key_field),
        action,
        described,
        field_row(&e.context_field),
        blank(),
        col().flex(1).children(info),
        row().h(Sizing::Cells(1)).children([
            text("   ").theme(ink()),
            button(&e.save_label, e.focused_button == Some(0), Target::Save),
            text("  ").theme(ink()),
            button(&e.cancel_label, e.focused_button == Some(1), Target::Cancel),
        ]),
    ])
}

fn autocomplete_layer(a: &Autocomplete) -> Node<UiMsg> {
    // The painter windowed the list by hand — `selected - VISIBLE + 1` — and
    // drew at most eight. A `List` with the selection controlled reveals it,
    // and the window is the viewport's.
    let items = std::rc::Rc::new(a.suggestions.clone());
    let n = items.len();
    let list = fresh_ui::List::windowed(n, |i| fresh_ui::Key::Str(i.to_string().into()), {
        let items = items.clone();
        move |i| text(items[i].clone())
    })
    .focusable(false)
    .scrollbar()
    .row_theme(|_, st| match st {
        fresh_ui::widgets::RowState::Selected | fresh_ui::widgets::RowState::SelectedBlur => {
            attrs("ui.popup_bg", "ui.help_key_fg", &["bold"])
        }
        _ => pair("ui.popup_text_fg", "ui.popup_bg"),
    });
    let list = match a.selected {
        Some(i) => list.selected(i),
        None => list,
    };
    fresh_ui::layer()
        .anchor(Anchor::Parent)
        .place(Place::Below)
        // Past the label, which is where the painter put it: `x + 12`.
        .offset(12, 0)
        .fit(fresh_ui::Fit::FLIP.or(fresh_ui::Fit::CLAMP))
        .child(
            col()
                .theme(ring())
                .border()
                .w(Sizing::Cells(36))
                .h(Sizing::Cells(
                    (n.min(AUTOCOMPLETE_VISIBLE) as u16).saturating_add(2),
                ))
                .child(fresh_ui::ComponentExt::node(list).flex(1)),
        )
}

fn confirm_box(c: &Confirm) -> Node<UiMsg> {
    let targets = [
        Target::ConfirmSave,
        Target::ConfirmDiscard,
        Target::ConfirmCancel,
    ];
    let mut kids: Vec<Node<UiMsg>> = vec![text(" ").theme(ink())];
    for (i, label) in c.buttons.iter().enumerate() {
        kids.push(button(
            label,
            i == c.selected,
            *targets.get(i).unwrap_or(&Target::ConfirmCancel),
        ));
        kids.push(text("  ").theme(ink()));
    }
    col()
        .theme(pair("ui.status_warning_fg", "ui.popup_bg"))
        .border()
        .children([
            titled(&c.title, "ui.status_warning_fg"),
            line(format!(" {}", c.message), ink()),
            blank(),
            row().flex(1),
            row().h(Sizing::Cells(1)).children(kids),
        ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::shell::frame::{frame_tree, Frame};
    use fresh_ui::{Size, Ui};

    fn laid_out(w: u16, h: u16, dock: Option<u16>) -> Ui<UiMsg> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                keybinding: true,
                dock,
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(w, h),
        );
        ui
    }

    /// **Ninety percent, capped, floored, centred** — the painter's own rule,
    /// arrived at by layout instead of by four lines of arithmetic.
    #[test]
    fn the_box_is_ninety_percent_capped_and_centred() {
        let ui = laid_out(200, 60, None);
        let r = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        assert_eq!(r.w, MAX_WIDTH, "capped at 120 however wide the frame is");
        assert_eq!(r.h, 54, "ninety percent of sixty");
        assert_eq!(r.x, (200 - MAX_WIDTH as i32) / 2, "centred across");
    }

    /// A frame too small for the cap gets ninety percent of itself.
    #[test]
    fn a_narrow_frame_gets_ninety_percent_of_itself() {
        let ui = laid_out(100, 40, None);
        let r = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        assert_eq!((r.w, r.h), (90, 36));
    }

    /// And one too small for the floor gets the floor, less the two columns
    /// the painter kept clear.
    #[test]
    fn a_tiny_frame_gets_the_floor_less_its_margin() {
        let ui = laid_out(50, 15, None);
        let r = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        assert_eq!((r.w, r.h), (48, 13));
    }

    /// **Beside the dock, not under it.** The painter added `area.x` back by
    /// hand because it was handed the post-dock chrome area; naming the region
    /// the layer may occupy says the same thing where the placing happens.
    ///
    /// This is `modal_centres_within_offset_area_left_of_dock`, moved: the
    /// modal used to be placed relative to column 0 and bled left under the
    /// dock.
    #[test]
    fn the_box_centres_beside_the_dock() {
        let ui = laid_out(200, 60, Some(40));
        let r = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        assert!(r.x >= 40, "clear of a forty-column dock, at {}", r.x);
        assert_eq!(
            r.x,
            40 + (160 - MAX_WIDTH as i32) / 2,
            "centred in what is left"
        );
    }

    /// **The box is a rectangle, not a surface.** Its interior is still the
    /// painter's — a table, a search bar, three dialogs, all hit-tested
    /// against rectangles it records — so a press inside it has to reach the
    /// modal slot that routes to `handle_keybinding_editor_mouse`. A layer
    /// that answered would be the first one with a path at the point and the
    /// slot behind it would never be asked, which is a surface that swallows
    /// every click in the editor it is standing in for.
    #[test]
    fn a_press_inside_the_box_reaches_the_modal_router() {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                keybinding: true,
                modal: Some(crate::view::shell::modal::Slot::KeybindingEditor),
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(200, 60),
        );
        let r = ui.rect_of(ui.find_by_key(&key()).expect("the box"));
        let got = ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(r.x + 4, r.y + 4),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        ));
        assert!(
            got.msgs.iter().any(|m| matches!(
                m,
                UiMsg::Ui(crate::view::shell::msg::UiFact::ModalPointer(
                    crate::view::shell::modal::Slot::KeybindingEditor
                ))
            )),
            "the slot behind it answers: {:?}",
            got.msgs
        );
    }

    /// And a press *outside* the box is the modal's too — the capture band
    /// consumed everything, and `Modality::Exclusive` on the slot is that.
    #[test]
    fn a_press_outside_the_box_is_still_the_modals() {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                keybinding: true,
                modal: Some(crate::view::shell::modal::Slot::KeybindingEditor),
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(200, 60),
        );
        let got = ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(2, 2),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        ));
        assert!(
            got.msgs.iter().any(|m| matches!(
                m,
                UiMsg::Ui(crate::view::shell::msg::UiFact::ModalPointer(_))
            )),
            "consumed by the modal: {:?}",
            got.msgs
        );
    }

    fn field(label: &str, value: &str, focused: bool, target: Target) -> Field {
        Field {
            label: label.into(),
            value: value.into(),
            hint: None,
            focused,
            invalid: false,
            caret: false,
            target,
        }
    }

    fn edit() -> Edit {
        Edit {
            title: "Edit binding".into(),
            instructions: "Press a key".into(),
            key_field: field("Key:", "Ctrl+S", true, Target::KeyField),
            action_field: field("Action:", "save", false, Target::ActionField),
            action_description: Some("Store the file".into()),
            context_field: field("Context:", "[normal]", false, Target::ContextField),
            error: None,
            conflicts_label: "Conflicts:".into(),
            conflicts: Vec::new(),
            save_label: "Apply".into(),
            cancel_label: "Dismiss".into(),
            focused_button: None,
            autocomplete: None,
        }
    }

    fn with_dialog(d: Dialog, w: u16, h: u16) -> Ui<UiMsg> {
        let mut ui: Ui<UiMsg> = Ui::new();
        ui.frame(
            frame_tree(Frame {
                keybinding: true,
                keybinding_dialog: Some(d),
                modal: Some(crate::view::shell::modal::Slot::KeybindingEditor),
                menu_bar: false,
                status_bar: false,
                ..Frame::default()
            }),
            Size::new(w, h),
        );
        ui
    }

    fn facts(got: fresh_ui::Dispatch<UiMsg>) -> Vec<crate::view::shell::msg::UiFact> {
        got.msgs
            .into_iter()
            .filter_map(|m| match m {
                UiMsg::Ui(f) => Some(f),
                _ => None,
            })
            .collect()
    }

    fn painted(ui: &Ui<UiMsg>) -> Vec<String> {
        ui.spec()
            .layers()
            .iter()
            .filter_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) => {
                    Some(l.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// **Every field and button answers its own press.** Each was a rectangle
    /// the painter filed and the mouse arm compared a cell against, in three
    /// chains of `point_in_rect`; the fact says what was pressed rather than
    /// where.
    #[test]
    fn the_edit_dialogs_fields_and_buttons_answer_for_themselves() {
        use crate::view::shell::msg::UiFact;
        for (needle, want) in [
            ("Key:", Target::KeyField),
            ("Action:", Target::ActionField),
            ("Context:", Target::ContextField),
            ("Apply", Target::Save),
            ("Dismiss", Target::Cancel),
        ] {
            let mut ui = with_dialog(Dialog::Edit(edit()), 120, 40);
            // Find the row carrying the label, and press inside it.
            let at = ui
                .spec()
                .layers()
                .iter()
                .find_map(|i| match &i.draw {
                    fresh_ui::Draw::Lines(l)
                        if l.iter().any(|s| s.contains(needle)) && i.rect.w > 0 =>
                    {
                        Some((i.rect.x, i.rect.y))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("a row carrying {needle:?}"));
            let got = facts(ui.dispatch(fresh_ui::Input::press(
                fresh_ui::Point::new(at.0, at.1),
                fresh_ui::MouseButton::Left,
                fresh_ui::Mods::NONE,
            )));
            assert!(
                got.contains(&UiFact::KeybindingDialog(want)),
                "{needle:?} asks for {want:?}, got {got:?}"
            );
        }
    }

    /// The confirmation's three buttons likewise, with the selected one marked.
    #[test]
    fn the_confirmations_buttons_answer_for_themselves() {
        use crate::view::shell::msg::UiFact;
        let confirm = || {
            Dialog::Confirm(Confirm {
                title: "Unsaved changes".into(),
                message: "Save before closing?".into(),
                buttons: vec!["Save".into(), "Discard".into(), "Cancel".into()],
                selected: 1,
            })
        };
        let mut ui = with_dialog(confirm(), 120, 40);
        let at = ui
            .spec()
            .layers()
            .iter()
            .find_map(|i| match &i.draw {
                fresh_ui::Draw::Lines(l) if l.iter().any(|s| s.contains("Discard")) => {
                    Some((i.rect.x, i.rect.y))
                }
                _ => None,
            })
            .expect("the discard button");
        let got = facts(ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(at.0, at.1),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        )));
        assert!(
            got.contains(&UiFact::KeybindingDialog(Target::ConfirmDiscard)),
            "{got:?}"
        );
    }

    /// **The dialog is exclusive**, which is what `apply_dimming` plus a
    /// `Clear` over the modal meant: a press on the backdrop is consumed and
    /// does nothing.
    #[test]
    fn a_press_on_the_backdrop_does_nothing() {
        use crate::view::shell::msg::UiFact;
        let mut ui = with_dialog(Dialog::Edit(edit()), 120, 40);
        let got = facts(ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(1, 1),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        )));
        assert!(
            !got.iter().any(|f| matches!(f, UiFact::KeybindingDialog(_))),
            "no field claims it: {got:?}"
        );
    }

    /// The resolved action's readable form is shown only when it says
    /// something the typed name does not — the painter's own comparison, kept
    /// on the description side of the seam.
    #[test]
    fn the_action_description_is_optional() {
        let with = painted(&with_dialog(Dialog::Edit(edit()), 120, 40));
        assert!(
            with.iter().any(|r| r.contains("→ Store the file")),
            "{with:?}"
        );
        let mut e = edit();
        e.action_description = None;
        let without = painted(&with_dialog(Dialog::Edit(e), 120, 40));
        assert!(!without.iter().any(|r| r.contains("→")), "{without:?}");
    }

    /// **The autocomplete hangs off the action field**, one row down and past
    /// the label — where the painter put it with `x + 12, y + 1`.
    #[test]
    fn the_autocomplete_hangs_off_the_action_field() {
        let mut e = edit();
        e.autocomplete = Some(Autocomplete {
            suggestions: (0..20).map(|i| format!("action_{i}")).collect(),
            selected: Some(0),
        });
        let ui = with_dialog(Dialog::Edit(e), 120, 40);
        let rows = painted(&ui);
        assert!(
            rows.iter().any(|r| r.contains("action_0")),
            "the suggestions are there: {rows:?}"
        );
        // Twenty suggestions in a window of eight: a bar says there is more.
        assert!(
            ui.spec()
                .items
                .iter()
                .any(|i| matches!(i.draw, fresh_ui::Draw::Scrollbar { .. })),
            "a long suggestion list scrolls"
        );
    }

    /// The help overlay is a list of bindings and takes no press of its own.
    #[test]
    fn the_help_overlay_lists_its_bindings() {
        use crate::view::shell::msg::UiFact;
        let help = Dialog::Help(Help {
            title: "Help".into(),
            lines: vec![
                HelpLine {
                    key: "Navigation".into(),
                    desc: String::new(),
                    heading: true,
                },
                HelpLine {
                    key: "  ↑ / ↓".into(),
                    desc: "Move".into(),
                    heading: false,
                },
            ],
        });
        let mut ui = with_dialog(help, 120, 40);
        let rows = painted(&ui);
        assert!(rows.iter().any(|r| r.contains("Navigation")), "{rows:?}");
        assert!(rows.iter().any(|r| r.contains("Move")), "{rows:?}");
        let got = facts(ui.dispatch(fresh_ui::Input::press(
            fresh_ui::Point::new(60, 20),
            fresh_ui::MouseButton::Left,
            fresh_ui::Mods::NONE,
        )));
        assert!(
            !got.iter().any(|f| matches!(f, UiFact::KeybindingDialog(_))),
            "{got:?}"
        );
    }
}
