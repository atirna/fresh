//! A large review is mounted in pieces: the first screenful lands
//! immediately and the rest is appended behind it. Nothing else in the
//! plugin knows that happened — row numbers, byte offsets and the maps
//! built from them have to come out as if one pass had built the whole
//! stream.
//!
//! The risk is entirely in the seams. Each chunk resumes a running byte
//! offset and row counter from where the last one stopped, so an error
//! there does not corrupt the chunk it happens in — it silently shifts
//! everything after it, and the damage shows up as navigation landing on
//! the wrong row far from the file that caused it.
//!
//! So this drives the real command against a review too big for one
//! chunk, then walks to the *last* file and checks what it lands on.
//! Anything wrong with the seams puts that landing somewhere else.

use crate::common::git_test_helper::GitTestRepo;
use crate::common::harness::{copy_plugin, copy_plugin_lib, EditorTestHarness};
use crossterm::event::{KeyCode, KeyModifiers};
use fresh::config::Config;
use std::fs;

/// Files in the fixture. The stream's first chunk is 400 rows and chunks
/// double, so this has to clear 400 + 800 + 1600 to exercise more than
/// one seam.
const FILES: usize = 10;
/// Lines per file, every tenth one changed.
const LINES: usize = 320;

/// Marker in the first file — must be on screen as soon as the review opens.
const FIRST_MARKER: &str = "FIRST_FILE_MARKER";
/// Marker in the last file — only reachable once every chunk has landed.
const LAST_MARKER: &str = "LAST_FILE_MARKER";

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

/// Screen row carrying the cursor-line bar, found by its background: the
/// bar washes its whole row in the selection colour, and no other row in
/// the diff carries more than a cell or two of it. The terminal caret is
/// not it — in this layout that sits in the toolbar.
fn cursor_bar_row(harness: &EditorTestHarness) -> Option<u16> {
    let selection_bg = harness.editor().theme().selection_bg;
    let area = harness.buffer().area;
    (0..area.height)
        .map(|y| {
            let washed = (0..area.width)
                .filter(|&x| {
                    harness
                        .get_cell_style(x, y)
                        .is_some_and(|style| style.bg == Some(selection_bg))
                })
                .count();
            (y, washed)
        })
        .max_by_key(|&(_, washed)| washed)
        .filter(|&(_, washed)| washed > 3)
        .map(|(y, _)| y)
}

/// A review spread over enough rows to need several chunks, with the first
/// and last files individually identifiable.
/// The plugin mounts the first 400 rows and appends the rest. Both tests
/// are about what happens across those seams, so both first check the
/// stream actually got past the first chunk — otherwise a fixture that
/// shrank would leave them asserting about a stream with no seams in it.
fn assert_mounted_past_the_first_chunk(harness: &EditorTestHarness) {
    const FIRST_CHUNK: usize = 400;
    let rows = harness
        .editor()
        .active_state()
        .buffer
        .line_count()
        .unwrap_or(0);
    assert!(
        rows > FIRST_CHUNK + 1,
        "the stream mounted {rows} rows, which fits in the first chunk of \
         {FIRST_CHUNK} — this fixture no longer exercises a chunk seam",
    );
}

