use crate::ui::pane::ShellPaneProjection;

pub(crate) struct SceneFrameProjections<'a> {
    projections: Vec<ShellPaneProjection<'a>>,
}

impl<'a> SceneFrameProjections<'a> {
    pub(crate) fn new(projections: Vec<ShellPaneProjection<'a>>, _layout_us: u128) -> Self {
        Self { projections }
    }

    pub(crate) fn projections(&self) -> &[ShellPaneProjection<'a>] {
        &self.projections
    }
}
