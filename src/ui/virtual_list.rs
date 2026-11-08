//! A virtualized, variable-height list, ported from blabbur's message list
//! (`blabbur-client/src/widgets/virtual_list.rs`).
//!
//! [`VirtualLayout`] is the pure math: row heights, lazily recomputed
//! prefix offsets and binary-searched visible ranges. [`VirtualList`] adds
//! keyed rows on top of an egui `ScrollArea`:
//! - rows are drawn only inside the viewport plus an overscan margin;
//!   every other row is empty space of its height;
//! - each drawn row is measured and its height cached per key and content
//!   signature, so a row that changes (a set expands) is measured again;
//! - unmeasured rows use the caller's estimate;
//! - the scroll position is anchored to the first visible row, so rows
//!   added or removed above (a new game, a filter) don't move what the user
//!   is looking at;
//! - [`VirtualList::scroll_to`] brings a row into view (for the keyboard)
//!   and can highlight it briefly.
//!
//! Replay lists are tops-first (newest at the top), so unlike blabbur's chat
//! there is no following of the bottom.

use eframe::egui;
use std::{
    collections::HashMap,
    hash::Hash,
    ops::Range,
    time::{Duration, Instant},
};

/// Pixels drawn above and below the viewport.
pub const OVERSCAN: f32 = 300.0;
/// How long a jumped-to row stays highlighted.
pub const HIGHLIGHT_DURATION: Duration = Duration::from_millis(1_500);
const HEIGHT_EPSILON: f32 = 0.5;

/// Row heights and their prefix offsets.
#[derive(Debug, Clone, Default)]
pub struct VirtualLayout {
    heights: Vec<f32>,
    /// `offsets[i]` is the top of row `i`; `offsets[len]` is the total.
    offsets: Vec<f32>,
    /// Offsets from this row on are stale.
    dirty_from: usize,
}

impl VirtualLayout {
    #[cfg(test)]
    pub fn new(heights: Vec<f32>) -> Self {
        let mut layout = Self::default();
        layout.set_heights(heights);
        layout
    }

    /// Replaces all rows.
    pub fn set_heights(&mut self, heights: Vec<f32>) {
        self.heights = heights;
        self.offsets.clear();
        self.offsets.resize(self.heights.len() + 1, 0.0);
        self.dirty_from = 0;
    }

    pub fn height(&self, index: usize) -> f32 {
        self.heights.get(index).copied().unwrap_or(0.0)
    }

    /// Updates one row; returns whether it changed noticeably.
    pub fn set_height(&mut self, index: usize, height: f32) -> bool {
        let Some(current) = self.heights.get_mut(index) else {
            return false;
        };
        if (*current - height).abs() < HEIGHT_EPSILON {
            return false;
        }
        *current = height;
        self.dirty_from = self.dirty_from.min(index);
        true
    }

    fn ensure(&mut self) {
        if self.dirty_from >= self.heights.len() {
            return;
        }
        let mut offset = self.offsets[self.dirty_from];
        for index in self.dirty_from..self.heights.len() {
            self.offsets[index] = offset;
            offset += self.heights[index];
        }
        self.offsets[self.heights.len()] = offset;
        self.dirty_from = self.heights.len();
    }

    /// Top of row `index` (the total height for `index == len`).
    pub fn offset(&mut self, index: usize) -> f32 {
        self.ensure();
        self.offsets[index.min(self.heights.len())]
    }

    pub fn total_height(&mut self) -> f32 {
        self.offset(self.heights.len())
    }

    /// Row containing content position `y` (clamped to the rows).
    pub fn index_at(&mut self, y: f32) -> usize {
        if self.heights.is_empty() {
            return 0;
        }
        self.ensure();
        let rows = &self.offsets[..self.heights.len()];
        match rows.partition_point(|top| *top <= y) {
            0 => 0,
            after => after - 1,
        }
    }

