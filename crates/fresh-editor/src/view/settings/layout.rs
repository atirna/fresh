//! Settings layout for hit testing
//!
//! Tracks the layout of rendered settings UI elements for mouse interaction.

use crate::view::ui::point_in_rect;
use ratatui::layout::Rect;

/// Layout information for the entire settings UI
#[derive(Debug, Clone, Default)]
pub struct SettingsLayout {
    /// The modal area
    pub modal_area: Rect,
    /// Search result items (page_index, item_index, area)
    pub search_results: Vec<SearchResultLayout>,
    /// The **narrow** layout's horizontal category strip: (index, area).
    ///
    /// **Four of this family's five are gone.** `sections`,
    /// `category_disclosures`, `categories_panel_area` and
    /// `categories_scrollbar_area` belonged to the wide layout's tree, and
    /// existed so a chain of `point_in_rect` could turn a cell back into a
    /// row — and, for the chevron, into a *column of* a row. That tree is a
    /// `widgets::List` now (`view::shell::settings::categories`): a row knows
    /// its own index, the chevron is a node beside the label, and the window
    /// and its bar are the viewport's. The strip below forty columns is still
    /// painted, so it still records these.
    pub categories: Vec<(usize, Rect)>,
    /// Search results scrollbar area (for search results scrolling)
    pub search_scrollbar_area: Option<Rect>,
    /// Search results content area (for scroll wheel detection)
    pub search_results_area: Option<Rect>,
}

/// Layout info for a search result
#[derive(Debug, Clone)]
pub struct SearchResultLayout {
    /// Absolute index into the state's `search_results` list. Only the
    /// visible rows are registered in the layout, so this is NOT the same
    /// as the position within `SettingsLayout::search_results` once the
    /// list is scrolled (#2860).
    pub result_index: usize,
    /// Page index (category)
    pub page_index: usize,
    /// Item index within the page
    pub item_index: usize,
    /// Full area for this result
    pub area: Rect,
}

impl SettingsLayout {
    /// Create a new layout for the given modal area
    pub fn new(modal_area: Rect) -> Self {
        Self {
            modal_area,
            categories: Vec::new(),
            search_results: Vec::new(),
            search_scrollbar_area: None,
            search_results_area: None,
        }
    }

    /// Register a row of the narrow layout's horizontal category strip.
    pub fn add_category(&mut self, index: usize, area: Rect) {
        self.categories.push((index, area));
    }

    /// Add a search result to the layout. `result_index` is the absolute
    /// index into the state's `search_results` list (not the on-screen
    /// slot), so hit-testing keeps working when the list is scrolled.
    pub fn add_search_result(
        &mut self,
        result_index: usize,
        page_index: usize,
        item_index: usize,
        area: Rect,
    ) {
        self.search_results.push(SearchResultLayout {
            result_index,
            page_index,
            item_index,
            area,
        });
    }

    /// Hit test a position and return what was clicked
    pub fn hit_test(&self, x: u16, y: u16) -> Option<SettingsHit> {
        // Check if outside modal
        if !point_in_rect(self.modal_area, x, y) {
            return Some(SettingsHit::Outside);
        }

        // **The footer's five buttons and the page header's `[Clear …]` are
        // the tree's**, and answer their own presses. What was here was six
        // more rectangles the painter filed as it drew them.

        // The wide layout's tree answered here through four ordered lists of
        // rectangle — chevrons, then sections, then rows, then the panel. Its
        // rows answer for themselves now; what is left is the narrow strip.
        for (index, area) in &self.categories {
            if point_in_rect(*area, x, y) {
                return Some(SettingsHit::Category(*index));
            }
        }

        // Check search scrollbar (before search results, for click/drag priority)
        if let Some(ref scrollbar) = self.search_scrollbar_area {
            if point_in_rect(*scrollbar, x, y) {
                return Some(SettingsHit::SearchScrollbar);
            }
        }

        // Check search results (before regular items, since they replace the
        // item list during search). The hit carries the ABSOLUTE result
        // index: only visible rows are registered here, so the position in
        // this vec is a viewport slot and would be off by the scroll offset
        // once the list is scrolled (#2860).
        for result in &self.search_results {
            if point_in_rect(result.area, x, y) {
                return Some(SettingsHit::SearchResult(result.result_index));
            }
        }

        // Check search results area (for scroll wheel when over the results area but not on a result)
        if let Some(ref area) = self.search_results_area {
            if point_in_rect(*area, x, y) {
                return Some(SettingsHit::SearchResultsPanel);
            }
        }

        // **The body's cards answer their own presses.** Every control filed
        // a `ControlLayoutInfo` rectangle here so a click could be compared
        // against what had been drawn — a toggle's chip, a number's value
        // cell, a dropdown's button and every open option row, each row of a
        // map and each of a text list. They are the runtime's own hits now,
        // resolved by `Editor::settings_widget_hit`, and the body's window is
        // a `viewport` whose wheel and scrollbar are the framework's.

        Some(SettingsHit::Background)
    }
}

