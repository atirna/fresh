//! Settings UI renderer
//!
//! Renders the settings modal with category navigation and setting controls.

use fresh_i18n::t;

use super::entry_dialog::EntryDialogState;
use super::items::SettingControl;
use super::layout::{SettingsHit, SettingsLayout};
use super::search::{DeepMatch, SearchResult};
use super::state::SettingsState;
use crate::view::theme::Theme;
use crate::view::ui::scrollbar::{render_scrollbar, ScrollbarColors, ScrollbarState};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Truncate `s` to at most `max_chars` characters, appending `"..."` if it
/// was actually shortened. Counts characters (not bytes) so non-ASCII
/// inputs (CJK descriptions, emoji, etc.) don't byte-slice through a
/// multi-byte UTF-8 sequence and panic — same class as #1718.
fn truncate_chars_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", kept)
    }
}


/// Render the settings modal
/// Render the settings dialog into the box the tree placed.
///
/// `modal_area` used to be computed here — ninety percent of `area`, capped at
/// 160 columns, centred with `area.x` and `area.y` added back so the dock did
/// not over-draw its left edge — and then filed in `SettingsLayout::modal_area`
/// for the mouse handler to measure every other rectangle from. It is
/// `view::shell::settings`'s now, and this is handed the answer.
///
/// `area` is still needed for one thing: the too-small message, which is not
/// the dialog and does not go where the dialog would have.
pub fn render_settings(
    frame: &mut Frame,
    area: Rect,
    modal_area: Rect,
    panel_area: Option<Rect>,
    items_area: Option<Rect>,
    state: &mut SettingsState,
    theme: &Theme,
) -> SettingsLayout {
    // Minimum size guard — prevent panics from zero-sized layout arithmetic.
    // The tree applies the same guard by placing no box; this is what it looks
    // like when it did.
    if modal_area.width == 0 || modal_area.height == 0 {
        let msg = "[Terminal too small for settings]";
        let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
        let y = area.y + area.height / 2;
        if area.width > 0 && area.height > 0 {
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(theme.diagnostic_warning_fg)),
                Rect::new(x, y, msg.len() as u16, 1),
            );
        }
        return SettingsLayout::new(Rect::ZERO);
    }

    // Clear the modal area and draw border
    frame.render_widget(Clear, modal_area);

    let title = if state.has_changes() {
        format!(" Settings [{}] • (modified) ", state.target_layer_name())
    } else {
        format!(" Settings [{}] ", state.target_layer_name())
    };

    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.popup_border_fg))
        .style(Style::default().bg(theme.popup_bg));
    frame.render_widget(block, modal_area);

    // Inner area after border
    let inner_area = Rect::new(
        modal_area.x + 1,
        modal_area.y + 1,
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );

    // Determine layout mode: vertical (narrow) vs horizontal (wide)
    // Narrow mode when inner width < 60 columns
    let narrow_mode = inner_area.width < 60;

    // Always render search bar at the top (1 line height to avoid layout
    // jump), with a 1-row blank gap below it so the bar reads as a header
    // rather than running into the panels.
    let search_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, 1);
    let search_header_height = 1u16;
    let search_gap = 1u16;
    // **The search row is the tree's** (`view::shell::settings`): the query
    // was already a `WidgetSpec::Text` rendered through `render_spec`, so it
    // is a *node* now, through the same adapter a plugin's field goes through.
    // `search_area` stays because everything below it is measured from here.
    let _ = search_area;

    // Footer height: 2 lines for horizontal (separator + buttons), 7 for vertical
    let footer_height = if narrow_mode { 7 } else { 2 };
    let chrome_height = search_header_height + search_gap + footer_height;
    let content_area = Rect::new(
        inner_area.x,
        inner_area.y + search_header_height + search_gap,
        inner_area.width,
        inner_area.height.saturating_sub(chrome_height),
    );

    // Create layout tracker
    let mut layout = SettingsLayout::new(modal_area);

    if narrow_mode {
        // Vertical layout: categories on top, items below
        render_vertical_layout(frame, content_area, state, theme, &mut layout);
    } else {
        // Horizontal layout: categories left, items right
        render_horizontal_layout(
            frame,
            content_area,
            panel_area,
            items_area,
            state,
            theme,
            &mut layout,
        );
    }

    // Determine the topmost dialog layer and apply dimming to layers below
    let has_confirm = state.showing_confirm_dialog;
    let has_reset = state.showing_reset_dialog;
    let has_entry = state.showing_entry_dialog();
    let has_help = state.showing_help;

    // **The confirm and reset prompts are the tree's** — layers over this box,
    // with `apply_dimming` as their scrim and their buttons answering their
    // own presses (`view::shell::settings`). They stay painted while the entry
    // stack is up, because that stack is still painted and a described prompt
    // would be a layer over it rather than under.
    if has_confirm && has_entry {
        crate::view::dimming::apply_dimming(frame, modal_area);
        render_confirm_dialog(frame, modal_area, state, theme);
    }
    if has_reset && has_entry {
        if !has_confirm {
            crate::view::dimming::apply_dimming(frame, modal_area);
        }
        render_reset_dialog(frame, modal_area, state, theme);
    }

    // Render entry dialog stack — dim between each level
    if has_entry {
        let stack_depth = state.entry_dialog_stack.len();
        for dialog_idx in 0..stack_depth {
            if !has_help || dialog_idx < stack_depth - 1 {
                crate::view::dimming::apply_dimming(frame, modal_area);
            }
            render_entry_dialog_at(frame, modal_area, state, theme, dialog_idx);
        }
    }

    // The entry dialog's two prompts are the tree's. They sit *over* the
    // entry stack, so a layer lands where they were even though the stack
    // itself is still painted (`view::shell::settings`). Both answer a press
    // now, which they never did.

    // The help overlay is the tree's too, on the same terms: painted only
    // while the entry stack is, because a layer would sit over it.
    if has_help && has_entry {
        crate::view::dimming::apply_dimming(frame, modal_area);
        render_help_overlay(frame, modal_area, theme);
    }

    layout
}

/// Render horizontal layout (wide mode): categories left, items right
fn render_horizontal_layout(
    frame: &mut Frame,
    content_area: Rect,
    panel: Option<Rect>,
    items: Option<Rect>,
    state: &mut SettingsState,
    theme: &Theme,
    layout: &mut SettingsLayout,
) {
    // Layout: [left panel (categories)] | [right panel (settings)]
    // 24 cols for categories, 1 col for the divider, the rest for settings.
    let chunks = Layout::horizontal([
        Constraint::Length(24),
        Constraint::Length(1),
        Constraint::Min(40),
    ])
    .split(content_area);

    let divider_area = chunks[1];
    // **The tree is the tree's** (`view::shell::settings::categories`): a
    // `widgets::List` in the window it scrolls in, with the rows answering
    // their own presses. `render_categories` and the five families of
    // rectangle it filed are gone with it. What is left of the split here is
    // the divider between the two panes and the fallback for the frame the
    // description has not been laid out on yet.
    let settings_area = panel.unwrap_or(chunks[2]);

    // Single straight vertical line dividing categories from settings.
    let divider_style = Style::default().fg(theme.split_separator_fg);
    for y in 0..divider_area.height {
        frame.render_widget(
            Paragraph::new("│").style(divider_style),
            Rect::new(divider_area.x, divider_area.y + y, 1, 1),
        );
    }

    // 1-col gutter on each side of the settings panel for breathing room.
    let horizontal_padding = 1u16;
    let settings_inner = Rect::new(
        settings_area.x + horizontal_padding,
        settings_area.y,
        settings_area.width.saturating_sub(horizontal_padding * 2),
        settings_area.height,
    );

    // **The body is the tree's** (`view::shell::settings::items`): one card
    // per setting, in the `viewport` that scrolls them. `render_settings_panel`
    // and everything under it went with it — `ScrollablePanel`'s second walk
    // of every item's height, the `ItemBox` plan and its five `_y()`
    // accessors, the `BandViewport` each band was clipped against, and the
    // `ControlLayoutInfo` filed per control so a later click could be compared
    // against it. What is left here is the search, which replaces the body
    // with its own results.
    if state.search_active && !state.search_results.is_empty() {
        render_search_results(frame, settings_inner, state, theme, layout);
    }
    let _ = items;

    // **Both footers are the tree's** (`view::shell::settings`): one row of
    // buttons across, or five down below sixty columns. `render_footer` and
    // `render_footer_vertical` are gone, and with them the five rectangles
    // the narrow one filed for `hit_test` to compare a cell against.
}

