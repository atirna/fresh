use crate::input::fuzzy::FuzzyMatch;
use crate::primitives::display_width::str_width;
use crate::view::file_tree::{
    ExplorerSlotContext, ExplorerSlotResolver, FileExplorerDecorationCache,
    FileExplorerSlotOverrideCache, FileTreeView, NodeId,
};
use crate::view::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders},
    Frame,
};

use std::collections::HashSet;
use std::path::PathBuf;

/// The plugin-driven decoration inputs for the file explorer: the slot
/// resolver plus the decoration and slot-override caches. These three always
/// travel together through the render pipeline, so they're bundled rather
/// than threaded as three parallel parameters. `Copy` (the resolver is two
/// `&dyn` pointers, the caches are shared refs), so it passes by value into
/// the per-row closure without cloning.
#[derive(Clone, Copy)]
pub struct ExplorerDecorations<'a> {
    pub slot_resolver: ExplorerSlotResolver<'a>,
    pub decorations: &'a FileExplorerDecorationCache,
    pub slot_overrides: &'a FileExplorerSlotOverrideCache,
}

pub struct FileExplorerRenderer;

/// Inputs to [`FileExplorerRenderer::trailing_slot_screen_bounds`], grouped

impl FileExplorerRenderer {
    /// The explorer panel's title in its non-search state: `" File Explorer (kb) "`
    /// locally, `" [host] (kb) "` on a remote connection. Shared by the
    /// materialised panel and the loading placeholder
    /// ([`Self::render_loading`]) so the chrome can't drift between the two
    /// build phases — the tree landing must change nothing but the list body.
    fn panel_title(remote_connection: Option<&str>, keybinding_suffix: &str) -> String {
        if let Some(host) = remote_connection {
            // Extract just the hostname from "user@host" or "user@host:port"
            let hostname = host
                .split('@')
                .next_back()
                .unwrap_or(host)
                .split(':')
                .next()
                .unwrap_or(host);
            format!(" [{}]{} ", hostname, keybinding_suffix)
        } else {
            format!(" File Explorer{} ", keybinding_suffix)
        }
    }

    /// Title + border styles for the panel chrome: warning colors when the
    /// remote is disconnected, inverted (dark on light) title when focused.
    /// Shared with [`Self::render_loading`] for the same no-drift reason as
    /// [`Self::panel_title`].
    fn panel_chrome_styles(
        remote_connection: Option<&str>,
        is_focused: bool,
        theme: &Theme,
    ) -> (Style, Style) {
        let remote_disconnected = remote_connection
            .map(|c| c.contains("(Disconnected)"))
            .unwrap_or(false);
        if remote_disconnected {
            (
                Style::default()
                    .fg(theme.status_error_indicator_fg)
                    .bg(theme.status_error_indicator_bg)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(theme.status_error_indicator_bg),
            )
        } else if is_focused {
            (
                Style::default()
                    .fg(theme.editor_bg)
                    .bg(theme.editor_fg)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(theme.cursor),
            )
        } else {
            (
                Style::default().fg(theme.line_number_fg),
                Style::default().fg(theme.split_separator_fg),
            )
        }
    }

    /// Render the close button "×" at the right side of the title bar.
    /// Shared by the materialised panel and the loading placeholder.
    fn render_close_button(
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        close_button_hovered: bool,
    ) {
        let close_button_x = area.x + area.width.saturating_sub(3);
        let close_fg = if close_button_hovered {
            theme.tab_close_hover_fg
        } else {
            theme.line_number_fg
        };
        let close_button =
            ratatui::widgets::Paragraph::new("×").style(Style::default().fg(close_fg));
        let close_area = Rect::new(close_button_x, area.y, 1, 1);
        frame.render_widget(close_button, close_area);
    }