fn repo_with_a_multi_chunk_review() -> GitTestRepo {
    let repo = GitTestRepo::new();
    let plugins_dir = repo.path.join("plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    copy_plugin(&plugins_dir, "audit_mode");
    copy_plugin_lib(&plugins_dir);

    let body = |f: usize, changed: bool| {
        (0..LINES)
            .map(|l| {
                // Markers must sit on a *changed* line: unchanged lines
                // outside a hunk's context never reach the stream.
                let marker = if l == 10 && f == 0 {
                    FIRST_MARKER
                } else if l == 10 && f == FILES - 1 {
                    LAST_MARKER
                } else {
                    "plain"
                };
                if changed && l % 10 == 0 {
                    format!("fn f{f}_{l}() {{ /* {marker} changed */ }}")
                } else {
                    format!("fn f{f}_{l}() {{ /* {marker} */ }}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    for f in 0..FILES {
        repo.create_file(&format!("src/file{f}.rs"), &body(f, false));
    }
    repo.git_add_all();
    repo.git_commit("baseline");
    for f in 0..FILES {
        repo.create_file(&format!("src/file{f}.rs"), &body(f, true));
    }
    repo
}

fn open_review(repo: &GitTestRepo) -> EditorTestHarness {
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
}

/// Walking to the last file must put the *cursor* on that file's header.
///
/// Navigation resolves a file to a row through the map the pass fills in,
/// so this compares that map against the buffer the same pass produced.
/// An off-by-one at any seam moves the landing without moving the text,
/// which is why this reads the cursor rather than the screen.
// TODO: git command output differs on Windows; the other review tests skip it.
#[test]
#[cfg_attr(target_os = "windows", ignore)]
fn walking_to_a_late_file_lands_the_cursor_on_its_header() {
    let repo = repo_with_a_multi_chunk_review();
    let mut harness = open_review(&repo);
    harness
        .wait_until(|h| h.screen_to_string().contains(FIRST_MARKER))
        .unwrap();
    // Let every remaining chunk land before navigating past the first.
    harness.wait_for_async_quiescence(8).unwrap();
    assert_mounted_past_the_first_chunk(&harness);

    // `.` steps to the next file; walking the whole list ends on the last.
    for _ in 0..FILES - 1 {
        harness
            .send_key(KeyCode::Char('.'), KeyModifiers::NONE)
            .unwrap();
    }
    harness.wait_for_async_quiescence(4).unwrap();

    let screen = harness.screen_to_string();
    let last_file = format!("src/file{}.rs", FILES - 1);
    let header_y = row_containing(&harness, &last_file)
        .unwrap_or_else(|| panic!("the last file's header never rendered.\nScreen:\n{screen}"));
    let cursor_y = cursor_bar_row(&harness)
        .unwrap_or_else(|| panic!("no cursor-line bar on screen:\n{screen}"));
    assert_eq!(
        cursor_y,
        header_y,
        "walking to {last_file} left the cursor on row {cursor_y} while its \
         header is drawn on row {header_y}; the row a later chunk recorded \
         for that file does not match where it actually mounted\n\
         cursor row: {:?}\nheader row: {:?}\nScreen:\n{screen}",
        row_text(&harness, cursor_y),
        row_text(&harness, header_y),
    );
}

/// Stepping through hunks deep into the stream must land on hunk headers.
///
/// Files and hunks are recorded in separate maps by the same pass, so a
/// seam can leave one usable and the other off.
// TODO: git command output differs on Windows; the other review tests skip it.
#[test]
#[cfg_attr(target_os = "windows", ignore)]
fn stepping_through_hunks_stays_on_hunk_headers() {
    let repo = repo_with_a_multi_chunk_review();
    let mut harness = open_review(&repo);
    harness
        .wait_until(|h| h.screen_to_string().contains(FIRST_MARKER))
        .unwrap();
    harness.wait_for_async_quiescence(8).unwrap();
    assert_mounted_past_the_first_chunk(&harness);

    // Enough steps to walk well past the first chunk's worth of hunks.
    for step in 0..60 {
        harness
            .send_key(KeyCode::Char('n'), KeyModifiers::NONE)
            .unwrap();
        harness.wait_for_async_quiescence(2).unwrap();
        let Some(cursor_y) = cursor_bar_row(&harness) else {
            continue;
        };
        let text = row_text(&harness, cursor_y);
        assert!(
            text.contains("@@"),
            "after {step} steps the cursor sits on row {cursor_y}, which is \
             not a hunk header; the row recorded for that hunk does not \
             match where it mounted\nrow: {text:?}\nScreen:\n{}",
            harness.screen_to_string(),
        );
    }
}