/// Render vertical layout (narrow mode): categories on top, items below
fn render_vertical_layout(
    frame: &mut Frame,
    content_area: Rect,
    state: &mut SettingsState,
    theme: &Theme,
    layout: &mut SettingsLayout,
) {
    // Calculate footer height for vertical buttons (5 buttons + separators)
    let footer_height = 7;

    // Layout: [categories (3 lines)] / [separator] / [settings] / [footer]
    let main_height = content_area.height.saturating_sub(footer_height);
    let category_height = 3u16.min(main_height);
    let settings_height = main_height.saturating_sub(category_height + 1); // +1 for separator

    // Categories area (horizontal strip at top)
    let categories_area = Rect::new(
        content_area.x,
        content_area.y,
        content_area.width,
        category_height,
    );

    // Separator line
    let sep_y = content_area.y + category_height;

    // Settings area
    let settings_area = Rect::new(
        content_area.x,
        sep_y + 1,
        content_area.width,
        settings_height,
    );

    // Render horizontal category strip
    render_categories_horizontal(frame, categories_area, state, theme, layout);

    // Render horizontal separator
    if sep_y < content_area.y + content_area.height {
        let sep_line: String = "─".repeat(content_area.width as usize);
        frame.render_widget(
            Paragraph::new(sep_line).style(Style::default().fg(theme.split_separator_fg)),
            Rect::new(content_area.x, sep_y, content_area.width, 1),
        );
    }

    // The body below the strip is the tree's, in this layout too; only the
    // search results are still painted.
    if state.search_active && !state.search_results.is_empty() {
        render_search_results(frame, settings_area, state, theme, layout);
    }
}

