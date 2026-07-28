use std::collections::BTreeSet;

use fika_core::{ViewPoint, ViewRect};

use crate::ui::metrics::RUBBER_BAND_START_THRESHOLD;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NavigationAction {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectionClick {
    pub(crate) point: ViewPoint,
    pub(crate) extend: bool,
    pub(crate) toggle: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RubberBand {
    pub(crate) start: ViewPoint,
    pub(crate) current: ViewPoint,
    pub(crate) active: bool,
    pub(crate) mode: RubberBandMode,
    pub(crate) base_selection: ShellSelection,
}

impl RubberBand {
    pub(crate) fn new(
        start: ViewPoint,
        mode: RubberBandMode,
        base_selection: ShellSelection,
    ) -> Self {
        Self {
            start,
            current: start,
            active: false,
            mode,
            base_selection,
        }
    }

    pub(crate) fn update(&mut self, current: ViewPoint) {
        self.current = current;
        if !self.active
            && ((self.current.x - self.start.x).abs() + (self.current.y - self.start.y).abs())
                >= RUBBER_BAND_START_THRESHOLD
        {
            self.active = true;
        }
    }

    pub(crate) fn active_rect(&self) -> Option<ViewRect> {
        self.active
            .then(|| rect_from_points(self.start, self.current))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RubberBandMode {
    Replace,
    Extend,
    Toggle,
}

impl RubberBandMode {
    pub(crate) fn from_modifiers(extend: bool, toggle: bool) -> Self {
        if toggle {
            Self::Toggle
        } else if extend {
            Self::Extend
        } else {
            Self::Replace
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellSelection {
    pub(crate) selected: BTreeSet<usize>,
    pub(crate) anchor: Option<usize>,
    pub(crate) focus: Option<usize>,
}

impl ShellSelection {
    pub(crate) fn contains(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub(crate) fn len(&self) -> usize {
        self.selected.len()
    }

    pub(crate) fn focus_or_first_selected(&self) -> Option<usize> {
        self.focus.or_else(|| self.selected.iter().next().copied())
    }

    pub(crate) fn select_indexes(&mut self, indexes: &[usize]) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected = indexes.iter().copied().collect();
        self.anchor = indexes.first().copied();
        self.focus = indexes.last().copied();

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn retain_indexes(&mut self, indexes: &[usize]) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected
            .retain(|index| indexes.binary_search(index).is_ok());
        self.anchor = self
            .anchor
            .filter(|index| self.selected.contains(index))
            .or_else(|| self.selected.iter().next().copied());
        self.focus = self
            .focus
            .filter(|index| self.selected.contains(index))
            .or_else(|| self.selected.iter().next_back().copied());

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn clear(&mut self) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected.clear();
        self.anchor = None;
        self.focus = None;

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn focus_selected(&mut self, index: usize) -> bool {
        if !self.selected.contains(&index) {
            return false;
        }
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        if self.anchor.is_none() {
            self.anchor = Some(index);
        }
        self.focus = Some(index);

        old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn apply_click(&mut self, hit: Option<usize>, extend: bool, toggle: bool) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        match hit {
            Some(index) if extend => {
                let anchor = self.anchor.unwrap_or(index);
                self.selected.clear();
                for item in anchor.min(index)..=anchor.max(index) {
                    self.selected.insert(item);
                }
                self.anchor = Some(anchor);
                self.focus = Some(index);
            }
            Some(index) if toggle => {
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                    self.anchor = Some(index);
                    self.focus = Some(index);
                } else if self.anchor == Some(index) {
                    self.anchor = self.selected.iter().next().copied();
                    self.focus = self.anchor;
                } else {
                    self.focus = Some(index);
                }
            }
            Some(index) => {
                self.selected.clear();
                self.selected.insert(index);
                self.anchor = Some(index);
                self.focus = Some(index);
            }
            None if !extend && !toggle => {
                self.selected.clear();
                self.anchor = None;
                self.focus = None;
            }
            None => {}
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn apply_navigation(&mut self, target: usize, extend: bool) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        if extend {
            let anchor = self.anchor.or(self.focus).unwrap_or(target);
            self.selected.clear();
            for item in anchor.min(target)..=anchor.max(target) {
                self.selected.insert(item);
            }
            self.anchor = Some(anchor);
            self.focus = Some(target);
        } else {
            self.selected.clear();
            self.selected.insert(target);
            self.anchor = Some(target);
            self.focus = Some(target);
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    pub(crate) fn apply_rubber_band(
        &mut self,
        base: &ShellSelection,
        indexes: &[usize],
        mode: RubberBandMode,
    ) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        match mode {
            RubberBandMode::Replace => {
                self.selected.clear();
                self.selected.extend(indexes.iter().copied());
                self.anchor = indexes.first().copied();
                self.focus = indexes.last().copied();
            }
            RubberBandMode::Extend => {
                *self = base.clone();
                self.selected.extend(indexes.iter().copied());
                if let Some(last) = indexes.last().copied() {
                    self.anchor = self.anchor.or_else(|| indexes.first().copied());
                    self.focus = Some(last);
                }
            }
            RubberBandMode::Toggle => {
                *self = base.clone();
                for index in indexes {
                    if !self.selected.remove(index) {
                        self.selected.insert(*index);
                    }
                }
                if self.selected.is_empty() {
                    self.anchor = None;
                    self.focus = None;
                } else if let Some(last) = indexes.last().copied() {
                    self.focus = Some(last);
                    if self
                        .anchor
                        .is_none_or(|anchor| !self.selected.contains(&anchor))
                    {
                        self.anchor = self.selected.iter().next().copied();
                    }
                }
            }
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }
}

fn rect_from_points(start: ViewPoint, current: ViewPoint) -> ViewRect {
    let x = start.x.min(current.x);
    let y = start.y.min(current.y);
    ViewRect {
        x,
        y,
        width: start.x.max(current.x) - x,
        height: start.y.max(current.y) - y,
    }
}
