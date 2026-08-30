//! Keybinding Editor rendering and input handling
//!
//! Renders the keybinding editor modal and handles input events.

use crate::app::keybinding_editor::{
    ContextFilter, DeleteResult, EditMode, KeybindingEditor, SearchMode, SourceFilter,
};
use crate::input::keybindings::{format_keybinding, normalize_key};
use crate::view::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fresh_i18n::t;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Render the keybinding editor modal into the box the tree placed.
///
/// `modal_area` used to be computed here — ninety percent of the area handed
/// in, capped at 120 columns, floored at 60 by 20, centred with `area.x` added
/// back so it landed beside the orchestrator dock rather than under it — and
/// then filed in `editor.layout.modal_area` for the mouse handler to compare
/// against. It is `view::shell::keybinding`'s now, and this is handed the
/// answer: one statement of where the box is, for the painter and the handler
/// both.
pub fn render_keybinding_editor(
    frame: &mut Frame,
    modal_area: Rect,
    editor: &mut KeybindingEditor,
    theme: &Theme,
) {
    // Clear background
    frame.render_widget(Clear, modal_area);

    // Border
    let title = format!(
        " {} \u{2500} [{}] ",
        t!("keybinding_editor.title"),
        editor.active_keymap
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.popup_border_fg))
        .style(Style::default().bg(theme.popup_bg).fg(theme.popup_text_fg));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Layout: header (3-4 lines) | table | footer (1 line)
    let chunks = Layout::vertical([
        Constraint::Length(3), // Header: config path + search + filters
        Constraint::Min(5),    // Table
        Constraint::Length(1), // Footer hints
    ])
    .split(inner);

    // Store layout for mouse hit testing. Two left: the box, which the tree
    // places and this reads, and the search bar, which is still a row this
    // paints.
    editor.layout.modal_area = modal_area;
    editor.layout.search_bar = Some(Rect {
        x: inner.x,
        y: inner.y + 1, // second row of header
        width: inner.width,
        height: 1,
    });

    // **The table is the tree's** (`view::shell::keybinding::table`): a header,
    // a rule, and `widgets::List` under them, in a `viewport` that owns the
    // window and draws the bar. What was here windowed the rows itself against
    // `editor.scroll`, filed `table_area` and `table_first_row_y` so a mouse
    // arm could turn a cell back into a row index, and filed `table_scrollbar`
    // so a second one could drag a thumb the library already drags.
    //
    // The page a `PgUp` moves by comes from the box the tree placed — see
    // `keybinding::table_rows` — so the window and the page cannot disagree.
    render_header(frame, chunks[0], editor, theme);
    render_footer(frame, chunks[2], editor, theme);

    // **The three dialogs are the tree's** — help, edit, and the
    // unsaved-changes confirmation (`view::shell::keybinding`). They are
    // layers, so they land on top of the table this drew, which is where
    // `apply_dimming` and three `Clear`s put them; the dimming is a `Scrim`.
    // Their fields and buttons answer their own presses, so the five
    // rectangles this used to file for the mouse arm — `dialog_key_field`,
    // `dialog_action_field`, `dialog_context_field`, `dialog_buttons`,
    // `confirm_buttons` — are gone from the record and from the arm.
}