/// Render categories as a horizontal strip (for narrow mode)
fn render_categories_horizontal(
    frame: &mut Frame,
    area: Rect,
    state: &SettingsState,
    theme: &Theme,
    layout: &mut SettingsLayout,
) {
    use super::state::FocusPanel;

    if area.height == 0 || area.width == 0 {
        return;
    }

    let is_focused = state.focus_panel() == FocusPanel::Categories;

    // Build category labels with indicators
    let mut spans = Vec::new();
    let mut total_width = 0u16;

    for (i, page) in state.pages.iter().enumerate() {
        let is_selected = i == state.selected_category;
        let has_modified = state.page_has_pending_changes(i);

        let indicator = if has_modified { "● " } else { "  " };
        let name = &page.name;

        let style = if is_selected && is_focused {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.menu_highlight_bg)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.popup_text_fg)
        };

        let indicator_style = if has_modified {
            Style::default().fg(theme.menu_highlight_fg)
        } else {
            style
        };

        // Add separator between categories
        if i > 0 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(theme.split_separator_fg),
            ));
            total_width += 3;
        }

        spans.push(Span::styled(indicator, indicator_style));
        spans.push(Span::styled(name.as_str(), style));
        total_width += (indicator.len() + name.len()) as u16;

        // Track category rect for click handling (approximate)
        let cat_x = area.x + total_width.saturating_sub((indicator.len() + name.len()) as u16);
        let cat_width = (indicator.len() + name.len()) as u16;
        layout
            .categories
            .push((i, Rect::new(cat_x, area.y, cat_width, 1)));
    }

    // Render the category line
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);

    // Show navigation hint on line 2 if space
    if area.height >= 2 {
        let hint = "←→: Switch category";
        let hint_style = Style::default().fg(theme.line_number_fg);
        frame.render_widget(
            Paragraph::new(hint).style(hint_style),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

/// Get an icon for a settings category name.
///
/// Two sets are available. The Nerd Font set uses private-use-area
/// codepoints that require a patched "Nerd Font" in the terminal — PUA
/// glyphs have no system-font fallback, so on any other font they
/// render as `?` or empty boxes (issue #2032). The default set uses
/// standard BMP codepoints (default text presentation, width 1) from
/// the same compatibility class as the `▶`/`✓`/`●` glyphs the UI
/// already relies on, so terminal font fallback can always supply
/// them. The Nerd Font set is used only when `editor.nerd_font_icons`
/// is enabled.
pub fn category_icon(name: &str, nerd_fonts: bool) -> &'static str {
    let name = name.to_lowercase();
    if nerd_fonts {
        return match name.as_str() {
            "general" => "\u{f013} ",       //
            "editor" => "\u{f044} ",        //
            "clipboard" => "\u{f328} ",     //
            "file browser" => "\u{f07b} ",  //
            "file explorer" => "\u{f07c} ", //
            "packages" => "\u{f487} ",      //
            "plugins" => "\u{f1e6} ",       //
            "terminal" => "\u{f120} ",      //
            "warnings" => "\u{f071} ",      //
            "keybindings" => "\u{f11c} ",   //
            _ => "\u{f111} ",               //  (dot circle as fallback)
        };
    }
    if name.starts_with("plugin: ") {
        return "\u{271a} "; // ✚ heavy plus (add-on)
    }
    match name.as_str() {
        "general" => "\u{2699} ",       // ⚙ gear
        "editor" => "\u{270e} ",        // ✎ pencil
        "clipboard" => "\u{2702} ",     // ✂ scissors (cut/copy)
        "file browser" => "\u{25a4} ",  // ▤ square with lines (document)
        "file explorer" => "\u{25a6} ", // ▦ square with grid (tree)
        "packages" => "\u{25c6} ",      // ◆ diamond
        "plugins" => "\u{271a} ",       // ✚ heavy plus (add-on)
        "terminal" => "\u{00bb} ",      // » prompt chevron
        "warnings" => "\u{26a0} ",      // ⚠ warning sign
        "keybindings" => "\u{2328} ",   // ⌨ keyboard
        _ => "\u{2022} ",               // • bullet as fallback
    }
}


/// Like [`render_scalar_via_widget`] but for multi-row controls: paints
/// entries starting at `skip_rows` (the settings viewport clips tall
/// controls at the top when scrolled) into `area`.
///
/// `focus_key` (usually the control's `name`) marks the widget focused so
/// the renderer paints the focus highlight and the block caret — pass it
/// when the control is actively editing (Text/JSON); pass `""` otherwise
/// (the settings chrome shows selection for the rest).
#[allow(clippy::too_many_arguments)]
fn render_control_via_widget(
    frame: &mut Frame,
    area: Rect,
    control: &SettingControl,
    name: &str,
    theme: &Theme,
    skip_rows: u16,
    focus_key: &str,
    label_width: Option<u16>,
    prev: &std::collections::HashMap<String, crate::widgets::WidgetInstanceState>,
) -> crate::widgets::RenderOutput {
    let spec = crate::view::settings::widget_map::setting_control_to_widget_aligned(
        name,
        control,
        label_width,
    );
    let out =
        crate::widgets::render_spec_no_autofocus(&spec, prev, focus_key, area.width.max(1) as u32);
    for (i, entry) in out.entries.iter().enumerate() {
        let row = i as u16;
        if row < skip_rows {
            continue;
        }
        let dst = row - skip_rows;
        if dst < area.height {
            crate::app::render::paint_text_property_entry(
                frame.buffer_mut(),
                entry,
                area.x,
                area.y + dst,
                area.width,
                theme,
                None,
            );
        }
    }
    out
}



/// Paint one entry-dialog control through the plugin widget framework.
///
/// **All that is left of it is the painting.** Every arm used to end by
/// filing a `ControlLayoutInfo` — a toggle's chip rectangle, a number's value
/// cell, a dropdown's button and one rect per open option row, a rect per map
/// entry and per text-list row — so a later click could be compared against
/// what had been drawn. The settings *body* is a description now and its
/// controls answer their own presses; what still calls this is the
/// entry-dialog stack, which paints its fields inline and hit-tests them by
/// walking its own item list.
///
/// # Arguments
/// * `name` - Setting name (for controls that render their own label)
/// * `skip_rows` - Number of rows to skip at top of control (for partial visibility)
/// * `label_width` - Optional label width for column alignment
/// * `read_only` - Whether this field is read-only (displays as plain text instead of input)
#[allow(clippy::too_many_arguments)]
fn render_control(
    frame: &mut Frame,
    area: Rect,
    control: &SettingControl,
    name: &str,
    skip_rows: u16,
    theme: &Theme,
    label_width: Option<u16>,
    read_only: bool,
    prev: &std::collections::HashMap<String, crate::widgets::WidgetInstanceState>,
) {
    // A truly read-only field (a `Key:` row in an entry dialog) is a label and
    // a value, not a control.
    if read_only {
        if let SettingControl::Text(state) = control {
            if skip_rows > 0 {
                return;
            }
            let label_w = label_width.unwrap_or(20);
            frame.render_widget(
                Paragraph::new(format!("{}: ", state.label))
                    .style(Style::default().fg(theme.editor_fg)),
                Rect::new(area.x, area.y, label_w, 1),
            );
            frame.render_widget(
                Paragraph::new(state.value()).style(Style::default().fg(theme.line_number_fg)),
                Rect::new(
                    area.x + label_w,
                    area.y,
                    area.width.saturating_sub(label_w),
                    1,
                ),
            );
            return;
        }
    }
    // A keyed widget takes its focus from `focus_key`, not from the spec's
    // `focused` flag, so only a control actually being edited paints its
    // caret and its focus band. Outside edit mode ↑↓ walks the dialog's
    // fields, and a cursor inside one would promise a movement the arrows do
    // not make.
    let focus_key = match control.is_editing() {
        true => name,
        false => "",
    };
    render_control_via_widget(
        frame,
        area,
        control,
        name,
        theme,
        skip_rows,
        focus_key,
        label_width,
        prev,
    );
    // An open dropdown's options, under the button.
    //
    // The shared widget framework surfaces them as a *floating* screen-level
    // pop-over and discards `render_dropdown`'s inline rows; the entry dialog
    // does not draw those pop-overs and reserves inline rows through
    // `SettingControl::height` instead, so relying on the widget render alone
    // leaves the reserved rows blank — which is #2765, the dropdown that
    // opened to an empty box. The settings *body* took the pop-over when it
    // became a description; this surface has not, and paints them here.
    if let SettingControl::Dropdown(state) = control {
        if state.open && skip_rows == 0 {
            let rendered = crate::widgets::render_dropdown(
                &state.options,
                state.selected as i32,
                &state.label,
                false,
                label_width.unwrap_or(0) as u32,
                true,
                state.scroll_offset as u32,
                // The dialog draws its own selection chrome and never
                // reserved the `▸ ` focus-marker gutter.
                false,
            );
            for (row_i, (_idx, entry)) in rendered.option_rows.iter().enumerate() {
                // Row 0 is the button the widget render already painted.
                let dst = 1 + row_i as u16;
                if dst >= area.height {
                    break;
                }
                crate::app::render::paint_text_property_entry(
                    frame.buffer_mut(),
                    entry,
                    area.x,
                    area.y + dst,
                    area.width,
                    theme,
                    None,
                );
            }
        }
    }
}

/// Render search results with breadcrumbs
fn render_search_results(
    frame: &mut Frame,
    area: Rect,
    state: &mut SettingsState,
    theme: &Theme,
    layout: &mut SettingsLayout,
) {
    // **The window is the tree's** (`view::shell::settings::search_window`),
    // computed from the box it places and set before this frame's description
    // was built. It was computed here instead, from the rectangle this painter
    // had been handed — which meant the search row's "(1-3 of 298)" was
    // describing a window measured on the frame before it.

    // Ensure scroll offset is valid
    if state.search_scroll_offset >= state.search_results.len() {
        state.search_scroll_offset = state.search_results.len().saturating_sub(1);
    }

    // Determine if we need a scrollbar
    let needs_scrollbar = state.search_results.len() > state.search_max_visible;
    let scrollbar_width = if needs_scrollbar { 1 } else { 0 };

    // Reserve space for scrollbar on the right
    let content_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(scrollbar_width),
        area.height,
    );

    let mut y = content_area.y;

    for (idx, result) in state
        .search_results
        .iter()
        .enumerate()
        .skip(state.search_scroll_offset)
    {
        if y >= content_area.y + content_area.height.saturating_sub(3) {
            break;
        }

        let is_selected = idx == state.selected_search_result;
        let is_hovered = matches!(state.hover_hit, Some(SettingsHit::SearchResult(i)) if i == idx);
        let item_area = Rect::new(content_area.x, y, content_area.width, 3);

        render_search_result_item(
            frame,
            item_area,
            result,
            idx,
            is_selected,
            is_hovered,
            theme,
            layout,
        );
        y += 3;
    }

    // Track search results area in layout for mouse wheel support
    layout.search_results_area = Some(content_area);

    // Render scrollbar if needed
    if needs_scrollbar {
        let scrollbar_area = Rect::new(
            area.x + area.width - 1,
            area.y,
            1,
            area.height.saturating_sub(3), // Leave space at bottom
        );

        let scrollbar_state = ScrollbarState::new(
            state.search_results.len(),
            state.search_max_visible,
            state.search_scroll_offset,
        );

        let colors = ScrollbarColors::from_theme(theme);
        render_scrollbar(
            frame.buffer_mut(),
            scrollbar_area,
            &scrollbar_state,
            &colors,
        );

        // Track scrollbar area in layout for click/drag support
        layout.search_scrollbar_area = Some(scrollbar_area);
    } else {
        layout.search_scrollbar_area = None;
    }
}

