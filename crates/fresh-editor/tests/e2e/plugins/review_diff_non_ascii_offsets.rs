//! Regression: the review stream's inline highlights are placed by **UTF-8
//! byte offset**, computed in the plugin, and every one of them is derived by
//! summing the byte lengths of the pieces to its left.
//!
//! A word-level highlight is the strictest case: its start is the byte length
//! of the row's line-number prefix plus the diff marker plus every unchanged
//! word before it. Get any of those lengths wrong for non-ASCII text — count
//! UTF-16 units, or take an ASCII shortcut that doesn't hold — and the
//! highlight slides left by one column per multi-byte character above and to
//! the left of it, landing on the wrong word.
//!
//! So the file under review puts accented text *before* the changed word on
//! the same line, and the test asserts the highlight covers that word and
//! nothing to its left.

use crate::common::git_test_helper::GitTestRepo;
use crate::common::harness::{copy_plugin, copy_plugin_lib, EditorTestHarness};
use fresh::config::Config;
use std::fs;

/// The word that changes on the accented line. Distinctive enough to locate
/// on screen, all-ASCII so the columns it occupies are its character count.
const CHANGED_WORD: &str = "REPLACEMENT";
/// What it replaced.
const ORIGINAL_WORD: &str = "PLACEHOLDER";
/// The accented run that sits to the left of the changed word. Six multi-byte
/// characters, so a byte/char confusion shifts the highlight six columns.
const ACCENTED: &str = "añadido ñ·ñ·ñ";

/// Text of screen row `y`.
fn row_text(harness: &EditorTestHarness, y: u16) -> String {
    let buf = harness.buffer();
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
}

/// Row index of the first screen row containing `needle`.
fn row_containing(harness: &EditorTestHarness, needle: &str) -> Option<u16> {
    let height = harness.buffer().area.height;
    (0..height).find(|&y| row_text(harness, y).contains(needle))
}

/// The columns of row `y` whose styling marks them as word-diff emphasis.
/// The stream paints a changed word bold on top of the row's add/remove
/// background, and paints nothing else on that row bold, so "bold" is the
/// discriminator that needs no theme colour hard-coded.
fn emphasised_columns(harness: &EditorTestHarness, y: u16) -> Vec<u16> {
    let buf = harness.buffer();
    (0..buf.area.width)
        .filter(|&x| {
            buf[(x, y)]
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        })
        .collect()
}

fn setup_audit_mode_plugin(repo: &GitTestRepo) {
    let plugins_dir = repo.path.join("plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    copy_plugin(&plugins_dir, "audit_mode");
    copy_plugin_lib(&plugins_dir);
}

/// A repo whose single unstaged change rewrites one word of a line that
/// carries accented text ahead of it.
fn repo_with_accented_modification() -> GitTestRepo {
    let repo = GitTestRepo::new();
    setup_audit_mode_plugin(&repo);
    let body = |word: &str| {
        format!(
            "fn head() {{}}\n\
             fn ctx_one() {{}}\n\
             fn ctx_two() {{}}\n\
             // {ACCENTED} {word} tail\n\
             fn ctx_three() {{}}\n\
             fn tail() {{}}\n"
        )
    };
    repo.create_file("src/lib.rs", &body(ORIGINAL_WORD));
    repo.git_add_all();
    repo.git_commit("baseline");
    repo.create_file("src/lib.rs", &body(CHANGED_WORD));
    repo
}

/// The word-level highlight on an added line must cover the word that
/// actually changed, even when multi-byte characters precede it.
// TODO: git command output differs on Windows; the other review tests skip it.
#[test]
#[cfg_attr(target_os = "windows", ignore)]
fn review_word_highlight_lands_on_the_changed_word_after_non_ascii() {
    let repo = repo_with_accented_modification();
    let mut harness = EditorTestHarness::with_config_and_working_dir(
        140,
        40,
        Config::default(),
        repo.path.clone(),
    )
    .unwrap();
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
        .wait_until(|h| h.screen_to_string().contains(CHANGED_WORD))
        .unwrap();
    // Overlays can land a frame after the text they decorate.
    harness.wait_for_async_quiescence(3).unwrap();

    let screen = harness.screen_to_string();
    let added_y = row_containing(&harness, CHANGED_WORD)
        .unwrap_or_else(|| panic!("`{CHANGED_WORD}` never rendered.\nScreen:\n{screen}"));
    let text = row_text(&harness, added_y);
    let word_start = text
        .find(CHANGED_WORD)
        .unwrap_or_else(|| panic!("row {added_y} lost `{CHANGED_WORD}`: {text:?}"));
    // `find` gives a byte offset into the row's text; the highlight is checked
    // in screen columns, and every character on this row is one column wide.
    let word_col = text[..word_start].chars().count() as u16;
    let word_cols: Vec<u16> = (0..CHANGED_WORD.chars().count() as u16)
        .map(|i| word_col + i)
        .collect();

    let bold = emphasised_columns(&harness, added_y);
    assert!(
        !bold.is_empty(),
        "the added row ({added_y}) carries no word-diff emphasis at all, so \
         this test cannot tell a misplaced highlight from a missing one\n\
         row: {text:?}\nScreen:\n{screen}",
    );
    assert_eq!(
        bold, word_cols,
        "the word-diff highlight on row {added_y} should cover exactly \
         `{CHANGED_WORD}` at columns {word_cols:?}; a highlight shifted left \
         means the accented text before it was measured in characters rather \
         than UTF-8 bytes\nrow: {text:?}\nScreen:\n{screen}",
    );

    // The accented run is unchanged text and must stay unpainted, which is
    // the same defect seen from the other side.
    let accented_start = text
        .find(ACCENTED)
        .unwrap_or_else(|| panic!("row {added_y} lost `{ACCENTED}`: {text:?}"));
    let accented_col = text[..accented_start].chars().count() as u16;
    let accented_end = accented_col + ACCENTED.chars().count() as u16;
    let leaked: Vec<u16> = bold
        .iter()
        .copied()
        .filter(|c| (accented_col..accented_end).contains(c))
        .collect();
    assert!(
        leaked.is_empty(),
        "columns {leaked:?} of the unchanged accented run on row {added_y} \
         are painted as changed\nrow: {text:?}\nScreen:\n{screen}",
    );
}