/// Render the header section (config path, search, filters)
fn render_header(frame: &mut Frame, area: Rect, editor: &KeybindingEditor, theme: &Theme) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // Config path + keymap info
        Constraint::Length(1), // Search bar
        Constraint::Length(1), // Filters
    ])
    .split(area);

    // Line 1: Config file path and keymap names
    let mut path_spans = vec![
        Span::styled(
            format!(" {} ", t!("keybinding_editor.label_config")),
            Style::default().fg(theme.popup_text_fg),
        ),
        Span::styled(
            &editor.config_file_path,
            Style::default().fg(theme.diagnostic_info_fg),
        ),
    ];
    if !editor.keymap_names.is_empty() {
        path_spans.push(Span::styled(
            format!("  {} ", t!("keybinding_editor.label_maps")),
            Style::default().fg(theme.popup_text_fg),
        ));
        path_spans.push(Span::styled(
            editor.keymap_names.join(", "),
            Style::default().fg(theme.popup_text_fg),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(path_spans)), chunks[0]);

    // Line 2: Search bar
    if editor.search_active {
        let search_spans = match editor.search_mode {
            SearchMode::Text => {
                let mut spans = vec![
                    Span::styled(
                        format!(" {} ", t!("keybinding_editor.label_search")),
                        Style::default()
                            .fg(theme.help_key_fg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        &editor.search_query,
                        Style::default().fg(theme.popup_text_fg),
                    ),
                ];
                if editor.search_focused {
                    spans.push(Span::styled("_", Style::default().fg(theme.cursor)));
                    spans.push(Span::styled(
                        format!("  {}", t!("keybinding_editor.search_text_hint")),
                        Style::default().fg(theme.popup_text_fg),
                    ));
                }
                spans
            }
            SearchMode::RecordKey => {
                let key_text = if editor.search_key_display.is_empty() {
                    t!("keybinding_editor.press_a_key").to_string()
                } else {
                    editor.search_key_display.clone()
                };
                vec![
                    Span::styled(
                        format!(" {} ", t!("keybinding_editor.label_record_key")),
                        Style::default()
                            .fg(theme.diagnostic_warning_fg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(key_text, Style::default().fg(theme.popup_text_fg)),
                    Span::styled(
                        format!("  {}", t!("keybinding_editor.search_record_hint")),
                        Style::default().fg(theme.popup_text_fg),
                    ),
                ]
            }
        };
        frame.render_widget(Paragraph::new(Line::from(search_spans)), chunks[1]);
    } else {
        let hint = Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                t!("keybinding_editor.search_hint").to_string(),
                Style::default().fg(theme.popup_text_fg),
            ),
        ]);
        frame.render_widget(Paragraph::new(hint), chunks[1]);
    }

    // Line 3: Filters and counts
    let total = editor.bindings.len();
    let filtered = editor.filtered_indices.len();
    let count_str = if filtered == total {
        t!("keybinding_editor.bindings_count", count = total).to_string()
    } else {
        t!(
            "keybinding_editor.bindings_filtered",
            filtered = filtered,
            total = total
        )
        .to_string()
    };

    let filter_spans = vec![
        Span::styled(
            format!(" {} ", t!("keybinding_editor.label_context")),
            Style::default().fg(theme.popup_text_fg),
        ),
        Span::styled(
            format!("[{}]", editor.context_filter_display()),
            Style::default().fg(if editor.context_filter == ContextFilter::All {
                theme.popup_text_fg
            } else {
                theme.diagnostic_info_fg
            }),
        ),
        Span::styled(
            format!("  {} ", t!("keybinding_editor.label_source")),
            Style::default().fg(theme.popup_text_fg),
        ),
        Span::styled(
            format!("[{}]", editor.source_filter_display()),
            Style::default().fg(if editor.source_filter == SourceFilter::All {
                theme.popup_text_fg
            } else {
                theme.diagnostic_info_fg
            }),
        ),
        Span::styled(
            format!("  {}", count_str),
            Style::default().fg(theme.popup_text_fg),
        ),
        Span::styled(
            if editor.has_changes {
                format!("  {}", t!("keybinding_editor.modified"))
            } else {
                String::new()
            },
            Style::default().fg(theme.diagnostic_warning_fg),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(filter_spans)), chunks[2]);
}

/// Render the footer with key hints
fn render_footer(frame: &mut Frame, area: Rect, editor: &KeybindingEditor, theme: &Theme) {
    let hints = if editor.search_active && editor.search_focused {
        vec![
            Span::styled(" Esc", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_cancel")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("Tab", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_toggle_mode")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("Enter", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}", t!("keybinding_editor.footer_confirm")),
                Style::default().fg(theme.popup_text_fg),
            ),
        ]
    } else {
        vec![
            Span::styled(" Enter", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_edit")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("a", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_add")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("d", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_delete")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("/", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_search")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("r", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_record_key")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("c", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_context")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("s", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_source")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("?", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_help")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("Ctrl+S", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}  ", t!("keybinding_editor.footer_save")),
                Style::default().fg(theme.popup_text_fg),
            ),
            Span::styled("Esc", Style::default().fg(theme.help_key_fg)),
            Span::styled(
                format!(":{}", t!("keybinding_editor.footer_close")),
                Style::default().fg(theme.popup_text_fg),
            ),
        ]
    };

    frame.render_widget(Paragraph::new(Line::from(hints)), area);
}