/// Render a single search result with breadcrumb. `result_index` is the
/// absolute index into the state's `search_results` (needed for hit-testing
/// because only the visible rows get registered in the layout).
#[allow(clippy::too_many_arguments)]
fn render_search_result_item(
    frame: &mut Frame,
    area: Rect,
    result: &SearchResult,
    result_index: usize,
    is_selected: bool,
    is_hovered: bool,
    theme: &Theme,
    layout: &mut SettingsLayout,
) {
    // Draw selection or hover highlight background
    if is_selected {
        // Use dedicated settings colors for selected items
        let bg_style = Style::default().bg(theme.settings_selected_bg);
        for row in 0..area.height.min(3) {
            let row_area = Rect::new(area.x, area.y + row, area.width, 1);
            frame.render_widget(Paragraph::new("").style(bg_style), row_area);
        }
    } else if is_hovered {
        // Subtle hover highlight using menu hover colors
        let bg_style = Style::default().bg(theme.menu_hover_bg);
        for row in 0..area.height.min(3) {
            let row_area = Rect::new(area.x, area.y + row, area.width, 1);
            frame.render_widget(Paragraph::new("").style(bg_style), row_area);
        }
    }

    // Determine display name and description based on deep match
    let (display_name, display_desc) = match &result.deep_match {
        Some(DeepMatch::MapKey { key, .. }) => (key.clone(), Some(result.item.name.clone())),
        Some(DeepMatch::MapValue {
            matched_text, key, ..
        }) => (
            matched_text.clone(),
            Some(format!("{} > {}", result.item.name, key)),
        ),
        Some(DeepMatch::TextListItem { text, .. }) => {
            (text.clone(), Some(result.item.name.clone()))
        }
        None => (result.item.name.clone(), result.item.description.clone()),
    };

    // First line: Setting name with highlighting
    let name_style = if is_selected {
        Style::default().fg(theme.settings_selected_fg)
    } else if is_hovered {
        Style::default().fg(theme.menu_hover_fg)
    } else {
        Style::default().fg(theme.popup_text_fg)
    };

    // Build name with match highlighting, prefixed with selection indicator
    let indicator = if is_selected { "▸ " } else { "  " };
    let indicator_style = if is_selected {
        Style::default()
            .fg(theme.settings_selected_fg)
            .add_modifier(Modifier::BOLD)
    } else {
        name_style
    };
    let mut name_line = build_highlighted_text(
        &display_name,
        &result.name_matches,
        name_style,
        Style::default()
            .fg(theme.diagnostic_warning_fg)
            .add_modifier(Modifier::BOLD),
    );
    name_line
        .spans
        .insert(0, Span::styled(indicator, indicator_style));
    frame.render_widget(
        Paragraph::new(name_line),
        Rect::new(area.x, area.y, area.width, 1),
    );

    // Second line: Breadcrumb
    let breadcrumb_style = Style::default()
        .fg(theme.line_number_fg)
        .add_modifier(Modifier::ITALIC);
    let breadcrumb = format!("  {} > {}", result.breadcrumb, result.item.path);
    let breadcrumb_line = Line::from(Span::styled(breadcrumb, breadcrumb_style));
    frame.render_widget(
        Paragraph::new(breadcrumb_line),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );

    // Third line: Description (if any). Counts characters (not bytes)
    // when checking and truncating: descriptions can be localized (e.g.
    // CJK translations) and a byte-based slice could land inside a
    // multi-byte UTF-8 sequence and panic — same class as #1718.
    if let Some(ref desc) = display_desc {
        let desc_style = Style::default().fg(theme.line_number_fg);
        let max_chars = (area.width as usize).saturating_sub(2);
        let truncated_desc = format!("  {}", truncate_chars_with_ellipsis(desc, max_chars));
        frame.render_widget(
            Paragraph::new(truncated_desc).style(desc_style),
            Rect::new(area.x, area.y + 2, area.width, 1),
        );
    }

    // Track this item in layout
    layout.add_search_result(result_index, result.page_index, result.item_index, area);
}

/// Build a line with highlighted match positions
fn build_highlighted_text(
    text: &str,
    matches: &[usize],
    normal_style: Style,
    highlight_style: Style,
) -> Line<'static> {
    if matches.is_empty() {
        return Line::from(Span::styled(text.to_string(), normal_style));
    }

    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_highlight = false;

    for (idx, ch) in chars.iter().enumerate() {
        let should_highlight = matches.contains(&idx);

        if should_highlight != in_highlight {
            if !current.is_empty() {
                let style = if in_highlight {
                    highlight_style
                } else {
                    normal_style
                };
                spans.push(Span::styled(current, style));
                current = String::new();
            }
            in_highlight = should_highlight;
        }

        current.push(*ch);
    }

    // Push remaining
    if !current.is_empty() {
        let style = if in_highlight {
            highlight_style
        } else {
            normal_style
        };
        spans.push(Span::styled(current, style));
    }

    Line::from(spans)
}

/// Render the unsaved changes confirmation dialog
/// Draw a centered modal dialog: clear the region, paint a rounded border in
/// `border_fg`, and return `(dialog_area, inner)` where `inner` is the
/// 2-column / 1-row padded content rect. Shared by every settings confirm
/// dialog so the centering, border, and inset math live in one place.
fn centered_dialog_frame(
    frame: &mut Frame,
    parent_area: Rect,
    width: u16,
    height: u16,
    title: String,
    border_fg: Color,
    theme: &Theme,
) -> (Rect, Rect) {
    let dialog_x = parent_area.x + (parent_area.width.saturating_sub(width)) / 2;
    let dialog_y = parent_area.y + (parent_area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_fg))
        .style(Style::default().bg(theme.popup_bg));
    frame.render_widget(block, dialog_area);

    let inner = Rect::new(
        dialog_area.x + 2,
        dialog_area.y + 1,
        dialog_area.width.saturating_sub(4),
        dialog_area.height.saturating_sub(2),
    );
    (dialog_area, inner)
}

/// Render the standard one-line key-hint footer just below the button row.
fn render_dialog_help(frame: &mut Frame, inner: Rect, button_y: u16, help: &str, theme: &Theme) {
    frame.render_widget(
        Paragraph::new(help.to_string()).style(Style::default().fg(theme.line_number_fg)),
        Rect::new(inner.x, button_y + 1, inner.width, 1),
    );
}

/// List the pending-change descriptions as bulleted, width-truncated lines
/// starting at `start_y`. Character-based truncation (rather than byte
/// truncation) keeps CJK / emoji descriptions from slicing through a
/// multi-byte UTF-8 sequence and panicking — same class as #1718.
fn render_change_list(
    frame: &mut Frame,
    inner: Rect,
    start_y: u16,
    changes: &[String],
    dialog_height: u16,
    theme: &Theme,
) {
    let change_style = Style::default().fg(theme.popup_text_fg);
    for (i, change) in changes
        .iter()
        .take((dialog_height as usize).saturating_sub(7))
        .enumerate()
    {
        let max_chars = (inner.width as usize).saturating_sub(2);
        let truncated = format!("• {}", truncate_chars_with_ellipsis(change, max_chars));
        frame.render_widget(
            Paragraph::new(truncated).style(change_style),
            Rect::new(inner.x, start_y + i as u16, inner.width, 1),
        );
    }
}