    /// Render the loading placeholder shown while the tree is still being
    /// built (initial async build or expand-to-path sync): the panel's FINAL
    /// chrome — same title, borders, styles, and close button as the
    /// materialised panel — with a dimmed "Loading…" body in place of the
    /// list. The tree landing therefore swaps only the body content; the
    /// page layout and the panel chrome are pixel-identical before and after
    /// the load, so a slow (e.g. remote) build never causes a two-stage
    /// paint of the surrounding window.
    #[allow(clippy::too_many_arguments)]
    pub fn render_loading(
        frame: &mut Frame,
        area: Rect,
        is_focused: bool,
        keybinding_resolver: &crate::input::keybindings::KeybindingResolver,
        current_context: crate::input::keybindings::KeyContext,
        theme: &Theme,
        close_button_hovered: bool,
        remote_connection: Option<&str>,
    ) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        let keybinding_suffix = keybinding_resolver
            .get_keybinding_for_action(
                &crate::input::keybindings::Action::FocusFileExplorer,
                current_context,
            )
            .map(|kb| format!(" ({})", kb))
            .unwrap_or_default();
        let title = Self::panel_title(remote_connection, &keybinding_suffix);
        let (title_style, border_style) =
            Self::panel_chrome_styles(remote_connection, is_focused, theme);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(title_style)
            .border_style(border_style)
            .style(Style::default().bg(theme.editor_bg));
        let placeholder = Paragraph::new(fresh_i18n::t!("explorer.loading").to_string())
            .style(Style::default().fg(theme.line_number_fg))
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(placeholder, area);

        Self::render_close_button(frame, area, theme, close_button_hovered);
    }

    /// Check if a directory contains any modified files
    pub(crate) fn folder_has_modified_files(
        folder_path: &PathBuf,
        files_with_unsaved_changes: &HashSet<PathBuf>,
    ) -> bool {
        for modified_file in files_with_unsaved_changes {
            if modified_file.starts_with(folder_path) {
                return true;
            }
        }
        false
    }
}

/// Everything one row needs to describe itself.
pub struct RowDesc<'a> {
    pub view: &'a FileTreeView,
    pub node_id: NodeId,
    pub indent: usize,
    /// The row's index *in the viewport* — its key, and what hit-testing
    /// answers with.
    pub row: usize,
    pub is_cursor: bool,
    pub is_multi: bool,
    pub focused: bool,
    pub unsaved: &'a HashSet<PathBuf>,
    pub cut: &'a [PathBuf],
    pub fuzzy: Option<&'a FuzzyMatch>,
    pub decorations: &'a crate::view::file_tree::FileExplorerDecorationCache,
    pub slot_overrides: &'a crate::view::file_tree::FileExplorerSlotOverrideCache,
    pub slot_resolver: &'a crate::view::file_tree::ExplorerSlotResolver<'static>,
    pub theme: &'a Theme,
    pub collapsed: &'a str,
    pub expanded: &'a str,
}