// ==================== INPUT HANDLING ====================

/// Handle input for the keybinding editor. Returns true if the editor should close.
pub fn handle_keybinding_editor_input(
    editor: &mut KeybindingEditor,
    event: &KeyEvent,
) -> KeybindingEditorAction {
    // Help overlay
    if editor.showing_help {
        match event.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => {
                editor.showing_help = false;
            }
            _ => {}
        }
        return KeybindingEditorAction::Consumed;
    }

    // Confirm dialog
    if editor.showing_confirm_dialog {
        return handle_confirm_input(editor, event);
    }

    // Edit dialog
    if editor.edit_dialog.is_some() {
        return handle_edit_dialog_input(editor, event);
    }

    // Search mode (only when focused/accepting input)
    if editor.search_active && editor.search_focused {
        return handle_search_input(editor, event);
    }

    // Main table navigation
    handle_main_input(editor, event)
}

/// Actions that the keybinding editor can return to the parent
pub enum KeybindingEditorAction {
    /// Input was consumed, no further action needed
    Consumed,
    /// Close the editor (no save)
    Close,
    /// Save and close
    SaveAndClose,
    /// Status message to display
    StatusMessage(String),
}

fn handle_main_input(editor: &mut KeybindingEditor, event: &KeyEvent) -> KeybindingEditorAction {
    match (event.code, event.modifiers) {
        // Close / clear search
        (KeyCode::Esc, KeyModifiers::NONE) => {
            if editor.search_active {
                // Search is visible but unfocused — clear it
                editor.cancel_search();
                KeybindingEditorAction::Consumed
            } else if editor.has_changes {
                editor.showing_confirm_dialog = true;
                editor.confirm_selection = 0;
                KeybindingEditorAction::Consumed
            } else {
                KeybindingEditorAction::Close
            }
        }

        // Save
        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
            KeybindingEditorAction::SaveAndClose
        }

        // Navigation
        (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
            editor.select_prev();
            KeybindingEditorAction::Consumed
        }
        (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
            editor.select_next();
            KeybindingEditorAction::Consumed
        }
        (KeyCode::PageUp, _) => {
            editor.page_up();
            KeybindingEditorAction::Consumed
        }
        (KeyCode::PageDown, _) => {
            editor.page_down();
            KeybindingEditorAction::Consumed
        }
        (KeyCode::Home, _) => {
            editor.selected = 0;
            editor.scroll.offset = 0;
            KeybindingEditorAction::Consumed
        }
        (KeyCode::End, _) => {
            editor.selected = editor.display_rows.len().saturating_sub(1);
            editor.ensure_visible_public();
            KeybindingEditorAction::Consumed
        }

        // Search (re-focuses existing search if visible)
        (KeyCode::Char('/'), KeyModifiers::NONE) => {
            editor.start_search();
            KeybindingEditorAction::Consumed
        }

        // Record key search
        (KeyCode::Char('r'), KeyModifiers::NONE) => {
            editor.start_record_key_search();
            KeybindingEditorAction::Consumed
        }

        // Help
        (KeyCode::Char('?'), _) => {
            editor.showing_help = true;
            KeybindingEditorAction::Consumed
        }

        // Add binding
        (KeyCode::Char('a'), KeyModifiers::NONE) => {
            editor.open_add_dialog();
            KeybindingEditorAction::Consumed
        }

        // Enter: toggle section header or edit binding
        (KeyCode::Enter, KeyModifiers::NONE) => {
            if editor.selected_is_section_header() {
                editor.toggle_section_at_selected();
            } else {
                editor.open_edit_dialog();
            }
            KeybindingEditorAction::Consumed
        }

        // Delete binding
        (KeyCode::Char('d'), KeyModifiers::NONE) | (KeyCode::Delete, _) => {
            match editor.delete_selected() {
                DeleteResult::CustomRemoved => KeybindingEditorAction::StatusMessage(
                    t!("keybinding_editor.status_binding_removed").to_string(),
                ),
                DeleteResult::KeymapOverridden => KeybindingEditorAction::StatusMessage(
                    t!("keybinding_editor.status_keymap_overridden").to_string(),
                ),
                DeleteResult::CannotDelete | DeleteResult::NothingSelected => {
                    KeybindingEditorAction::StatusMessage(
                        t!("keybinding_editor.status_cannot_delete").to_string(),
                    )
                }
            }
        }

        // Context filter
        (KeyCode::Char('c'), KeyModifiers::NONE) => {
            editor.cycle_context_filter();
            KeybindingEditorAction::Consumed
        }

        // Source filter
        (KeyCode::Char('s'), KeyModifiers::NONE) => {
            editor.cycle_source_filter();
            KeybindingEditorAction::Consumed
        }

        _ => KeybindingEditorAction::Consumed,
    }
}