/// Render a centered row of `[ label ]` choice buttons using the menu
/// highlight/hover palette. The selected button is prefixed with `>` and bold;
/// a hovered (but unselected) button uses the hover palette. Shared by the
/// unsaved-changes and reset confirm dialogs.
fn render_choice_buttons(
    frame: &mut Frame,
    inner: Rect,
    button_y: u16,
    options: &[String],
    selected: usize,
    hover: Option<usize>,
    theme: &Theme,
) {
    let total_width: u16 = options.iter().map(|o| o.len() as u16 + 4).sum::<u16>() + 4; // +4 for gaps
    let mut x = inner.x + (inner.width.saturating_sub(total_width)) / 2;

    for (idx, label) in options.iter().enumerate() {
        let is_selected = idx == selected;
        let is_hovered = hover == Some(idx);
        let button_width = label.len() as u16 + 4;

        let style = if is_selected {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.menu_highlight_bg)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::default()
                .fg(theme.menu_hover_fg)
                .bg(theme.menu_hover_bg)
        } else {
            Style::default().fg(theme.popup_text_fg)
        };

        let text = if is_selected {
            format!(">[ {} ]", label)
        } else {
            format!(" [ {} ]", label)
        };
        frame.render_widget(
            Paragraph::new(text).style(style),
            Rect::new(x, button_y, button_width + 1, 1),
        );

        x += button_width + 3;
    }
}

