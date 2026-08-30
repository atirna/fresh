//! Settings UI renderer
//!
//! Renders the settings modal with category navigation and setting controls.

use super::entry_dialog::EntryDialogState;
use super::layout::{SettingsHit, SettingsLayout};
use super::search::{DeepMatch, SearchResult};
use super::state::SettingsState;
use crate::view::theme::Theme;
use crate::view::ui::scrollbar::{render_scrollbar, ScrollbarColors, ScrollbarState};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
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

    // **Nothing over the box is painted any more.** The confirm and reset
    // prompts, the entry dialog's discard and delete prompts, the help
    // overlay and now the entry stack itself are all layers
    // (`view::shell::settings`, `view::shell::entry`), with `apply_dimming`
    // as each one's `Scrim` and every button answering its own press. They
    // had to stay painted while the stack was, because a described prompt
    // would have been a layer over a painted dialog rather than under it;
    // the stack crossing is what released them.
    let _ = (has_confirm, has_reset, has_entry, has_help);

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

// **The whole of the settings dialog's chrome is described.** What stood
// here — the two prompts and their button rows, the help overlay, the
// entry-edit stack with its own scroll, its per-field controls and its three
// bottom rows, and the widget adapter each of those controls painted through
// — is `view::shell::settings` and `view::shell::entry`. What is left in this
// file is the search results, which replace the body while a search runs, and
// the narrow layout's horizontal category strip.

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

/// Compute the footer Delete-button label for an entry dialog.
///
/// Schema-driven: shows the map key for map entries (e.g.
/// `[ Delete "rust" ]`), a generic "item" for array items (the
/// numeric index isn't meaningful to the user), or a bare fallback
/// when neither is available. The key is truncated so a very long
/// identifier can't blow out the dialog footer.
pub(crate) fn entry_delete_button_label(dialog: &EntryDialogState) -> String {
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
}
