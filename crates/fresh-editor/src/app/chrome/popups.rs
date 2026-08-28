//! Info/message popups: transient-dismiss guard, per-popup OPAQUE
//! rects (absorb as a tree property) and scrollbar tracks, and the
//! double-click block guard.

use crate::app::types::HoverTarget;
use crate::input::keybindings::Action;
use crate::widgets::LayoutBox;
use anyhow::Result as AnyhowResult;

use super::{
    in_rect, ChromeComponent, ChromePointer, ChromeTreeBuilder, Disposition, Editor, PointerPress,
};

pub(crate) struct Popups;

impl ChromeComponent for Popups {
    fn collect(&self, ed: &Editor, t: &mut ChromeTreeBuilder) {
        // No dismiss guards. A transient popup's layer declares
        // `Dismiss::OUTSIDE_POINTER.passing_through()`, which is these two
        // boxes exactly: dismissed by a press outside it, and the press goes
        // on to what it was aimed at — the `PassAfter` both arms returned.
        // No scrollbar box. The bar is the viewport's, and `hit.rs` owns the
        // press on its gutter and the drag that follows — the same rail the
        // prompt's list stopped carrying.
        //
        // Popups are rect-bounded, OPAQUE surfaces: a pointer event
        // inside a popup that its handlers decline dies at the popup
        // box (the scan's opacity gate) instead of falling to content
        // beneath — absorb is a tree property, not a guard box.
        //
        // **At the shell's own rank, not above it.** `z = 150` was there to
        // beat the shell's *background* surfaces back when a popup was a
        // painted overlay and they were boxes. The popup is a layer in the
        // shell's tree now, so a rank above `SHELL_BACKGROUND_Z` would make
        // `placed_surface_outranks_shell` skip the tree for every point inside
        // a popup — its own rows, its own wheel, its own scrollbar. What is
        // left here runs as the floor beneath the tree, which is where the
        // parts that have not migrated belong.
        let opaque_popup = |t: &mut ChromeTreeBuilder, r: ratatui::layout::Rect| {
            let mut b = LayoutBox::plain(
                "chrome:popups",
                r.y as u32,
                r.x as u32,
                r.width as u32,
                r.height as u32,
            );
            b.z = super::SHELL_BACKGROUND_Z;
            b.pointer_opaque = true;
            t.push(b);
        };
        for (_, popup_rect, ..) in &ed.active_chrome().global_popup_areas {
            opaque_popup(t, *popup_rect);
        }
        for area in &ed.active_chrome().popup_areas {
            opaque_popup(t, area.1);
        }
    }

    fn hover(&self, ed: &mut Editor, bx: &LayoutBox, col: u16, row: u16) -> Option<HoverTarget> {
        if bx.kind != "chrome:popups" {
            return None;
        }
        // Check popups from top to bottom (reverse order since the
        // last popup is on top).
        for (popup_idx, _popup_rect, inner_rect, scroll_offset, num_items, _, _) in
            ed.active_chrome().popup_areas.iter().rev()
        {
            if in_rect(col, row, *inner_rect) && *num_items > 0 {
                let relative_row = (row - inner_rect.y) as usize;
                let item_idx = scroll_offset + relative_row;
                if item_idx < *num_items {
                    return Some(HoverTarget::PopupListItem(*popup_idx, item_idx));
                }
            }
        }
        None
    }

    fn on_pointer(
        &self,
        ed: &mut Editor,
        bx: &LayoutBox,
        ev: &ChromePointer,
    ) -> AnyhowResult<Disposition> {
        if ev.press == PointerPress::Left {
            return match bx.kind {
                "chrome:popups" => {
                    if let Some(r) = ed
                        .handle_click_global_popups(ev.col, ev.row)
                        .or_else(|| ed.handle_click_buffer_popups(ev.col, ev.row))
                    {
                        r?;
                        return Ok(Disposition::Consumed);
                    }
                    Ok(Disposition::Pass)
                }
                _ => Ok(Disposition::Pass),
            };
        }
        if !matches!(ev.press, PointerPress::Double | PointerPress::Triple) {
            return Ok(Disposition::Pass);
        }
        match bx.kind {
            // Double/triple-click inside a popup: BLOCK, as a consume
            // (belt over the opacity gate's suspenders — the split
            // select arms live in the walk now, so opacity alone
            // would also stop them, but an explicit block keeps the
            // guard's dismiss half unambiguous).
            "chrome:popups" => Ok(Disposition::Consumed),
            _ => Ok(Disposition::Pass),
        }
    }