fn handle_search_input(editor: &mut KeybindingEditor, event: &KeyEvent) -> KeybindingEditorAction {
    match editor.search_mode {
        SearchMode::Text => match (event.code, event.modifiers) {
            (KeyCode::Esc, _) => {
                editor.cancel_search();
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Enter, _) | (KeyCode::Down, _) => {
                // Unfocus search, keep results visible, move to list
                editor.search_focused = false;
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Up, _) => {
                // Unfocus search, move to list, select last item
                editor.search_focused = false;
                editor.selected = editor.filtered_indices.len().saturating_sub(1);
                editor.ensure_visible_public();
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Tab, _) => {
                // Switch to record key mode
                editor.search_mode = SearchMode::RecordKey;
                editor.search_key_display.clear();
                editor.search_key_code = None;
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Backspace, _) => {
                editor.search_query.pop();
                editor.apply_filters();
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                editor.search_query.push(c);
                editor.apply_filters();
                KeybindingEditorAction::Consumed
            }
            _ => KeybindingEditorAction::Consumed,
        },
        SearchMode::RecordKey => match (event.code, event.modifiers) {
            (KeyCode::Esc, KeyModifiers::NONE) => {
                editor.cancel_search();
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                // Switch to text mode, preserve query
                editor.search_mode = SearchMode::Text;
                editor.apply_filters();
                KeybindingEditorAction::Consumed
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                // Unfocus search, keep results visible
                editor.search_focused = false;
                KeybindingEditorAction::Consumed
            }
            _ => {
                // Record the key
                editor.record_search_key(event);
                KeybindingEditorAction::Consumed
            }
        },
    }
}

