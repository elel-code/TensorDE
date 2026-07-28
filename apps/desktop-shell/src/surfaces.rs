use wayland_client_runtime::{
    LayerAnchor, LayerEdge, LayerKeyboardInteractivity, LayerMargins, LayerSurfaceAttributes,
    LayerSurfaceLayer, LayerSurfaceState, LogicalSize, OutputId,
};

use crate::{ShellComponent, ShellLayout};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfacePlan {
    pub component: ShellComponent,
    pub attributes: LayerSurfaceAttributes,
}

pub fn surface_plan(
    component: ShellComponent,
    output: OutputId,
    layout: ShellLayout,
) -> SurfacePlan {
    let attributes = match component {
        ShellComponent::Panel => panel(output, layout),
        ShellComponent::Launcher => top_left_popover("desktop-shell.launcher", output, layout),
        ShellComponent::Notifications => {
            top_right_popover("desktop-shell.notifications", output, layout, false)
        }
        ShellComponent::Osd => osd(output, layout),
        ShellComponent::ControlCenter => {
            top_right_popover("desktop-shell.control-center", output, layout, true)
        }
        ShellComponent::Overview => fullscreen("desktop-shell.overview", output, false),
        ShellComponent::LockScreen => fullscreen("desktop-shell.lock-screen", output, true),
    };
    SurfacePlan {
        component,
        attributes,
    }
}

fn panel(output: OutputId, layout: ShellLayout) -> LayerSurfaceAttributes {
    LayerSurfaceAttributes {
        namespace: "desktop-shell.panel".into(),
        output: Some(output),
        state: LayerSurfaceState {
            size: LogicalSize::new(0, layout.panel_height),
            anchor: LayerAnchor::TOP | LayerAnchor::LEFT | LayerAnchor::RIGHT,
            exclusive_zone: i32::try_from(layout.panel_height).unwrap_or(i32::MAX),
            exclusive_edge: Some(LayerEdge::Top),
            margins: LayerMargins::default(),
            keyboard_interactivity: LayerKeyboardInteractivity::None,
            layer: LayerSurfaceLayer::Top,
        },
    }
}

fn top_left_popover(
    namespace: &str,
    output: OutputId,
    layout: ShellLayout,
) -> LayerSurfaceAttributes {
    popover(
        namespace,
        output,
        layout,
        LayerAnchor::TOP | LayerAnchor::LEFT,
        true,
    )
}

fn top_right_popover(
    namespace: &str,
    output: OutputId,
    layout: ShellLayout,
    takes_focus: bool,
) -> LayerSurfaceAttributes {
    popover(
        namespace,
        output,
        layout,
        LayerAnchor::TOP | LayerAnchor::RIGHT,
        takes_focus,
    )
}

fn popover(
    namespace: &str,
    output: OutputId,
    layout: ShellLayout,
    anchor: LayerAnchor,
    takes_focus: bool,
) -> LayerSurfaceAttributes {
    LayerSurfaceAttributes {
        namespace: namespace.into(),
        output: Some(output),
        state: LayerSurfaceState {
            size: LogicalSize::new(layout.popover_width, layout.popover_height),
            anchor,
            exclusive_zone: -1,
            exclusive_edge: None,
            margins: LayerMargins::new(
                i32::try_from(layout.panel_height).unwrap_or(i32::MAX) + layout.edge_gap,
                layout.edge_gap,
                layout.edge_gap,
                layout.edge_gap,
            ),
            keyboard_interactivity: if takes_focus {
                LayerKeyboardInteractivity::OnDemand
            } else {
                LayerKeyboardInteractivity::None
            },
            layer: LayerSurfaceLayer::Overlay,
        },
    }
}

fn osd(output: OutputId, layout: ShellLayout) -> LayerSurfaceAttributes {
    LayerSurfaceAttributes {
        namespace: "desktop-shell.osd".into(),
        output: Some(output),
        state: LayerSurfaceState {
            size: LogicalSize::new(layout.osd_width, layout.osd_height),
            anchor: LayerAnchor::BOTTOM,
            exclusive_zone: -1,
            exclusive_edge: None,
            margins: LayerMargins::new(0, 0, layout.edge_gap * 4, 0),
            keyboard_interactivity: LayerKeyboardInteractivity::None,
            layer: LayerSurfaceLayer::Overlay,
        },
    }
}

fn fullscreen(namespace: &str, output: OutputId, lock: bool) -> LayerSurfaceAttributes {
    LayerSurfaceAttributes {
        namespace: namespace.into(),
        output: Some(output),
        state: LayerSurfaceState {
            size: LogicalSize::new(0, 0),
            anchor: LayerAnchor::TOP | LayerAnchor::BOTTOM | LayerAnchor::LEFT | LayerAnchor::RIGHT,
            exclusive_zone: -1,
            exclusive_edge: None,
            margins: LayerMargins::default(),
            keyboard_interactivity: if lock {
                LayerKeyboardInteractivity::Exclusive
            } else {
                LayerKeyboardInteractivity::OnDemand
            },
            layer: LayerSurfaceLayer::Overlay,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: OutputId = OutputId::from_raw(7);

    #[test]
    fn panel_reserves_exactly_its_height() {
        let plan = surface_plan(ShellComponent::Panel, OUTPUT, ShellLayout::default());
        assert_eq!(plan.attributes.state.size, LogicalSize::new(0, 40));
        assert_eq!(plan.attributes.state.exclusive_zone, 40);
        assert_eq!(plan.attributes.state.exclusive_edge, Some(LayerEdge::Top));
        assert!(plan.attributes.state.anchor.contains(LayerAnchor::LEFT));
        assert!(plan.attributes.state.anchor.contains(LayerAnchor::RIGHT));
    }

    #[test]
    fn lock_screen_covers_output_and_takes_exclusive_focus() {
        let plan = surface_plan(ShellComponent::LockScreen, OUTPUT, ShellLayout::default());
        assert_eq!(plan.attributes.state.size, LogicalSize::new(0, 0));
        assert_eq!(
            plan.attributes.state.keyboard_interactivity,
            LayerKeyboardInteractivity::Exclusive
        );
        assert_eq!(plan.attributes.state.layer, LayerSurfaceLayer::Overlay);
    }
}
