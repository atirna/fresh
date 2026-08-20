//! The review stream's add / remove rows are coloured by the `Fresh Review`
//! grammar on the host, not by a per-row overlay from the plugin.
//!
//! That grammar anchors on the *exact rendered width* of the row's
//! line-number gutter, which the plugin builds from its own `LINE_NUM_W`.
//! Nothing in either language checks the two against each other, so if one
//! side's width changes the grammar quietly matches nothing and every
//! content row renders unstyled. Driving the real review command and
//! reading the rendered cells is what catches that.
//!
//! Both emission paths are covered, because they were changed separately:
//! a modified line (adjacent `-`/`+`, the word-diff path) and a pure
//! addition (the plain path).

use crate::common::git_test_helper::GitTestRepo;
use crate::common::harness::{copy_plugin, copy_plugin_lib, EditorTestHarness, HarnessOptions};
use fresh::config::Config;
use ratatui::style::Color;
use std::collections::HashSet;
use std::fs;

/// Context line kept on both sides of the change.
const CTX_ROW: &str = "KEEP CONTEXT ROW";
/// The modified line, before and after — an adjacent `-`/`+` pair.
const OLD_ROW: &str = "OLD MODIFIED ROW";
const NEW_ROW: &str = "NEW MODIFIED ROW";
/// A line added with no removal beside it, so it takes the plain path.
const PURE_ADD: &str = "PURE ADDED ROW";

fn harness_with_highlighting(working_dir: std::path::PathBuf) -> EditorTestHarness {
    EditorTestHarness::create(
        140,
        40,
        HarnessOptions::new()
            .with_config(Config::default())
            .with_working_dir(working_dir)
            .without_empty_plugins_dir()
            .with_full_grammar_registry(),
    )
    .unwrap()
}

fn row_text(harness: &EditorTestHarness, y: u16) -> String {
    let buf = harness.buffer();
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
}

fn row_containing(harness: &EditorTestHarness, needle: &str) -> Option<u16> {
    let height = harness.buffer().area.height;
    (0..height).find(|&y| row_text(harness, y).contains(needle))
}

/// Background colour of a single cell.
fn bg_at(harness: &EditorTestHarness, x: u16, y: u16) -> Color {
    harness.buffer()[(x, y)].style().bg.unwrap_or(Color::Reset)
}

/// The distinct background colours across row `y`, ignoring the last two
/// columns so a scrollbar or divider can't join the sample.
fn backgrounds(harness: &EditorTestHarness, y: u16) -> HashSet<Color> {
    let width = harness.buffer().area.width;
    (0..width.saturating_sub(2))
        .map(|x| bg_at(harness, x, y))
        .collect()
}