fn handle_edit_dialog_input(
    editor: &mut KeybindingEditor,
    event: &KeyEvent,
) -> KeybindingEditorAction {
    // Take the dialog out to avoid borrow conflicts
    let mut dialog = match editor.edit_dialog.take() {
        Some(d) => d,
        None => return KeybindingEditorAction::Consumed,
    };

    // In special-capture mode on the key field, record the very next key
    // (including Esc, Tab, Enter) and exit capture mode.
    if dialog.capturing_special && dialog.focus_area == 0 {
        match event.code {
            KeyCode::Modifier(_) => {} // ignore bare modifier presses
            _ => {
                // Normalize the event so terminals that don't report SHIFT for
                // uppercase letters still produce a "Shift+letter" binding (e.g.
                // Shift+P stored as `key=p, modifiers=[shift]` rather than just
                // `key=p`). This mirrors the lookup-time normalization so the
                // recorded binding will match at runtime.
                let (norm_code, norm_mods) = normalize_key(event.code, event.modifiers);
                dialog.key_code = Some(norm_code);
                dialog.modifiers = norm_mods;
                // A recorded key replaces whatever the row held — including a
                // chord sequence, which must not linger and win at save time.
                dialog.chord_keys.clear();
                dialog.key_display = format_keybinding(&norm_code, &norm_mods);
                dialog.conflicts = editor.find_conflicts(norm_code, norm_mods, &dialog.context);
                dialog.capturing_special = false;
            }
        }
        editor.edit_dialog = Some(dialog);
        return KeybindingEditorAction::Consumed;
    }

    // Close dialog on Esc
    if event.code == KeyCode::Esc && event.modifiers == KeyModifiers::NONE {
        // Don't put it back - it's closed
        return KeybindingEditorAction::Consumed;
    }

    match dialog.focus_area {
        0 => {
            // Key recording area
            match (event.code, event.modifiers) {
                // Enter enters special-capture mode for the next keypress
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    dialog.capturing_special = true;
                }
                (KeyCode::Tab | KeyCode::Down, KeyModifiers::NONE) => {
                    dialog.focus_area = 1;
                    dialog.mode = EditMode::EditingAction;
                }
                _ => {
                    // Keys are only recorded via capture mode (Enter then key).
                    // Ignore everything else in the key field.
                }
            }
        }
        1 => {
            // Action editing area with autocomplete
            match (event.code, event.modifiers) {
                (KeyCode::Tab, KeyModifiers::NONE) => {
                    // Accept selected autocomplete suggestion, or move to next field
                    if dialog.autocomplete_visible {
                        if let Some(sel) = dialog.autocomplete_selected {
                            if sel < dialog.autocomplete_suggestions.len() {
                                let suggestion = dialog.autocomplete_suggestions[sel].clone();
                                dialog.action_text = suggestion;
                                dialog.action_cursor = dialog.action_text.len();
                                dialog.autocomplete_visible = false;
                                dialog.autocomplete_selected = None;
                                dialog.action_error = None;
                            }
                        }
                    } else {
                        dialog.focus_area = 2;
                        dialog.mode = EditMode::EditingContext;
                    }
                }
                (KeyCode::BackTab, _) => {
                    dialog.autocomplete_visible = false;
                    dialog.focus_area = 0;
                    dialog.mode = EditMode::RecordingKey;
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    // Accept selected autocomplete suggestion, or move to buttons
                    if dialog.autocomplete_visible {
                        if let Some(sel) = dialog.autocomplete_selected {
                            if sel < dialog.autocomplete_suggestions.len() {
                                let suggestion = dialog.autocomplete_suggestions[sel].clone();
                                dialog.action_text = suggestion;
                                dialog.action_cursor = dialog.action_text.len();
                                dialog.autocomplete_visible = false;
                                dialog.autocomplete_selected = None;
                                dialog.action_error = None;
                            }
                        }
                    } else {
                        dialog.focus_area = 3;
                        dialog.selected_button = 0;
                        dialog.mode = EditMode::EditingContext;
                    }
                }
                (KeyCode::Up, _) if dialog.autocomplete_visible => {
                    // Navigate autocomplete up
                    if let Some(sel) = dialog.autocomplete_selected {
                        if sel > 0 {
                            dialog.autocomplete_selected = Some(sel - 1);
                        }
                    }
                }
                (KeyCode::Down, _) if dialog.autocomplete_visible => {
                    // Navigate autocomplete down
                    if let Some(sel) = dialog.autocomplete_selected {
                        let max = dialog.autocomplete_suggestions.len().saturating_sub(1);
                        if sel < max {
                            dialog.autocomplete_selected = Some(sel + 1);
                        }
                    }
                }
                (KeyCode::Up, KeyModifiers::NONE) => {
                    // Move to previous field (key)
                    dialog.autocomplete_visible = false;
                    dialog.focus_area = 0;
                    dialog.mode = EditMode::RecordingKey;
                }
                (KeyCode::Down, KeyModifiers::NONE) => {
                    // Move to next field (context)
                    dialog.focus_area = 2;
                    dialog.mode = EditMode::EditingContext;
                }
                (KeyCode::Esc, _) if dialog.autocomplete_visible => {
                    // Close autocomplete without closing dialog
                    dialog.autocomplete_visible = false;
                    dialog.autocomplete_selected = None;
                    // Put dialog back and return early (don't let outer Esc handler close dialog)
                    editor.edit_dialog = Some(dialog);
                    return KeybindingEditorAction::Consumed;
                }
                (KeyCode::Backspace, _) => {
                    if dialog.action_cursor > 0 {
                        dialog.action_cursor -= 1;
                        dialog.action_text.remove(dialog.action_cursor);
                        dialog.action_error = None;
                    }
                    // Put dialog back and update autocomplete
                    editor.edit_dialog = Some(dialog);
                    editor.update_autocomplete();
                    return KeybindingEditorAction::Consumed;
                }
                (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                    dialog.action_text.insert(dialog.action_cursor, c);
                    dialog.action_cursor += 1;
                    dialog.action_error = None;
                    // Put dialog back and update autocomplete
                    editor.edit_dialog = Some(dialog);
                    editor.update_autocomplete();
                    return KeybindingEditorAction::Consumed;
                }
                _ => {}
            }
        }
        2 => {
            // Context selection area
            match (event.code, event.modifiers) {
                (KeyCode::Tab | KeyCode::Down, KeyModifiers::NONE) => {
                    dialog.focus_area = 3;
                    dialog.selected_button = 0;
                }
                (KeyCode::BackTab, _) | (KeyCode::Up, KeyModifiers::NONE) => {
                    dialog.focus_area = 1;
                    dialog.mode = EditMode::EditingAction;
                }
                (KeyCode::Left, _) if dialog.context_option_index > 0 => {
                    dialog.context_option_index -= 1;
                    dialog.context = dialog.context_options[dialog.context_option_index].clone();
                    // Update conflicts
                    if let Some(key_code) = dialog.key_code {
                        dialog.conflicts =
                            editor.find_conflicts(key_code, dialog.modifiers, &dialog.context);
                    }
                }
                (KeyCode::Right, _)
                    if dialog.context_option_index + 1 < dialog.context_options.len() =>
                {
                    dialog.context_option_index += 1;
                    dialog.context = dialog.context_options[dialog.context_option_index].clone();
                    if let Some(key_code) = dialog.key_code {
                        dialog.conflicts =
                            editor.find_conflicts(key_code, dialog.modifiers, &dialog.context);
                    }
                }
                (KeyCode::Enter, _) => {
                    dialog.focus_area = 3;
                    dialog.selected_button = 0;
                }
                _ => {}
            }
        }
        3 => {
            // Buttons area
            match (event.code, event.modifiers) {
                (KeyCode::Tab, KeyModifiers::NONE) => {
                    if dialog.selected_button < 1 {
                        // Move from Save to Cancel
                        dialog.selected_button = 1;
                    } else {
                        // Wrap from Cancel to Key field
                        dialog.focus_area = 0;
                        dialog.mode = EditMode::RecordingKey;
                    }
                }
                (KeyCode::BackTab, _) => {
                    if dialog.selected_button > 0 {
                        // Move from Cancel to Save
                        dialog.selected_button = 0;
                    } else {
                        // Wrap from Save to Context field
                        dialog.focus_area = 2;
                        dialog.mode = EditMode::EditingContext;
                    }
                }
                (KeyCode::Up, KeyModifiers::NONE) => {
                    dialog.focus_area = 2;
                    dialog.mode = EditMode::EditingContext;
                }
                (KeyCode::Left, _) if dialog.selected_button > 0 => {
                    dialog.selected_button -= 1;
                }
                (KeyCode::Right, _) if dialog.selected_button < 1 => {
                    dialog.selected_button += 1;
                }
                (KeyCode::Enter, _) => {
                    if dialog.selected_button == 0 {
                        // Save - put the dialog back first so apply_edit_dialog can take it
                        editor.edit_dialog = Some(dialog);
                        if let Some(err) = editor.apply_edit_dialog() {
                            // Validation failed - dialog is still open with error
                            return KeybindingEditorAction::StatusMessage(err);
                        }
                        return KeybindingEditorAction::Consumed;
                    } else {
                        // Cancel - don't put dialog back
                        return KeybindingEditorAction::Consumed;
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    // Put the dialog back
    editor.edit_dialog = Some(dialog);
    KeybindingEditorAction::Consumed
}

fn handle_confirm_input(editor: &mut KeybindingEditor, event: &KeyEvent) -> KeybindingEditorAction {
    match (event.code, event.modifiers) {
        (KeyCode::Left, _) => {
            if editor.confirm_selection > 0 {
                editor.confirm_selection -= 1;
            }
            KeybindingEditorAction::Consumed
        }
        (KeyCode::Right, _) => {
            if editor.confirm_selection < 2 {
                editor.confirm_selection += 1;
            }
            KeybindingEditorAction::Consumed
        }
        (KeyCode::Enter, _) => match editor.confirm_selection {
            0 => KeybindingEditorAction::SaveAndClose,
            1 => KeybindingEditorAction::Close, // Discard
            _ => {
                editor.showing_confirm_dialog = false;
                KeybindingEditorAction::Consumed
            }
        },
        (KeyCode::Esc, _) => {
            editor.showing_confirm_dialog = false;
            KeybindingEditorAction::Consumed
        }
        _ => KeybindingEditorAction::Consumed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybinding_editor::EditBindingState;
    use crate::config::Config;
    use crate::input::buffer_mode::ModeRegistry;
    use crate::input::command_registry::CommandRegistry;
    use crate::input::keybindings::KeybindingResolver;

    // **The two placement tests moved with the placement.** They pinned that
    // the modal stays inside the area it is handed and centres in it — the
    // orchestrator-dock regression, where it was placed relative to column 0
    // and bled left under the dock. That is `view::shell::keybinding`'s now
    // (`the_box_centres_beside_the_dock`), stated against the region the layer
    // names rather than against a rectangle a caller passed in.

    fn make_editor() -> KeybindingEditor {
        let config = Config::default();
        let resolver = KeybindingResolver::new(&config);
        let mode_registry = ModeRegistry::new();
        let cmd_registry = CommandRegistry::new();
        let menu_names: Vec<String> = ["File", "Edit", "View"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        KeybindingEditor::new(
            &config,
            &resolver,
            &mode_registry,
            &cmd_registry,
            String::from("/tmp/fresh-config.toml"),
            &menu_names,
        )
    }

    /// Drive the add-binding dialog through one "capture key" flow and
    /// return the resulting (key_code, modifiers).
    fn capture_in_add_dialog(event: KeyEvent) -> (Option<KeyCode>, KeyModifiers) {
        let mut editor = make_editor();
        editor.edit_dialog = Some(EditBindingState::new_add());
        // Enter the "press a key" capture mode by sending Enter on the key
        // field, then send the simulated event.
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_keybinding_editor_input(&mut editor, &enter);
        handle_keybinding_editor_input(&mut editor, &event);
        let dialog = editor.edit_dialog.as_ref().expect("dialog still open");
        (dialog.key_code, dialog.modifiers)
    }

    #[test]
    fn add_dialog_records_shift_when_terminal_omits_shift_modifier() {
        // Regression for https://github.com/sinelaw/fresh/issues/1899
        // When a non-kitty terminal sends Char('P') with no modifier (the
        // typical case for Shift+P), the add-binding dialog must still
        // capture this as a "Shift+P" binding rather than just "p".
        let plain_upper = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::empty());
        let (code, mods) = capture_in_add_dialog(plain_upper);
        assert_eq!(code, Some(KeyCode::Char('p')));
        assert!(
            mods.contains(KeyModifiers::SHIFT),
            "Shift+P (sent as plain 'P') must capture SHIFT (got modifiers={:?})",
            mods
        );
    }

    #[test]
    fn add_dialog_records_shift_when_terminal_includes_shift_modifier() {
        // The fix must not regress the kitty-protocol path either.
        let kitty_shift = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT);
        let (code, mods) = capture_in_add_dialog(kitty_shift);
        assert_eq!(code, Some(KeyCode::Char('p')));
        assert!(mods.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn add_dialog_preserves_ctrl_when_capturing_upper_letter() {
        // CapsLock+Ctrl+A — uppercase letter with CONTROL modifier — should
        // record as plain Ctrl+A (no inferred SHIFT) so the long-standing
        // caps-lock-tolerant lookup keeps working.
        let caps_ctrl_a = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::CONTROL);
        let (code, mods) = capture_in_add_dialog(caps_ctrl_a);
        assert_eq!(code, Some(KeyCode::Char('a')));
        assert_eq!(mods, KeyModifiers::CONTROL);
    }
}