    // No wheel arm. The popup's window is a viewport in the shell's tree and
    // takes its own wheel, vertical and horizontal — which is also what stops
    // a horizontal delta panning the buffer underneath, since a layer's
    // content claims the event rather than a guard absorbing it.

    fn layers(&self, ed: &Editor, out: &mut Vec<(u16, crate::app::overlay::Layer)>) {
        use crate::app::overlay::{Layer, LayerKind};
        // A non-trust popup is *present* whenever visible, but only
        // *owns* the keyboard while capturing; a merely-visible
        // unfocused popup falls through. Either way a visible popup
        // blocks PTY routing — it covers the active buffer. While the
        // workspace-trust prompt tops the global stack, its dedicated
        // layer (the modals component) takes this one's place.
        if !ed.workspace_trust_on_top()
            && (ed.global_popups.is_visible() || ed.active_state().popups.is_visible())
        {
            out.push((
                super::layer_rank::POPUP,
                Layer {
                    kind: LayerKind::Popup,
                    owns_keyboard: ed.popups_capture_keys(),
                    key_context: Some(crate::input::keybindings::KeyContext::Popup),
                    blocks_terminal_input: true,
                },
            ));
        }
    }

    fn on_layer_key(
        &self,
        ed: &mut Editor,
        _layer: &crate::app::overlay::Layer,
        event: &crossterm::event::KeyEvent,
    ) -> Option<anyhow::Result<crate::input::handler::InputResult>> {
        ed.dispatch_popup_keys(event)
    }
}

/// Behavior owned by this component (moved from mouse_input.rs —
/// the handlers its arms dispatch to).
impl Editor {
    /// Keyboard for the popup layer (the rungs of
    /// `dispatch_modal_input`'s popup block plus `handle_key`'s
    /// unfocused-popup interception, moved here — offered by the
    /// layer walk when it reaches the Popup layer).
    ///
    /// The unfocused rung runs first: a merely-visible popup doesn't
    /// capture the keyboard, but the user's bound popup-cancel
    /// (default Esc) and popup-focus (default Alt+T) keys must still
    /// affect it. `resolve_unfocused_popup_action` keeps its internal
    /// `popup_blocked_by_higher_modal` guard DELIBERATELY: the Prompt
    /// layer above declines keys its handler ignores (walk
    /// fall-through is broader than its `owns_keyboard` claim), and
    /// the old pipeline ran this interception before the prompt block
    /// only when no higher layer owned the keyboard — the guard is
    /// what keeps that precedence byte-identical on the walk.
    ///
    /// The capturing rungs mirror the old block exactly: completion
    /// resolver → global popups → buffer popups, with the global
    /// rung's Ignored deliberately returning `None` without trying
    /// buffer popups (its dispatch may have queued a ClosePopup that
    /// the deferred-action processor has already fired).
    pub(super) fn dispatch_popup_keys(
        &mut self,
        event: &crossterm::event::KeyEvent,
    ) -> Option<anyhow::Result<crate::input::handler::InputResult>> {
        use crate::input::handler::{InputContext, InputHandler, InputResult};

        if let Some(action) = self.resolve_unfocused_popup_action(event) {
            return Some(self.handle_action(action).map(|_| InputResult::Consumed));
        }

        if !self.popups_capture_keys() {
            return None;
        }

        let mut ctx = InputContext::new();

        // Completion popups consult the keybinding resolver in the
        // `Completion` context first, so accept/dismiss can be remapped
        // via the keybinding editor. Falls through to the popup's own
        // handler for everything else (type-to-filter, navigation, etc.).
        if let Some(action) = self.resolve_completion_popup_action(event) {
            self.process_deferred_actions(ctx);
            if let Err(e) = self.handle_action(action) {
                tracing::warn!("Completion popup action failed: {}", e);
            }
            return Some(Ok(InputResult::Consumed));
        }

        // (The workspace-trust rung lives with the WorkspaceTrust
        // component now — its 870-ranked layer replaces this one while
        // the trust prompt tops the global stack, so the walk never
        // reaches here in that state.)

        // Editor-level (global) popups take precedence over buffer popups
        // so that plugin notifications stay focused even when the active
        // buffer owns its own popup stack.
        if self.global_popups.is_visible() {
            let result = self.global_popups.dispatch_input(event, &mut ctx);
            self.process_deferred_actions(ctx);
            if result != InputResult::Ignored {
                return Some(Ok(result));
            }
            // Re-check visibility — the dispatch may have queued a
            // ClosePopup that the deferred-action processor has now fired.
            return None;
        }

        // Popup is next
        if self.active_state().popups.is_visible() {
            let result = self
                .active_state_mut()
                .popups
                .dispatch_input(event, &mut ctx);
            self.process_deferred_actions(ctx);
            // If the popup handler returned Ignored (e.g., non-word
            // character, Ctrl+key, arrow keys), fall through to normal
            // input handling. The deferred ClosePopup action was already
            // processed above.
            if result != InputResult::Ignored {
                return Some(Ok(result));
            }
        }

        None
    }