/// One row of the tree, as the shell describes it.
///
/// This is [`FileExplorerRenderer::build_node_line`] with the arithmetic taken
/// out. It still decides *what the row says* — the indicator glyph and its
/// padding, the leading slot and its padding, the compacted ancestor chain,
/// the name and its fuzzy-match highlights, the trailing status slot and the
/// error marker — and it still decides what each piece looks like, but now as
/// a theme *name* rather than a resolved `Color`.
///
/// What it no longer decides is where anything sits. `content_width`,
/// `left_side_width`, `total_right_width` and the `padding` rule are gone: the
/// gap between the name and the status slot is a flex spacer with a floor, and
/// the tree measures it. So is `trailing_slot_screen_bounds`, the 45-line
/// second derivation that existed only so a hover could find the slot the
/// painter had already placed.
pub fn describe_row(d: RowDesc<'_>) -> Option<crate::view::shell::file_explorer::Row> {
    use crate::app::shell_host::shell_theme::{literal, pair};
    use crate::view::shell::file_explorer as fe;

    let node = d.view.tree().get_node(d.node_id)?;
    let is_hidden = node
        .entry
        .metadata
        .as_ref()
        .map(|m| m.is_hidden)
        .unwrap_or(false);
    let neutral = fe::neutral_key(is_hidden, node.entry.is_symlink(), node.is_dir());
    let ground = if d.is_cursor && d.focused {
        "editor.selection_bg"
    } else if d.is_cursor {
        "editor.current_line_bg"
    } else if d.is_multi && d.focused {
        "editor.selection_bg"
    } else {
        "editor.bg"
    };

    let has_unsaved = if node.is_dir() {
        FileExplorerRenderer::folder_has_modified_files(&node.entry.path, d.unsaved)
    } else {
        d.unsaved.contains(&node.entry.path)
    };
    let slots = d.slot_resolver.resolve(&ExplorerSlotContext {
        path: &node.entry.path,
        is_dir: node.is_dir(),
        has_unsaved,
        is_symlink: node.entry.is_symlink(),
        is_hidden,
        decorations: d.decorations,
        slot_overrides: d.slot_overrides,
        theme: d.theme,
        // The neutral colour the slot providers fall back to. They still work
        // in `Color`; only the description speaks in names.
        neutral_fg: d
            .theme
            .resolve_theme_key(neutral)
            .unwrap_or(d.theme.editor_fg),
    });

    let is_cut = d.cut.iter().any(|p| p == &node.entry.path);
    let name_fg = if is_cut {
        "editor.line_number_fg".to_string()
    } else if let Some(c) = slots.name_color_hint {
        literal(c)
    } else if (d.is_cursor || d.is_multi) && d.focused {
        "editor.fg".to_string()
    } else {
        neutral.to_string()
    };

    let mut left: fe::Runs = Vec::new();
    if d.indent > 0 {
        left.push((" ".repeat(d.indent * 2), pair(neutral, ground)));
    }

    // The indicator column is sized from the configured glyphs so names stay
    // aligned when a user picks a wider one.
    let collapsed_w = str_width(d.collapsed);
    let expanded_w = str_width(d.expanded);
    let indicator_width = collapsed_w.max(expanded_w).max(1) + 1;
    if node.is_dir() {
        let (glyph, w) = if node.is_expanded() {
            (format!("{} ", d.expanded), expanded_w + 1)
        } else if node.is_collapsed() {
            (format!("{} ", d.collapsed), collapsed_w + 1)
        } else if node.is_loading() {
            ("⟳ ".to_string(), 2)
        } else {
            ("! ".to_string(), 2)
        };
        left.push((glyph, pair("diagnostic.warning_fg", ground)));
        let pad = indicator_width.saturating_sub(w);
        if pad > 0 {
            left.push((" ".repeat(pad), pair(neutral, ground)));
        }
    } else {
        left.push((" ".repeat(indicator_width), pair(neutral, ground)));
    }

    if let Some(slot) = &slots.leading {
        let text_w = str_width(&slot.text);
        let pad = slot.width().saturating_sub(text_w) + 1;
        left.push((slot.text.clone(), pair(&literal(slot.fg), ground)));
        left.push((" ".repeat(pad), pair(neutral, ground)));
    }

    // Ancestors that compact mode folded into this row, outermost first.
    for id in d.view.compact_chain_for_anchor(d.node_id) {
        if let Some(n) = d.view.tree().get_node(id) {
            left.push((n.entry.name.clone(), pair("syntax.keyword", ground)));
            left.push(("/".to_string(), pair("editor.line_number_fg", ground)));
        }
    }

    match d.fuzzy {
        Some(fm) => {
            let matched: std::collections::HashSet<usize> =
                fm.match_positions.iter().copied().collect();
            let hit = pair("search.match_fg", "search.match_bg");
            let base = pair(&name_fg, ground);
            let mut run = String::new();
            let mut run_is_match = false;
            for (i, c) in node.entry.name.chars().enumerate() {
                let is_match = matched.contains(&i);
                if i > 0 && is_match != run_is_match {
                    let theme = if run_is_match {
                        hit.clone()
                    } else {
                        base.clone()
                    };
                    left.push((std::mem::take(&mut run), theme));
                }
                run_is_match = is_match;
                run.push(c);
            }
            if !run.is_empty() {
                left.push((run, if run_is_match { hit } else { base }));
            }
        }
        None => left.push((node.entry.name.clone(), pair(&name_fg, ground))),
    }

    Some(fe::Row {
        index: d.row,
        theme: pair("editor.fg", ground),
        left,
        trailing: slots.trailing.as_ref().map(|slot| fe::Slot {
            text: slot.text.clone(),
            theme: pair(&literal(slot.fg), ground),
            path: node.entry.path.clone(),
        }),
        error: node
            .is_error()
            .then(|| (" [Error]".to_string(), pair("diagnostic.error_fg", ground))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::filesystem::StdFileSystem;
    use crate::services::fs::FsManager;
    use std::collections::{HashMap, HashSet};
    use std::fs as std_fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_renderer_view() -> (TempDir, FileTreeView) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        std_fs::create_dir(root.join("src")).unwrap();
        std_fs::write(root.join("README.md"), "hello").unwrap();
        std_fs::write(root.join("src/schema.ts"), "export const value = 1;\n").unwrap();

        let manager = Arc::new(FsManager::new(Arc::new(StdFileSystem)));
        let mut tree = crate::view::file_tree::FileTree::new(root.to_path_buf(), manager)
            .await
            .unwrap();
        let root_id = tree.root_id();
        tree.expand_node(root_id).await.unwrap();
        let src_id = tree
            .get_node(root_id)
            .unwrap()
            .children
            .iter()
            .copied()
            .find(|id| tree.get_node(*id).unwrap().entry.name == "src")
            .unwrap();
        tree.expand_node(src_id).await.unwrap();

        (temp_dir, FileTreeView::new(tree))
    }

    /// One row's pieces, with each theme *name* resolved back to the style it
    /// stands for — so these tests can go on asserting about colours while the
    /// description itself speaks in names.
    fn build_line(
        view: &FileTreeView,
        node_id: NodeId,
        indent: usize,
        decorations: &FileExplorerDecorationCache,
        slot_overrides: &FileExplorerSlotOverrideCache,
        theme: &Theme,
    ) -> Vec<(String, Style)> {
        let resolver = crate::view::file_tree::default_slot_providers().resolver();
        let row = describe_row(RowDesc {
            view,
            node_id,
            indent,
            row: 0,
            is_cursor: false,
            is_multi: false,
            focused: false,
            unsaved: &HashSet::new(),
            cut: &[],
            fuzzy: None,
            decorations,
            slot_overrides,
            slot_resolver: &resolver,
            theme,
            collapsed: ">",
            expanded: "▼",
        })
        .expect("the node exists");
        let resolve = |name: &str| crate::app::shell_host::shell_theme::resolve(name, theme);
        row.left
            .into_iter()
            .map(|(t, name)| (t, resolve(&name)))
            .chain(row.trailing.map(|s| (s.text, resolve(&s.theme))))
            .chain(row.error.map(|(t, name)| (t, resolve(&name))))
            .collect()
    }

    #[tokio::test]
    async fn renderer_line_shows_plugin_decoration_badge() {
        let (_temp_dir, view) = create_renderer_view().await;
        let theme = Theme::load_builtin("dark").unwrap();
        let schema_path = view.tree().root_path().join("src/schema.ts");
        let schema_id = view.tree().get_node_by_path(&schema_path).unwrap().id;
        let decorations = FileExplorerDecorationCache::rebuild(
            vec![crate::view::file_tree::FileExplorerDecoration {
                path: schema_path,
                symbol: "M".to_string(),
                color: fresh_core::api::OverlayColorSpec::ThemeKey(
                    "ui.file_status_modified_fg".into(),
                ),
                priority: 50,
            }],
            view.tree().root_path(),
            &HashMap::new(),
        );

        let line = build_line(
            &view,
            schema_id,
            2,
            &decorations,
            &FileExplorerSlotOverrideCache::default(),
            &theme,
        );

        assert!(line
            .iter()
            .any(|(text, style)| text == "M" && style.fg == Some(theme.file_status_modified_fg)));
    }

    #[tokio::test]
    async fn directories_render_bubbled_plugin_status() {
        let (_temp_dir, view) = create_renderer_view().await;
        let theme = Theme::load_builtin("dark").unwrap();
        let src_path = view.tree().root_path().join("src");
        let schema_path = src_path.join("schema.ts");
        let src_id = view.tree().get_node_by_path(&src_path).unwrap().id;
        let decorations = FileExplorerDecorationCache::rebuild(
            vec![crate::view::file_tree::FileExplorerDecoration {
                path: schema_path,
                symbol: "R".to_string(),
                color: fresh_core::api::OverlayColorSpec::ThemeKey(
                    "ui.file_status_renamed_fg".into(),
                ),
                priority: 40,
            }],
            view.tree().root_path(),
            &HashMap::new(),
        );

        let line = build_line(
            &view,
            src_id,
            1,
            &decorations,
            &FileExplorerSlotOverrideCache::default(),
            &theme,
        );

        assert!(line
            .iter()
            .any(|(text, style)| text == "●" && style.fg == Some(theme.file_status_renamed_fg)));
    }

    #[tokio::test]
    async fn default_slot_providers_allow_explicit_slot_and_name_color_overrides() {
        let (_temp_dir, view) = create_renderer_view().await;
        let theme = Theme::load_builtin("dark").unwrap();
        let schema_path = view.tree().root_path().join("src/schema.ts");
        let schema_id = view.tree().get_node_by_path(&schema_path).unwrap().id;
        let slot_overrides = FileExplorerSlotOverrideCache::rebuild(
            vec![fresh_core::file_explorer::FileExplorerSlotEntry {
                path: schema_path.clone(),
                leading: Some(fresh_core::file_explorer::FileExplorerLeadingSlot {
                    text: "PL".to_string(),
                    color: fresh_core::api::OverlayColorSpec::ThemeKey("syntax.string".into()),
                    min_width: 2,
                }),
                trailing: Some(fresh_core::file_explorer::FileExplorerTrailingSlot {
                    text: "X".to_string(),
                    color: fresh_core::api::OverlayColorSpec::ThemeKey("syntax.type".into()),
                    tooltip: Some(fresh_core::file_explorer::FileExplorerTooltip {
                        title: "Plugin".to_string(),
                        lines: vec!["Overridden".to_string()],
                    }),
                }),
                name_color: Some(fresh_core::api::OverlayColorSpec::ThemeKey(
                    "ui.file_status_added_fg".into(),
                )),
                priority: 50,
                suppress_leading: false,
                suppress_trailing: false,
                suppress_name_color: false,
            }],
            view.tree().root_path(),
            &HashMap::new(),
        );

        let line = build_line(
            &view,
            schema_id,
            2,
            &FileExplorerDecorationCache::default(),
            &slot_overrides,
            &theme,
        );

        assert!(line.iter().any(|(text, _)| text == "PL"));
        assert!(line.iter().any(|(text, _)| text == "X"));
        assert!(line.iter().any(
            |(text, style)| text == "schema.ts" && style.fg == Some(theme.file_status_added_fg)
        ));
    }

    #[tokio::test]
    async fn default_slot_providers_fall_back_when_only_name_color_is_overridden() {
        let (_temp_dir, view) = create_renderer_view().await;
        let theme = Theme::load_builtin("dark").unwrap();
        let schema_path = view.tree().root_path().join("src/schema.ts");
        let schema_id = view.tree().get_node_by_path(&schema_path).unwrap().id;
        let decorations = FileExplorerDecorationCache::rebuild(
            vec![crate::view::file_tree::FileExplorerDecoration {
                path: schema_path.clone(),
                symbol: "M".to_string(),
                color: fresh_core::api::OverlayColorSpec::ThemeKey(
                    "ui.file_status_modified_fg".into(),
                ),
                priority: 50,
            }],
            view.tree().root_path(),
            &HashMap::new(),
        );
        let slot_overrides = FileExplorerSlotOverrideCache::rebuild(
            vec![fresh_core::file_explorer::FileExplorerSlotEntry {
                path: schema_path,
                leading: None,
                trailing: None,
                name_color: Some(fresh_core::api::OverlayColorSpec::ThemeKey(
                    "syntax.string".into(),
                )),
                priority: 50,
                suppress_leading: false,
                suppress_trailing: false,
                suppress_name_color: false,
            }],
            view.tree().root_path(),
            &HashMap::new(),
        );

        let line = build_line(&view, schema_id, 2, &decorations, &slot_overrides, &theme);

        assert!(line
            .iter()
            .any(|(text, style)| text == "schema.ts" && style.fg == Some(theme.syntax_string)));
        assert!(line
            .iter()
            .any(|(text, style)| text == "M" && style.fg == Some(theme.file_status_modified_fg)));
    }
}