    /// Rows intersecting `[top - overscan, bottom + overscan]`.
    pub fn visible_range(&mut self, top: f32, bottom: f32, overscan: f32) -> Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let first = self.index_at(top - overscan);
        let last = self.index_at(bottom + overscan);
        first..(last + 1).min(self.heights.len())
    }
}

/// Where a jumped-to row lands in the viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAlign {
    Center,
    /// Only as far as it takes to show the whole row (the keyboard).
    Nearest,
}

#[derive(Debug, Clone, Copy)]
struct Measured {
    height: f32,
    width: f32,
    signature: u64,
}

/// What happened during [`VirtualList::show`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VirtualListOutput {
    pub rendered_rows: usize,
    pub total_rows: usize,
    /// The first and last rows (at least partly) on screen.
    pub first_visible: usize,
    pub last_visible: usize,
}

/// A keyed virtual list; keep one per scroll position (one per view).
#[derive(Debug)]
pub struct VirtualList<K> {
    keys: Vec<K>,
    index: HashMap<K, usize>,
    signatures: Vec<u64>,
    layout: VirtualLayout,
    measured: HashMap<K, Measured>,
    width: f32,
    source_revision: Option<u64>,
    layout_version: u64,
    /// First visible row and its distance from the viewport top.
    anchor: Option<(K, f32, u64)>,
    /// A row to scroll to once it is one, how, and whether to highlight it.
    pending_jump: Option<(K, ScrollAlign, bool)>,
    highlight: Option<(K, Instant)>,
    /// Go back to the top on the next frame.
    reset: bool,
    viewport_height: f32,
    offset: f32,
}