/// Result of a hit test on the settings UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    /// Click outside the modal
    Outside,
    /// Click on modal background
    Background,
    /// Click on a category (index)
    Category(usize),
    /// Click on a setting item (index)
    Item(usize),
    /// Click on a search result (absolute index into the state's
    /// `search_results`, not the on-screen slot)
    SearchResult(usize),
    /// Click on toggle control
    ControlToggle(usize),
    /// Click on number decrement button
    ControlDecrement(usize),
    /// Click on number increment button
    ControlIncrement(usize),
    /// Click on the value area between the brackets of a number control —
    /// should focus the item and enter inline editing mode.
    ControlNumberValue(usize),
    /// Click on dropdown button
    ControlDropdown(usize),
    /// Click on dropdown option (item_idx, option_idx)
    ControlDropdownOption(usize, usize),
    /// Click on text input
    ControlText(usize),
    /// Click on text list row (item_idx, row_idx)
    ControlTextListRow(usize, usize),
    /// Click on map row (item_idx, row_idx)
    ControlMapRow(usize, usize),
    /// Click on map add-new row (item_idx)
    ControlMapAddNew(usize),
    /// Click on inherit button (item_idx) - unset a nullable value
    ControlInherit(usize),
    /// Click on dual-list available row (item_idx, row_idx)
    ControlDualListAvailable(usize, usize),
    /// Click on dual-list included row (item_idx, row_idx)
    ControlDualListIncluded(usize, usize),
    /// Click on dual-list add button (item_idx)
    ControlDualListAdd(usize),
    /// Click on dual-list remove button (item_idx)
    ControlDualListRemove(usize),
    /// Click on dual-list move-up button (item_idx)
    ControlDualListMoveUp(usize),
    /// Click on dual-list move-down button (item_idx)
    ControlDualListMoveDown(usize),
    /// Click on layer button
    LayerButton,
    /// Click on edit config file button
    EditButton,
    /// Click on save button
    SaveButton,
    /// Click on cancel button
    CancelButton,
    /// Click on reset button
    ResetButton,
    /// Click on clear category button (for nullable categories)
    ClearCategoryButton,
    /// Click on settings panel scrollbar
    Scrollbar,
    /// Click on settings panel (scrollable area)
    SettingsPanel,
    /// Click on search results scrollbar
    SearchScrollbar,
    /// Click on search results area (for scroll wheel)
    SearchResultsPanel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_creation() {
        let modal = Rect::new(10, 5, 80, 30);
        let mut layout = SettingsLayout::new(modal);

        layout.add_category(0, Rect::new(11, 6, 20, 1));
        layout.add_category(1, Rect::new(11, 7, 20, 1));

        assert_eq!(layout.categories.len(), 2);
    }

    #[test]
    fn test_hit_test_outside() {
        let modal = Rect::new(10, 5, 80, 30);
        let layout = SettingsLayout::new(modal);

        assert_eq!(layout.hit_test(0, 0), Some(SettingsHit::Outside));
        assert_eq!(layout.hit_test(5, 5), Some(SettingsHit::Outside));
    }

    #[test]
    fn test_hit_test_category() {
        let modal = Rect::new(10, 5, 80, 30);
        let mut layout = SettingsLayout::new(modal);

        layout.add_category(0, Rect::new(11, 6, 20, 1));
        layout.add_category(1, Rect::new(11, 7, 20, 1));

        assert_eq!(layout.hit_test(15, 6), Some(SettingsHit::Category(0)));
        assert_eq!(layout.hit_test(15, 7), Some(SettingsHit::Category(1)));
    }

    /// Reproducer for issue #2860: only VISIBLE search results are registered
    /// in the layout, so when the list is scrolled the first registered row
    /// is not result 0. `hit_test` must report the absolute result index the
    /// row was registered with, not the row's position in the layout vec —
    /// otherwise hover and click resolve to a result `scroll_offset` rows
    /// above the pointer.
    #[test]
    fn test_hit_test_search_result_scrolled_uses_absolute_index() {
        let modal = Rect::new(0, 0, 100, 40);
        let mut layout = SettingsLayout::new(modal);

        // Scrolled viewport: visible rows are results 3, 4, 5 (3 rows each).
        layout.add_search_result(3, 0, 3, Rect::new(25, 3, 70, 3));
        layout.add_search_result(4, 0, 4, Rect::new(25, 6, 70, 3));
        layout.add_search_result(5, 0, 5, Rect::new(25, 9, 70, 3));

        assert_eq!(layout.hit_test(30, 4), Some(SettingsHit::SearchResult(3)));
        assert_eq!(layout.hit_test(30, 7), Some(SettingsHit::SearchResult(4)));
        assert_eq!(layout.hit_test(30, 10), Some(SettingsHit::SearchResult(5)));
    }
}
