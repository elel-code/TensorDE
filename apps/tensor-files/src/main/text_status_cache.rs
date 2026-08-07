use std::sync::Arc;

use crate::ui::pane::ShellPaneView;
use crate::ui::status::ShellPaneStatus;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneStatusTextCacheStats {
    pub(crate) entries: usize,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaneStatusTextKey {
    total: usize,
    folders: usize,
    selected: usize,
    visible: usize,
    filtered: usize,
    show_hidden: bool,
    filter_active: bool,
    zoom_percent: i32,
}

impl PaneStatusTextKey {
    fn new(
        pane: ShellPaneView<'_>,
        visible: usize,
        show_hidden: bool,
        filter_active: bool,
        zoom_percent: i32,
    ) -> Self {
        Self {
            total: pane.entries.len(),
            folders: pane.dir_count,
            selected: pane.selection.len(),
            visible,
            filtered: pane.filtered_entry_count(),
            show_hidden,
            filter_active,
            zoom_percent,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PaneStatusText {
    pub(crate) primary: Arc<str>,
    pub(crate) qualifier: Arc<str>,
    pub(crate) zoom: Arc<str>,
}

#[derive(Debug)]
struct CachedPaneStatusText {
    key: PaneStatusTextKey,
    text: PaneStatusText,
}

#[derive(Debug)]
pub(crate) struct PaneStatusTextCache {
    entries: [Option<CachedPaneStatusText>; 2],
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl PaneStatusTextCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            #[cfg(test)]
            hits: 0,
            #[cfg(test)]
            misses: 0,
        }
    }

    pub(crate) fn labels(
        &mut self,
        pane_index: usize,
        pane: ShellPaneView<'_>,
        visible: usize,
        show_hidden: bool,
        filter_active: bool,
        zoom_percent: i32,
    ) -> PaneStatusText {
        let key = PaneStatusTextKey::new(pane, visible, show_hidden, filter_active, zoom_percent);
        if let Some(cached) = self.entries[pane_index].as_ref()
            && cached.key == key
        {
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return cached.text.clone();
        }

        #[cfg(test)]
        {
            self.misses += 1;
        }
        let text = Self::build_labels(pane, visible, show_hidden, filter_active, zoom_percent);
        self.entries[pane_index] = Some(CachedPaneStatusText {
            key,
            text: text.clone(),
        });
        text
    }

    pub(crate) fn build_labels(
        pane: ShellPaneView<'_>,
        visible: usize,
        show_hidden: bool,
        filter_active: bool,
        zoom_percent: i32,
    ) -> PaneStatusText {
        let status = ShellPaneStatus::for_view(pane, visible, show_hidden, filter_active);
        PaneStatusText {
            primary: Arc::from(status.primary),
            qualifier: Arc::from(status.qualifier),
            zoom: Arc::from(format!("{zoom_percent}%")),
        }
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> PaneStatusTextCacheStats {
        PaneStatusTextCacheStats {
            entries: self.entries.iter().filter(|entry| entry.is_some()).count(),
            hits: self.hits,
            misses: self.misses,
        }
    }
}