impl<K: Copy + Eq + Hash + std::fmt::Debug> Default for VirtualList<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Eq + Hash + std::fmt::Debug> VirtualList<K> {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            index: HashMap::new(),
            signatures: Vec::new(),
            layout: VirtualLayout::default(),
            measured: HashMap::new(),
            width: 0.0,
            source_revision: None,
            layout_version: 0,
            anchor: None,
            pending_jump: None,
            highlight: None,
            reset: false,
            viewport_height: 0.0,
            offset: 0.0,
        }
    }

    /// Whether rows need rebuilding for `revision` at `width`.
    pub fn needs_sync(&self, revision: u64, width: f32) -> bool {
        self.source_revision != Some(revision) || (self.width - width).abs() >= HEIGHT_EPSILON
    }

    /// Rebuilds the rows from `(key, signature, estimated height)` when the
    /// source revision or the width changed. Measurements are reused while
    /// the signature and width match.
    pub fn sync(&mut self, revision: u64, width: f32, rows: impl Iterator<Item = (K, u64, f32)>) {
        if !self.needs_sync(revision, width) {
            return;
        }
        self.width = width;
        self.source_revision = Some(revision);
        let count = rows.size_hint().0;
        self.keys.clear();
        self.keys.reserve(count);
        self.signatures.clear();
        self.signatures.reserve(count);
        self.index.clear();
        self.index.reserve(count);
        let mut heights = Vec::with_capacity(count);
        for (position, (key, signature, estimate)) in rows.enumerate() {
            let height = self
                .measured
                .get(&key)
                .filter(|m| m.signature == signature && (m.width - width).abs() < HEIGHT_EPSILON)
                .map_or(estimate, |m| m.height);
            self.keys.push(key);
            self.signatures.push(signature);
            self.index.insert(key, position);
            heights.push(height);
        }
        let count = self.keys.len();
        if self.measured.len() > count * 2 + 256 {
            let index = &self.index;
            self.measured.retain(|key, _| index.contains_key(key));
        }
        self.layout.set_heights(heights);
        self.layout_version += 1;
    }

    /// Scrolls to `key` once it is a row; `highlight` flashes it.
    pub fn scroll_to(&mut self, key: K, align: ScrollAlign, highlight: bool) {
        self.pending_jump = Some((key, align, highlight));
    }

    /// Back to the first row (a new filter, another view).
    pub fn scroll_to_top(&mut self) {
        self.reset = true;
        self.pending_jump = None;
    }

    /// The row jumped to last and how strongly it still shows (1 right
    /// after the jump, fading to 0).
    pub fn highlighted(&self) -> Option<(K, f32)> {
        let (key, since) = self.highlight?;
        let elapsed = Instant::now().saturating_duration_since(since);
        let strength = 1.0 - (elapsed.as_secs_f32() / HIGHLIGHT_DURATION.as_secs_f32()).min(1.0);
        (strength > 0.0).then_some((key, strength))
    }

    /// Target scroll offset for this frame, if the list must move.
    fn target_offset(&mut self) -> Option<f32> {
        if std::mem::take(&mut self.reset) {
            self.anchor = None;
            return Some(0.0);
        }
        let viewport = self.viewport_height;
        if let Some((key, align, highlight)) = self.pending_jump
            && let Some(&index) = self.index.get(&key)
        {
            self.pending_jump = None;
            if highlight {
                self.highlight = Some((key, Instant::now()));
            }
            let top = self.layout.offset(index);
            let height = self.layout.height(index);
            let slack = (viewport - height).max(0.0);
            let target = match align {
                ScrollAlign::Center => top - slack / 2.0,
                ScrollAlign::Nearest => {
                    if top < self.offset {
                        top
                    } else if top + height > self.offset + viewport {
                        top + height - viewport
                    } else {
                        return None;
                    }
                }
            };
            return Some(target.max(0.0));
        }
        let (key, delta, version) = self.anchor?;
        if version == self.layout_version {
            return None;
        }
        let index = *self.index.get(&key)?;
        Some((self.layout.offset(index) - delta).max(0.0))
    }

    /// Draws the list. `render(ui, index, key)` draws one row into a ui of
    /// the list's width; its height is measured from what it allocates.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id_salt: impl egui::AsIdSalt,
        mut render: impl FnMut(&mut egui::Ui, usize, K),
    ) -> VirtualListOutput {
        let mut list_ui = clipped_to_room(ui);
        self.viewport_height = list_ui.available_height();
        let target = self.target_offset();

        let mut area = egui::ScrollArea::vertical()
            .id_salt(id_salt)
            .auto_shrink([false, false])
            .animated(false);
        if let Some(target) = target {
            area = area.vertical_scroll_offset(target);
        }

        let keys = &self.keys;
        let layout = &mut self.layout;
        // (row, measured height)
        let mut measurements: Vec<(usize, f32)> = Vec::new();
        let mut visible = (usize::MAX, 0usize);
        let output = area.show_viewport(&mut list_ui, |ui, viewport| {
            let total = layout.total_height();
            ui.set_height(total);
            let range = layout.visible_range(viewport.min.y, viewport.max.y, OVERSCAN);
            let origin = ui.max_rect().min;
            let width = ui.max_rect().width();
            let mut y = origin.y + layout.offset(range.start);
            for index in range {
                let key = keys[index];
                let rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x, y),
                    egui::vec2(width, layout.height(index).max(1.0)),
                );
                // A row's id comes from its key, not its place, so the row
                // under the pointer keeps its hover when rows above change.
                let response = ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .id_salt(("virtual-row", key_hash(&key)))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| render(ui, index, key),
                );
                let height = response.response.rect.height();
                let top = y - origin.y;
                if top < viewport.max.y && top + height > viewport.min.y {
                    visible.0 = visible.0.min(index);
                    visible.1 = visible.1.max(index);
                }
                measurements.push((index, height));
                y += height;
            }
        });

        self.viewport_height = output.inner_rect.height();
        let offset = output.state.clamped_offset().y;
        self.offset = offset;

        if !self.keys.is_empty() {
            let row = self.layout.index_at(offset);
            let delta = self.layout.offset(row) - offset;
            self.anchor = Some((self.keys[row], delta, self.layout_version));
        } else {
            self.anchor = None;
        }

        let rendered_rows = measurements.len();
        let mut changed = false;
        for (index, height) in measurements {
            let key = self.keys[index];
            if self.layout.set_height(index, height) {
                changed = true;
            }
            self.measured.insert(
                key,
                Measured {
                    height,
                    width: self.width,
                    signature: self.signatures[index],
                },
            );
        }
        if changed {
            self.layout_version += 1;
            ui.ctx().request_repaint();
        }
        if self.pending_jump.is_some()
            || self
                .highlight
                .is_some_and(|(_, since)| since.elapsed() < HIGHLIGHT_DURATION)
        {
            ui.ctx().request_repaint();
        }

        VirtualListOutput {
            rendered_rows,
            total_rows: self.keys.len(),
            first_visible: if visible.0 == usize::MAX {
                0
            } else {
                visible.0
            },
            last_visible: visible.1,
        }
    }
}