    /// Choose a row of the topmost popup, then confirm it.
    ///
    /// The tail of `handle_click_global_popups` and `handle_click_buffer_popups`
    /// with the hit-test taken off the front — a list row that answers its own
    /// click has an index, and asking it to report a screen position so the
    /// editor can hit-test its way back to that index is the round trip the
    /// migration removes.
    ///
    /// "Topmost" is `handle_popup_confirm`'s own rule, restated so the row that
    /// is *selected* and the popup that is *confirmed* cannot be different
    /// ones: global popups win over a buffer's while any is visible.
    pub(crate) fn select_popup_item(&mut self, index: usize) {
        let set = |p: &mut crate::view::popup::Popup| {
            if let crate::view::popup::PopupContent::List { selected, .. } = &mut p.content {
                *selected = index;
            }
        };
        if self.global_popups.is_visible() {
            if let Some(p) = self.global_popups.top_mut() {
                set(p);
            }
        } else if let Some(p) = self.active_state_mut().popups.top_mut() {
            set(p);
        }
        if let Err(e) = self.handle_action(Action::PopupConfirm) {
            tracing::warn!("popup confirm failed: {e}");
        }
    }

    pub(super) fn handle_click_global_popups(
        &mut self,
        col: u16,
        row: u16,
    ) -> Option<AnyhowResult<()>> {
        for (popup_idx, popup_rect, inner_rect, scroll_offset, num_items) in self
            .active_chrome()
            .global_popup_areas
            .clone()
            .into_iter()
            .rev()
        {
            if popup_rect.width >= 5 {
                let cb_x = popup_rect.x + popup_rect.width - 4;
                if row == popup_rect.y && col >= cb_x && col < cb_x + 3 {
                    return Some(self.handle_action(Action::PopupCancel));
                }
            }
            if in_rect(col, row, inner_rect) && num_items > 0 {
                let relative_row = (row - inner_rect.y) as usize;
                let item_idx = scroll_offset + relative_row;
                if item_idx < num_items {
                    if let Some(popup) = self.global_popups.get_mut(popup_idx) {
                        if let crate::view::popup::PopupContent::List { items: _, selected } =
                            &mut popup.content
                        {
                            *selected = item_idx;
                        }
                    }
                    return Some(self.handle_action(Action::PopupConfirm));
                }
            }
        }
        None
    }