fn render_confirm_dialog(
    frame: &mut Frame,
    parent_area: Rect,
    state: &SettingsState,
    theme: &Theme,
) {
    let changes = state.get_change_descriptions();
    let dialog_width = 50.min(parent_area.width.saturating_sub(4));
    // Base height: 2 borders + 2 prompt lines + 1 separator + 1 buttons + 1 help = 7
    // Plus one line per change
    let dialog_height = (7 + changes.len() as u16)
        .min(20)
        .min(parent_area.height.saturating_sub(4));

    let title = format!(" {} ", t!("confirm.unsaved_changes_title"));
    let (dialog_area, inner) = centered_dialog_frame(
        frame,
        parent_area,
        dialog_width,
        dialog_height,
        title,
        theme.diagnostic_warning_fg,
        theme,
    );

    // Prompt text
    let prompt = t!("confirm.unsaved_changes_prompt").to_string();
    frame.render_widget(
        Paragraph::new(prompt).style(Style::default().fg(theme.popup_text_fg)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    render_change_list(frame, inner, inner.y + 2, &changes, dialog_height, theme);

    let button_y = dialog_area.y + dialog_area.height - 3;

    // Draw separator
    let sep_line: String = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(sep_line).style(Style::default().fg(theme.split_separator_fg)),
        Rect::new(inner.x, button_y - 1, inner.width, 1),
    );

    let options = [
        t!("confirm.save_and_exit").to_string(),
        t!("confirm.discard").to_string(),
        t!("confirm.cancel").to_string(),
    ];
    render_choice_buttons(
        frame,
        inner,
        button_y,
        &options,
        state.confirm_dialog_selection,
        state.confirm_dialog_hover,
        theme,
    );
    render_dialog_help(
        frame,
        inner,
        button_y,
        "←/→/Tab: Select   Enter: Confirm   Esc: Cancel",
        theme,
    );
}

/// Render the reset confirmation dialog
fn render_reset_dialog(frame: &mut Frame, parent_area: Rect, state: &SettingsState, theme: &Theme) {
    let changes = state.get_change_descriptions();
    let dialog_width = 50.min(parent_area.width.saturating_sub(4));
    // Base height: 2 borders + 2 prompt lines + 1 separator + 1 buttons + 1 help = 7
    // Plus one line per change
    let dialog_height = (7 + changes.len() as u16)
        .min(20)
        .min(parent_area.height.saturating_sub(4));

    let (dialog_area, inner) = centered_dialog_frame(
        frame,
        parent_area,
        dialog_width,
        dialog_height,
        " Reset All Changes ".to_string(),
        theme.diagnostic_warning_fg,
        theme,
    );

    // Prompt text
    frame.render_widget(
        Paragraph::new("Discard all pending changes?")
            .style(Style::default().fg(theme.popup_text_fg)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    render_change_list(frame, inner, inner.y + 2, &changes, dialog_height, theme);

    let button_y = dialog_area.y + dialog_area.height - 3;

    // Draw separator
    let sep_line: String = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(sep_line).style(Style::default().fg(theme.split_separator_fg)),
        Rect::new(inner.x, button_y - 1, inner.width, 1),
    );

    let options = ["Reset".to_string(), "Cancel".to_string()];
    render_choice_buttons(
        frame,
        inner,
        button_y,
        &options,
        state.reset_dialog_selection,
        state.reset_dialog_hover,
        theme,
    );
    render_dialog_help(
        frame,
        inner,
        button_y,
        "←/→/Tab: Select   Enter: Confirm   Esc: Cancel",
        theme,
    );
}

/// Compute the footer Delete-button label for an entry dialog.
///
/// Schema-driven: shows the map key for map entries (e.g.
/// `[ Delete "rust" ]`), a generic "item" for array items (the
/// numeric index isn't meaningful to the user), or a bare fallback
/// when neither is available. The key is truncated so a very long
/// identifier can't blow out the dialog footer.
fn entry_delete_button_label(dialog: &EntryDialogState) -> String {
    const MAX_KEY_IN_LABEL: usize = 24;
    if dialog.is_array_item {
        "[ Delete item ]".to_string()
    } else if dialog.entry_key.is_empty() {
        "[ Delete entry ]".to_string()
    } else {
        let key = if dialog.entry_key.chars().count() > MAX_KEY_IN_LABEL {
            let truncated: String = dialog
                .entry_key
                .chars()
                .take(MAX_KEY_IN_LABEL - 1)
                .collect();
            format!("{}…", truncated)
        } else {
            dialog.entry_key.clone()
        };
        format!("[ Delete \"{}\" ]", key)
    }
}

/// Render a specific entry dialog from the stack by index.
fn render_entry_dialog_at(
    frame: &mut Frame,
    parent_area: Rect,
    state: &mut SettingsState,
    theme: &Theme,
    dialog_idx: usize,
) {
    let Some(dialog) = state.entry_dialog_stack.get_mut(dialog_idx) else {
        return;
    };
    render_entry_dialog_inner(frame, parent_area, dialog, theme);
}

/// Render the scrolled list of items and (when needed) the scrollbar.
#[allow(clippy::too_many_arguments)]
fn render_entry_items(
    frame: &mut Frame,
    dialog_area: Rect,
    inner: Rect,
    dialog: &super::entry_dialog::EntryDialogState,
    theme: &Theme,
    label_col_width: u16,
    scroll_offset: usize,
    total_content_height: usize,
    viewport_height: usize,
) {
    let needs_scroll = total_content_height > viewport_height;
    let mut content_y: usize = 0;
    let mut screen_y = inner.y;

    let first_editable = dialog.first_editable_index;
    let needs_separator = first_editable > 0 && first_editable < dialog.items.len();

    for (idx, item) in dialog.items.iter().enumerate() {
        // Separator between read-only and editable sections
        if needs_separator && idx == first_editable {
            let separator_end = content_y + 1;
            if separator_end > scroll_offset
                && screen_y < inner.y + inner.height
                && content_y >= scroll_offset
            {
                let sep_style = Style::default().fg(theme.line_number_fg);
                let separator_line = "─".repeat(inner.width.saturating_sub(2) as usize);
                frame.render_widget(
                    Paragraph::new(separator_line).style(sep_style),
                    Rect::new(inner.x + 1, screen_y, inner.width.saturating_sub(2), 1),
                );
                screen_y += 1;
            }
            content_y = separator_end;
        }

        // Section header (2 logical rows: title + blank spacer below)
        if item.is_section_start {
            if let Some(ref section_name) = item.section {
                let header_start = content_y;
                let header_end = content_y + 2;
                if header_end > scroll_offset && screen_y < inner.y + inner.height {
                    let skip_h = header_start.saturating_sub(scroll_offset) as u16;
                    if skip_h == 0 {
                        let section_style = Style::default()
                            .fg(theme.line_number_fg)
                            .add_modifier(Modifier::BOLD);
                        frame.render_widget(
                            Paragraph::new(format!("── {} ──", section_name)).style(section_style),
                            Rect::new(inner.x + 1, screen_y, inner.width.saturating_sub(2), 1),
                        );
                        screen_y += 1;
                    }
                    if skip_h <= 1 && screen_y < inner.y + inner.height {
                        screen_y += 1; // blank line after header
                    }
                }
                content_y = header_end;
            }
        }

        let control_height = item.control.control_height() as usize;
        let item_start = content_y;
        let item_end = content_y + control_height;

        if item_end <= scroll_offset {
            content_y = item_end;
            continue;
        }
        if screen_y >= inner.y + inner.height {
            break;
        }

        let skip_rows = if item_start < scroll_offset {
            (scroll_offset - item_start) as u16
        } else {
            0
        };
        let visible_height = control_height.saturating_sub(skip_rows as usize);
        let available_height = (inner.y + inner.height).saturating_sub(screen_y) as usize;
        let render_height = visible_height.min(available_height);

        if render_height == 0 {
            content_y = item_end;
            continue;
        }

        let is_readonly = item.read_only;
        let is_focused = !is_readonly && !dialog.focus_on_buttons && dialog.selected_item == idx;
        let is_hovered = !is_readonly && dialog.hover_item == Some(idx);

        if is_focused || is_hovered {
            let bg_style = if is_focused {
                Style::default().bg(theme.settings_selected_bg)
            } else {
                Style::default().bg(theme.menu_hover_bg)
            };
            if item.control.is_composite() {
                let sub_row = item.control.focused_sub_row();
                if sub_row >= skip_rows && (sub_row - skip_rows) < render_height as u16 {
                    let highlight_y = screen_y + sub_row - skip_rows;
                    frame.render_widget(
                        Paragraph::new("").style(bg_style),
                        Rect::new(inner.x, highlight_y, inner.width, 1),
                    );
                }
            } else {
                for row in 0..render_height as u16 {
                    frame.render_widget(
                        Paragraph::new("").style(bg_style),
                        Rect::new(inner.x, screen_y + row, inner.width, 1),
                    );
                }
            }
        }

        // Indicator column: [>] focus  [●] modified  [ ] spacer
        let focus_indicator_width: u16 = 3;
        if is_focused {
            let indicator_y = if item.control.is_composite() {
                let sub_row = item.control.focused_sub_row();
                let visible_sub = sub_row.saturating_sub(skip_rows);
                if visible_sub < render_height as u16 {
                    screen_y + visible_sub
                } else {
                    screen_y
                }
            } else {
                screen_y
            };
            if indicator_y >= screen_y && indicator_y < screen_y + render_height as u16 {
                let indicator_style = Style::default()
                    .fg(theme.settings_selected_fg)
                    .add_modifier(Modifier::BOLD);
                frame.render_widget(
                    Paragraph::new(">").style(indicator_style),
                    Rect::new(inner.x, indicator_y, 1, 1),
                );
            }
        }
        if item.modified && skip_rows == 0 {
            let modified_style = Style::default().fg(theme.settings_selected_fg);
            frame.render_widget(
                Paragraph::new("●").style(modified_style),
                Rect::new(inner.x + 1, screen_y, 1, 1),
            );
        }

        let control_area = Rect::new(
            inner.x + focus_indicator_width,
            screen_y,
            inner.width.saturating_sub(focus_indicator_width),
            render_height as u16,
        );
        render_control(
            frame,
            control_area,
            &item.control,
            &item.name,
            skip_rows,
            theme,
            Some(label_col_width.saturating_sub(focus_indicator_width)),
            item.read_only,
            // Entry-dialog controls will carry their own runtime store once
            // that path is mounted; until then render statelessly (empty prev).
            &std::collections::HashMap::new(),
        );

        // Per-field affordances on the control's first row at the right edge:
        // a dim `(Inherited)` badge when the value is inherited, otherwise the
        // applicable action buttons (`[Reset]` to the built-in default and/or
        // `[Inherit]` to the global/parent value). A field only offers the
        // action(s) that lead to a different result (issue #2345). Hit-testing
        // mirrors this geometry in `handle_entry_dialog_item_click`.
        if !item.read_only && skip_rows == 0 && control_area.width > 0 {
            let right_edge = control_area.x.saturating_add(control_area.width);
            let inherits = dialog
                .inheritable_fields
                .contains(item.path.trim_start_matches('/'));
            if item.nullable && item.is_null {
                // Only show the "(Inherited)" badge when the unset value really
                // does inherit a parent value; a clear-only field (e.g. a
                // formatter) just reads as empty/not-set.
                if inherits {
                    let badge = t!("settings.inherited_badge").to_string();
                    let w = badge.chars().count() as u16 + 1;
                    let x = right_edge.saturating_sub(w);
                    if x > control_area.x {
                        frame.render_widget(
                            Paragraph::new(badge).style(
                                Style::default()
                                    .fg(theme.line_number_fg)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                            Rect::new(x, screen_y, w, 1),
                        );
                    }
                }
            } else {
                let buttons = dialog.field_action_buttons(idx);
                let positions =
                    super::entry_dialog::layout_field_action_buttons(&buttons, right_edge);
                let focused = if dialog.selected_item == idx {
                    dialog.field_button_focus
                } else {
                    None
                };
                for (bi, ((_, label), (_, x, w))) in
                    buttons.iter().zip(positions.iter()).enumerate()
                {
                    if *x <= control_area.x {
                        continue;
                    }
                    let style = if Some(bi) == focused {
                        Style::default()
                            .fg(theme.menu_hover_fg)
                            .bg(theme.menu_hover_bg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.line_number_fg)
                    };
                    frame.render_widget(
                        Paragraph::new(label.clone()).style(style),
                        Rect::new(*x, screen_y, *w, 1),
                    );
                }
            }
        }

        screen_y += render_height as u16;
        content_y = item_end;
    }

    if needs_scroll {
        let scrollbar_x = dialog_area.x + dialog_area.width - 3;
        let scrollbar_area = Rect::new(scrollbar_x, inner.y, 1, inner.height);
        let scrollbar_state =
            ScrollbarState::new(total_content_height, viewport_height, scroll_offset);
        let scrollbar_colors = ScrollbarColors::from_theme(theme);
        render_scrollbar(
            frame.buffer_mut(),
            scrollbar_area,
            &scrollbar_state,
            &scrollbar_colors,
        );
    }
}

/// Render the Save / Cancel / Delete button row.
///
/// Order: [Save] [Cancel]  [Delete …] — Delete is separated by a wider gap so
/// the destructive action cannot be reached by accidentally pressing Tab one
/// extra time.  Delete uses a per-entry label (map key or generic "item") so
/// the user knows what will be removed before committing.
fn render_entry_buttons(
    frame: &mut Frame,
    dialog_area: Rect,
    dialog: &super::entry_dialog::EntryDialogState,
    theme: &Theme,
) {
    let button_y = dialog_area.y + dialog_area.height - 2;
    let has_delete = !dialog.is_new && !dialog.no_delete;
    let delete_label = entry_delete_button_label(dialog);
    let buttons: Vec<String> = if has_delete {
        vec![
            "[ Save ]".to_string(),
            "[ Cancel ]".to_string(),
            delete_label,
        ]
    } else {
        vec!["[ Save ]".to_string(), "[ Cancel ]".to_string()]
    };
    let delete_idx = if has_delete {
        Some(buttons.len() - 1)
    } else {
        None
    };

    const BUTTON_GAP: u16 = 2;
    const DELETE_GAP: u16 = 6;
    let button_width: u16 = buttons
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let gap = if Some(i) == delete_idx {
                DELETE_GAP
            } else if i == 0 {
                0
            } else {
                BUTTON_GAP
            };
            b.len() as u16 + gap
        })
        .sum();
    let button_x = dialog_area.x + (dialog_area.width.saturating_sub(button_width)) / 2;

    let mut x = button_x;
    for (idx, label) in buttons.iter().enumerate() {
        let is_selected = dialog.focus_on_buttons && dialog.focused_button == idx;
        let is_hovered = dialog.hover_button == Some(idx);
        let is_delete = Some(idx) == delete_idx;

        if idx > 0 {
            x += if is_delete { DELETE_GAP } else { BUTTON_GAP };
        }
        if is_selected {
            let indicator_style = Style::default()
                .fg(theme.settings_selected_fg)
                .add_modifier(Modifier::BOLD);
            frame.render_widget(
                Paragraph::new(">").style(indicator_style),
                Rect::new(x.saturating_sub(2), button_y, 1, 1),
            );
        }

        // Selected Delete keeps red fg as a "still destructive" cue while
        // REVERSED signals keyboard focus — consistent with other selected items.
        let style = if is_selected && is_delete {
            Style::default()
                .fg(theme.diagnostic_error_fg)
                .bg(theme.popup_selection_bg)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_selected {
            Style::default()
                .fg(theme.popup_selection_fg)
                .bg(theme.popup_selection_bg)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_hovered && is_delete {
            Style::default()
                .fg(theme.diagnostic_error_fg)
                .bg(theme.menu_hover_bg)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::default()
                .fg(theme.menu_hover_fg)
                .bg(theme.menu_hover_bg)
        } else if is_delete {
            Style::default()
                .fg(theme.diagnostic_error_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.editor_fg)
        };

        frame.render_widget(
            Paragraph::new(label.as_str()).style(style),
            Rect::new(x, button_y, label.len() as u16, 1),
        );
        x += label.len() as u16;
    }
}

/// Render the field-description hint (row above buttons) and the keybinding
/// legend (row below buttons) at the bottom of the entry dialog.
fn render_entry_footer(
    frame: &mut Frame,
    dialog_area: Rect,
    inner: Rect,
    dialog: &super::entry_dialog::EntryDialogState,
    theme: &Theme,
) {
    let button_y = dialog_area.y + dialog_area.height - 2;
    let helper_y = button_y.saturating_sub(1);

    // One line of contextual help immediately above the buttons.
    if !dialog.focus_on_buttons && helper_y > inner.y {
        // When the cursor is on a TextList's "[+] Add new" row the focused
        // item slot is None; surface a caption that names what Enter/Esc do
        // rather than silently absorbing keystrokes.
        let pending_list_caption = dialog.current_item().and_then(|it| {
            if let SettingControl::TextList(state) = &it.control {
                if state.focused_item.is_none() {
                    return Some(if !state.pending_active && state.new_item_text.is_empty() {
                        "Press Enter (or type) to add a new item; ↓/Tab to leave"
                    } else if state.new_item_text.is_empty() {
                        "Type the new item — Enter to add, Esc to cancel"
                    } else {
                        "Editing new item — Enter to add, Esc to cancel"
                    });
                }
            }
            None
        });

        let text: Option<String> = pending_list_caption.map(String::from).or_else(|| {
            dialog
                .current_item()
                .and_then(|it| it.description.as_deref())
                .filter(|d| !d.is_empty())
                .map(String::from)
        });

        if let Some(text) = text {
            let max_width = dialog_area.width.saturating_sub(4) as usize;
            let truncated: String = text.chars().take(max_width).collect();
            let helper_style = Style::default()
                .fg(theme.line_number_fg)
                .add_modifier(Modifier::ITALIC);
            frame.render_widget(
                Paragraph::new(truncated).style(helper_style),
                Rect::new(
                    dialog_area.x + 2,
                    helper_y,
                    dialog_area.width.saturating_sub(4),
                    1,
                ),
            );
        }
    }

    // Keybinding legend / validation warning on the row below the buttons.
    let is_editing_json = dialog.editing_text && dialog.is_editing_json();
    let (has_invalid_json, is_json_control) = dialog
        .current_item()
        .map(|item| match &item.control {
            SettingControl::Text(state) => (!state.is_valid(), false),
            SettingControl::Json(state) => (!state.is_valid(), is_editing_json),
            _ => (false, false),
        })
        .unwrap_or((false, false));

    let help_area = Rect::new(
        dialog_area.x + 2,
        button_y + 1,
        dialog_area.width.saturating_sub(4),
        1,
    );

    let (text, style) = if has_invalid_json && !is_json_control {
        (
            "⚠ Invalid JSON - fix before leaving field",
            Style::default().fg(theme.diagnostic_warning_fg),
        )
    } else if has_invalid_json {
        (
            "⚠ Invalid JSON",
            Style::default().fg(theme.diagnostic_warning_fg),
        )
    } else if is_json_control {
        (
            "↑↓←→:Move  Enter:Newline  Tab/Esc:Exit",
            Style::default().fg(theme.line_number_fg),
        )
    } else if dialog.editing_text {
        (
            "Enter/Tab:Commit field  Esc:Cancel",
            Style::default().fg(theme.line_number_fg),
        )
    } else {
        // The `●:modified` legend is the only place that explains the row-indicator.
        (
            "↑↓:Navigate  Tab:Fields/Buttons  Enter:Edit/Apply  Ctrl+S:Save  Esc:Cancel  ●:modified",
            Style::default().fg(theme.line_number_fg),
        )
    };
    frame.render_widget(Paragraph::new(text).style(style), help_area);
}

/// Draw the entry-edit dialog into `parent_area`.
fn render_entry_dialog_inner(
    frame: &mut Frame,
    parent_area: Rect,
    dialog: &mut super::entry_dialog::EntryDialogState,
    theme: &Theme,
) {
    let dialog_width = (parent_area.width * 85 / 100).clamp(50, 90);
    let dialog_height = (parent_area.height * 90 / 100).max(15);
    let dialog_x = parent_area.x + (parent_area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = parent_area.y + (parent_area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    // Title shows "• modified" when the form has uncommitted edits.
    let title = if dialog.is_dirty() {
        format!(" {} • modified ", dialog.title)
    } else {
        format!(" {} ", dialog.title)
    };
    let border_color = if dialog.is_dirty() {
        theme.diagnostic_warning_fg
    } else {
        theme.popup_border_fg
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.popup_bg));
    frame.render_widget(block, dialog_area);

    // Reserve 2 lines at the bottom for the button row + keybinding hint.
    let inner = Rect::new(
        dialog_area.x + 2,
        dialog_area.y + 1,
        dialog_area.width.saturating_sub(4),
        dialog_area.height.saturating_sub(5),
    );

    let max_label_width = (inner.width / 2).max(20);
    let label_col_width = dialog
        .items
        .iter()
        .map(|item| item.name.len() as u16 + 2)
        .filter(|&w| w <= max_label_width)
        .max()
        .unwrap_or(20)
        .min(max_label_width);

    let total_content_height = dialog.total_content_height();
    let viewport_height = inner.height as usize;
    dialog.viewport_height = viewport_height;
    let scroll_offset = dialog.scroll_offset;

    render_entry_items(
        frame,
        dialog_area,
        inner,
        dialog,
        theme,
        label_col_width,
        scroll_offset,
        total_content_height,
        viewport_height,
    );
    render_entry_buttons(frame, dialog_area, dialog, theme);
    render_entry_footer(frame, dialog_area, inner, dialog, theme);
}

/// Render the help overlay showing keyboard shortcuts
fn render_help_overlay(frame: &mut Frame, parent_area: Rect, theme: &Theme) {
    // Define the help content
    let help_items = [
        (
            "Navigation",
            vec![
                ("↑ / ↓", "Move up/down"),
                ("Tab", "Switch between categories and settings"),
                ("Enter", "Activate/toggle setting"),
            ],
        ),
        (
            "Search",
            vec![
                ("/", "Start search"),
                ("Esc", "Cancel search"),
                ("↑ / ↓", "Navigate results"),
                ("Enter", "Jump to result"),
            ],
        ),
        (
            "Actions",
            vec![
                ("Ctrl+S", "Save settings"),
                ("Esc", "Close settings"),
                ("?", "Toggle this help"),
            ],
        ),
    ];

    // Calculate dialog size
    let dialog_width = 50.min(parent_area.width.saturating_sub(4));
    let dialog_height = 20.min(parent_area.height.saturating_sub(4));

    // Center the dialog
    let dialog_x = parent_area.x + (parent_area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = parent_area.y + (parent_area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    // Clear and draw border
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Keyboard Shortcuts ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.menu_highlight_fg))
        .style(Style::default().bg(theme.popup_bg));
    frame.render_widget(block, dialog_area);

    // Inner area
    let inner = Rect::new(
        dialog_area.x + 2,
        dialog_area.y + 1,
        dialog_area.width.saturating_sub(4),
        dialog_area.height.saturating_sub(2),
    );

    let mut y = inner.y;

    for (section_name, bindings) in &help_items {
        if y >= inner.y + inner.height.saturating_sub(1) {
            break;
        }

        // Section header
        let header_style = Style::default()
            .fg(theme.menu_active_fg)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(*section_name).style(header_style),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;

        for (key, description) in bindings {
            if y >= inner.y + inner.height.saturating_sub(1) {
                break;
            }

            let key_style = Style::default()
                .fg(theme.popup_text_fg)
                .bg(theme.split_separator_fg);
            let desc_style = Style::default().fg(theme.popup_text_fg);

            let line = Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(format!(" {} ", key), key_style),
                Span::styled(format!("  {}", description), desc_style),
            ]);
            frame.render_widget(Paragraph::new(line), Rect::new(inner.x, y, inner.width, 1));
            y += 1;
        }

        y += 1; // Blank line between sections
    }

    // Footer hint
    let footer_y = dialog_area.y + dialog_area.height - 2;
    let footer = "Press ? or Esc or Enter to close";
    let footer_style = Style::default().fg(theme.line_number_fg);
    let centered_x = inner.x + (inner.width.saturating_sub(footer.len() as u16)) / 2;
    frame.render_widget(
        Paragraph::new(footer).style(footer_style),
        Rect::new(centered_x, footer_y, footer.len() as u16, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_with_ellipsis_ascii_fits() {
        assert_eq!(truncate_chars_with_ellipsis("hi", 10), "hi");
    }

    #[test]
    fn truncate_chars_with_ellipsis_ascii_truncates() {
        assert_eq!(truncate_chars_with_ellipsis("hello world!", 8), "hello...");
    }

    #[test]
    fn truncate_chars_with_ellipsis_multibyte_does_not_panic() {
        // Regression: byte-slicing this string at `max - 3` would land
        // inside the 3-byte UTF-8 sequence for `こ` and panic — same class
        // as #1718.
        let out = truncate_chars_with_ellipsis("こんにちは世界からのテスト", 8);
        assert!(out.ends_with("..."));
        // 5 kept chars + 3 ellipsis chars = 8 total chars.
        assert_eq!(out.chars().count(), 8);
    }

    #[test]
    fn truncate_chars_with_ellipsis_emoji_does_not_panic() {
        let out = truncate_chars_with_ellipsis("📦📦📦📦📦📦📦📦", 5);
        assert!(out.ends_with("..."));
        assert_eq!(out.chars().count(), 5);
    }

    /// Regression for #2765: an *open* settings dropdown must actually paint
    /// its option rows into the frame.
    ///
    /// The shared widget framework (`collect_dropdown`) turns an open
    /// dropdown's option list into a floating screen-level pop-over and
    /// discards the inline `option_rows`. The Settings modal does not draw
    /// those floating pop-overs — it reserves inline rows for the open list —
    /// so rendering through `render_scalar_via_widget` alone left the reserved
    /// rows blank and the dropdown opened to an empty box (the Theme and every
    /// other dynamic dropdown showed no options at runtime).
    ///
    /// This drives the real `render_control` paint path (not a hand-built
    /// widget spec) and asserts the option names land in the painted buffer
    /// and that per-option hit rects are produced.
    #[test]
    fn open_dropdown_paints_option_rows() {
        use crate::view::controls::DropdownState;
        use crate::view::theme::{self, Theme};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        // An open dropdown with distinctive display names (mirrors the Theme
        // dropdown: display != stored value, e.g. a user theme).
        let mut dd = DropdownState::with_values(
            vec![
                "dark".to_string(),
                "light".to_string(),
                "my-cool-theme".to_string(),
            ],
            vec![
                "builtin://dark".to_string(),
                "builtin://light".to_string(),
                "my-cool-theme.json".to_string(),
            ],
            "Theme",
        )
        .with_selected(0);
        dd.open = true;
        let control = SettingControl::Dropdown(dd);

        let theme = Theme::load_builtin(theme::THEME_DARK).unwrap();
        let prev = std::collections::HashMap::new();

        let width = 60u16;
        let height = 6u16;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, width, height);
                render_control(
                    frame,
                    area,
                    &control,
                    "/theme",
                    0,
                    &theme,
                    Some(10),
                    false,
                    &prev,
                );
            })
            .unwrap();

        // And every option's display name must appear somewhere in the
        // painted buffer — the pre-fix code painted only the button row, so
        // the option names were absent.
        let buffer = terminal.backend().buffer().clone();
        let screen: String = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for name in ["dark", "light", "my-cool-theme"] {
            assert!(
                screen.contains(name),
                "option {name:?} not painted in open dropdown; screen was:\n{screen}"
            );
        }
    }
}