/// A child of `ui` for a scrolling list in the room `ui` has left (which it
/// takes), clipped where that room ends.
pub fn clipped_to_room(ui: &mut egui::Ui) -> egui::Ui {
    let room = ui.available_rect_before_wrap();
    let mut list = ui.new_child(egui::UiBuilder::new().max_rect(room).layout(*ui.layout()));
    list.set_clip_rect(ui.clip_rect().intersect(room));
    ui.advance_cursor_after_rect(room);
    list
}

fn key_hash<K: Hash>(key: &K) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_and_totals_follow_heights() {
        let mut layout = VirtualLayout::new(vec![10.0, 20.0, 30.0]);
        assert_eq!(layout.offset(0), 0.0);
        assert_eq!(layout.offset(1), 10.0);
        assert_eq!(layout.offset(2), 30.0);
        assert_eq!(layout.total_height(), 60.0);
        assert!(layout.set_height(1, 50.0));
        assert!(!layout.set_height(1, 50.2), "sub-pixel changes are ignored");
        assert!(!layout.set_height(9, 1.0));
        assert_eq!(layout.offset(2), 60.0);
        assert_eq!(layout.total_height(), 90.0);
    }

    #[test]
    fn index_at_and_visible_range_use_row_boundaries() {
        let mut layout = VirtualLayout::new(vec![10.0; 100]);
        assert_eq!(layout.index_at(-5.0), 0);
        assert_eq!(layout.index_at(9.9), 0);
        assert_eq!(layout.index_at(10.0), 1);
        assert_eq!(layout.index_at(5_000.0), 99);
        assert_eq!(layout.visible_range(100.0, 150.0, 0.0), 10..16);
        assert_eq!(layout.visible_range(100.0, 150.0, 20.0), 8..18);
        assert_eq!(VirtualLayout::default().visible_range(0.0, 10.0, 0.0), 0..0);
    }

    #[test]
    fn measurements_are_reused_until_signature_or_width_changes() {
        let mut list = VirtualList::<u32>::new();
        let rows = |sig: u64| (0..3u32).map(move |key| (key, sig, 20.0));
        list.sync(1, 300.0, rows(7));
        list.measured.insert(
            1,
            Measured {
                height: 55.0,
                width: 300.0,
                signature: 7,
            },
        );
        list.sync(2, 300.0, rows(7));
        assert_eq!(list.layout.height(1), 55.0);
        list.sync(3, 300.0, rows(8));
        assert_eq!(list.layout.height(1), 20.0, "content changed");
        assert!(!list.needs_sync(3, 300.0));
        assert!(list.needs_sync(3, 400.0));
    }

    #[test]
    fn a_large_list_only_draws_what_is_on_screen() {
        let ctx = egui::Context::default();
        let mut list = VirtualList::<u32>::new();
        let mut drawn = 0;
        for _ in 0..3 {
            drawn = 0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    list.sync(
                        1,
                        ui.available_width(),
                        (0..10_000u32).map(|k| (k, 0, 36.0)),
                    );
                    list.show(ui, "test", |ui, _, _| {
                        drawn += 1;
                        ui.allocate_exact_size(egui::vec2(10.0, 36.0), egui::Sense::hover());
                    });
                },
            );
            // No renderer takes the font atlas here.
            output.textures_delta.clear();
        }
        assert!(drawn < 60, "drew {drawn} rows of 10,000");
    }
}
