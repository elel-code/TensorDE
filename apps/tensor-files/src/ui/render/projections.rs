use arrayvec::ArrayVec;

use crate::ui::pane::ShellPaneProjection;
use crate::{
    ShellFrameProjectionStaging, ShellPreparedFrameProjectionLayouts, ShellPreparedPaneProjection,
};

pub(crate) const FRAME_PANE_CAPACITY: usize = 2;

pub(crate) struct SceneFrameProjections<'a> {
    projections: ArrayVec<ShellPaneProjection<'a>, FRAME_PANE_CAPACITY>,
    staging: ShellFrameProjectionStaging,
}

impl<'a> SceneFrameProjections<'a> {
    pub(crate) fn new(
        projections: ArrayVec<ShellPaneProjection<'a>, FRAME_PANE_CAPACITY>,
        staging: ShellFrameProjectionStaging,
    ) -> Self {
        Self {
            projections,
            staging,
        }
    }

    pub(crate) fn projections(&self) -> &[ShellPaneProjection<'a>] {
        self.projections.as_slice()
    }

    pub(crate) fn into_prepared_layouts(self) -> ShellPreparedFrameProjectionLayouts {
        let Self {
            projections,
            staging,
        } = self;
        let ShellFrameProjectionStaging {
            mut layouts,
            visible_items,
        } = staging;
        layouts.clear();
        layouts.extend(
            projections
                .into_iter()
                .map(|projection| ShellPreparedPaneProjection {
                    geometry: projection.geometry,
                    visible_items: projection.visible_items,
                    scroll_metrics: projection.scroll_metrics,
                }),
        );
        ShellPreparedFrameProjectionLayouts {
            layouts,
            recycled_visible_items: visible_items,
        }
    }

    pub(crate) fn recycle(self) -> ShellFrameProjectionStaging {
        let Self {
            projections,
            mut staging,
        } = self;
        for mut projection in projections {
            projection.visible_items.clear();
            staging.visible_items[projection.geometry.kind.index()] = projection.visible_items;
        }
        staging
    }
}