fn setup(repo: &GitTestRepo) {
    let plugins_dir = repo.path.join("plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    copy_plugin(&plugins_dir, "audit_mode");
    copy_plugin_lib(&plugins_dir);
}

/// One unstaged change carrying both a modified line and a pure addition.
fn repo_with_a_modification_and_an_addition() -> GitTestRepo {
    let repo = GitTestRepo::new();
    setup(&repo);
    repo.create_file(
        "src/lib.rs",
        &format!("fn head() {{}}\n{CTX_ROW}\n{OLD_ROW}\nfn tail() {{}}\n"),
    );
    repo.git_add_all();
    repo.git_commit("baseline");
    repo.create_file(
        "src/lib.rs",
        &format!("fn head() {{}}\n{CTX_ROW}\n{NEW_ROW}\n{PURE_ADD}\nfn tail() {{}}\n"),
    );
    repo
}

fn open_review(repo: &GitTestRepo) -> EditorTestHarness {
    let mut harness = harness_with_highlighting(repo.path.clone());
    harness.render().unwrap();
    harness.run_palette_command("Review Diff").unwrap();
    harness.wait_for_prompt_closed().unwrap();
    harness
        .wait_until(|h| {
            let s = h.screen_to_string();
            s.contains("next hunk") && !s.contains("Generating Review")
        })
        .unwrap();
    harness
        .wait_until(|h| h.screen_to_string().contains(PURE_ADD))
        .unwrap();
    harness.wait_for_async_quiescence(3).unwrap();
    harness
}

/// Added and removed rows must each carry their own single background, and
/// a context row must carry neither. Comparing the three against each other
/// keeps the assertion independent of any particular theme's palette.
// TODO: git command output differs on Windows; the other review tests skip it.
#[test]
#[cfg_attr(target_os = "windows", ignore)]
fn add_and_remove_rows_are_coloured_and_context_is_not() {
    let repo = repo_with_a_modification_and_an_addition();
    let harness = open_review(&repo);
    let screen = harness.screen_to_string();

    let find = |needle: &str| {
        row_containing(&harness, needle)
            .unwrap_or_else(|| panic!("`{needle}` never rendered.\nScreen:\n{screen}"))
    };
    let ctx_y = find(CTX_ROW);
    let removed_y = find(OLD_ROW);
    let word_add_y = find(NEW_ROW);
    let plain_add_y = find(PURE_ADD);

    // A content row is washed edge to edge, so exactly one colour covers it.
    let one_bg = |y: u16, label: &str| -> Color {
        let bgs = backgrounds(&harness, y);
        assert_eq!(
            bgs.len(),
            1,
            "{label} row ({y}) should carry a single background across its \
             width, got {bgs:?}\nrow: {:?}\nScreen:\n{screen}",
            row_text(&harness, y),
        );
        bgs.into_iter().next().unwrap()
    };

    let ctx_bg = one_bg(ctx_y, "context");
    let removed_bg = one_bg(removed_y, "removed");
    let word_add_bg = one_bg(word_add_y, "added (word-diff path)");
    let plain_add_bg = one_bg(plain_add_y, "added (plain path)");

    assert_ne!(
        removed_bg, ctx_bg,
        "the removed row is painted like context, so the grammar matched \
         nothing — most likely the gutter width and the grammar disagree\n\
         Screen:\n{screen}",
    );
    assert_ne!(
        plain_add_bg, ctx_bg,
        "the added row is painted like context, so the grammar matched \
         nothing\nScreen:\n{screen}",
    );
    assert_ne!(
        plain_add_bg, removed_bg,
        "additions and removals share a background\nScreen:\n{screen}",
    );
    assert_eq!(
        word_add_bg, plain_add_bg,
        "the two add-row emission paths disagree on the row background\n\
         Screen:\n{screen}",
    );
}

/// The wash has to run past the end of the row's text to the pane edge, the
/// way the plugin's `extendToLineEnd` overlay used to. A span only covers
/// the bytes it matched, so this is the renderer's diff tail-fill being
/// exercised — and it is the half most likely to regress silently, since
/// the row still looks coloured where the text is.
// TODO: git command output differs on Windows; the other review tests skip it.
#[test]
#[cfg_attr(target_os = "windows", ignore)]
fn the_add_row_wash_reaches_the_pane_edge() {
    let repo = repo_with_a_modification_and_an_addition();
    let harness = open_review(&repo);
    let screen = harness.screen_to_string();

    let y = row_containing(&harness, PURE_ADD)
        .unwrap_or_else(|| panic!("`{PURE_ADD}` never rendered.\nScreen:\n{screen}"));
    let text = row_text(&harness, y);
    let text_end = text.trim_end().chars().count() as u16;
    let width = harness.buffer().area.width;
    assert!(
        text_end + 4 < width,
        "row {y} fills the pane, so there is no tail to check: {text:?}",
    );

    let at_text = bg_at(&harness, text_end.saturating_sub(1), y);
    for x in text_end..width.saturating_sub(2) {
        assert_eq!(
            bg_at(&harness, x, y),
            at_text,
            "column {x} of the added row ({y}) is past the text and lost the \
             wash; the diff background must fill to the pane edge\n\
             row: {text:?}\nScreen:\n{screen}",
        );
    }
}