    pub(super) fn handle_click_buffer_popups(
        &mut self,
        col: u16,
        row: u16,
    ) -> Option<AnyhowResult<()>> {
        // Check close-button overlay ("[×]") on each popup.
        let close_hit = self.active_chrome().popup_areas.iter().rev().find_map(
            |(_idx, popup_rect, _inner, _scroll, _n, _sb, _tl)| {
                if popup_rect.width < 5 {
                    return None;
                }
                let cb_x = popup_rect.x + popup_rect.width - 4;
                if row == popup_rect.y && col >= cb_x && col < cb_x + 3 {
                    Some(())
                } else {
                    None
                }
            },
        );
        if close_hit.is_some() {
            return Some(self.handle_action(Action::PopupCancel));
        }

        // Content area clicks — clone to allow &mut self calls inside the loop.
        let popup_areas = self.active_chrome().popup_areas.clone();
        for (popup_idx, _popup_rect, inner_rect, scroll_offset, num_items, _, _) in
            popup_areas.iter().rev()
        {
            if !in_rect(col, row, *inner_rect) {
                continue;
            }
            let relative_col = (col - inner_rect.x) as usize;
            let relative_row = (row - inner_rect.y) as usize;

            let link_url = {
                let state = self.active_state();
                state
                    .popups
                    .top()
                    .and_then(|p| p.link_at_position(relative_col, relative_row))
            };
            if let Some(url) = link_url {
                #[cfg(feature = "runtime")]
                if let Err(e) = open::that(&url) {
                    self.set_status_message(format!("Failed to open URL: {}", e));
                } else {
                    self.set_status_message(format!("Opening: {}", url));
                }
                return Some(Ok(()));
            }

            if *num_items > 0 {
                let item_idx = scroll_offset + relative_row;
                if item_idx < *num_items {
                    let state = self.active_state_mut();
                    if let Some(popup) = state.popups.top_mut() {
                        if let crate::view::popup::PopupContent::List { items: _, selected } =
                            &mut popup.content
                        {
                            *selected = item_idx;
                        }
                    }
                    return Some(self.handle_action(Action::PopupConfirm));
                }
            }

            let is_text_popup = {
                let state = self.active_state();
                state.popups.top().is_some_and(|p| {
                    matches!(
                        p.content,
                        crate::view::popup::PopupContent::Text(_)
                            | crate::view::popup::PopupContent::Markdown(_)
                    )
                })
            };
            if is_text_popup {
                let line = scroll_offset + relative_row;
                let popup_idx_copy = *popup_idx;
                let state = self.active_state_mut();
                if let Some(popup) = state.popups.top_mut() {
                    popup.start_selection(line, relative_col);
                }
                self.active_window_mut().mouse_state.selecting_in_popup = Some(popup_idx_copy);
                return Some(Ok(()));
            }
        }
        None
    }

    /// Popup text-selection drag (`PointerGrab::PopupSelect`): extend
    /// the selection to the pointer within the grabbed popup.
    pub(crate) fn handle_popup_select_drag(&mut self, col: u16, row: u16) {
        if let Some(popup_idx) = self.active_window_mut().mouse_state.selecting_in_popup {
            // Find the popup area from cached layout
            if let Some((_, _, inner_rect, scroll_offset, _, _, _)) = self
                .active_chrome()
                .popup_areas
                .iter()
                .find(|(idx, _, _, _, _, _, _)| *idx == popup_idx)
            {
                // Check if mouse is within the popup inner area
                if col >= inner_rect.x
                    && col < inner_rect.x + inner_rect.width
                    && row >= inner_rect.y
                    && row < inner_rect.y + inner_rect.height
                {
                    let relative_col = (col - inner_rect.x) as usize;
                    let relative_row = (row - inner_rect.y) as usize;
                    let line = scroll_offset + relative_row;

                    let state = self.active_state_mut();
                    if let Some(popup) = state.popups.get_mut(popup_idx) {
                        popup.extend_selection(line, relative_col);
                    }
                }
            }
        }
    }
}
