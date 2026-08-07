mod state;

pub use state::{
    PanelAppletAvailability, PanelAppletEmphasis, PanelAppletState, PanelAppletStore,
    PanelAppletUpdate,
};

use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{LogicalRect, LogicalSize};

use crate::{PanelConfig, ShellComponent};

const PANEL_INSET: u32 = 8;
const WIDGET_GAP: u32 = 6;
const WIDGET_VERTICAL_INSET: u32 = 6;

/// Stable identities for the retained widgets in one panel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PanelWidgetKind {
    Launcher,
    Workspaces,
    ActiveWindow,
    Media,
    SystemStatus,
    Clock,
    Notifications,
    ControlCenter,
}

impl PanelWidgetKind {
    pub const ALL: [Self; 8] = [
        Self::Launcher,
        Self::Workspaces,
        Self::ActiveWindow,
        Self::Media,
        Self::SystemStatus,
        Self::Clock,
        Self::Notifications,
        Self::ControlCenter,
    ];

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Launcher => 0,
            Self::Workspaces => 1,
            Self::ActiveWindow => 2,
            Self::Media => 3,
            Self::SystemStatus => 4,
            Self::Clock => 5,
            Self::Notifications => 6,
            Self::ControlCenter => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelWidget {
    pub kind: PanelWidgetKind,
    pub bounds: LogicalRect,
}

/// Retained logical geometry for a single output's panel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelScene {
    extent: LogicalSize,
    widgets: Vec<PanelWidget>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PanelInteraction {
    pub hovered: Option<PanelWidgetKind>,
    pub pressed: Option<PanelWidgetKind>,
    pub active: Option<PanelWidgetKind>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelDraw {
    pub rect: Rect2D,
    pub color: [f32; 4],
}

impl PanelScene {
    pub fn build(extent: LogicalSize, config: &PanelConfig) -> Self {
        let mut widgets = Vec::with_capacity(8);
        if extent.width == 0 || extent.height <= WIDGET_VERTICAL_INSET * 2 {
            return Self { extent, widgets };
        }

        let height = extent.height - WIDGET_VERTICAL_INSET * 2;
        let inset = PANEL_INSET.min(extent.width / 2);
        let mut left = inset;
        let mut right = extent.width.saturating_sub(inset);

        for &kind in config.left() {
            if extent.width < kind.minimum_panel_width() {
                continue;
            }
            push_left(&mut widgets, &mut left, right, kind, kind.width(), height);
        }
        for &kind in config.right().iter().rev() {
            if extent.width < kind.minimum_panel_width() {
                continue;
            }
            push_right(&mut widgets, left, &mut right, kind, kind.width(), height);
        }
        push_center_group(&mut widgets, left, right, extent, config.center(), height);
        widgets.sort_by_key(|widget| widget.bounds.origin.x);
        Self { extent, widgets }
    }

    pub fn extent(&self) -> LogicalSize {
        self.extent
    }

    pub fn widgets(&self) -> &[PanelWidget] {
        &self.widgets
    }

    pub fn hit_test(&self, position: (f64, f64)) -> Option<PanelWidgetKind> {
        self.widgets
            .iter()
            .find(|widget| contains(widget.bounds, position))
            .map(|widget| widget.kind)
    }

    pub(crate) fn physical_draws(
        &self,
        physical_extent: Extent2D,
        interaction: PanelInteraction,
        applets: &PanelAppletStore,
    ) -> Vec<PanelDraw> {
        self.widgets
            .iter()
            .filter_map(|widget| {
                physical_rect(widget.bounds, self.extent, physical_extent).map(|rect| PanelDraw {
                    rect,
                    color: widget_color(widget.kind, applets.state(widget.kind), interaction),
                })
            })
            .collect()
    }
}

impl PanelWidgetKind {
    pub const fn activation(self) -> Option<ShellComponent> {
        match self {
            Self::Launcher => None,
            Self::Workspaces => Some(ShellComponent::Overview),
            Self::Media | Self::SystemStatus | Self::ControlCenter => {
                Some(ShellComponent::ControlCenter)
            }
            Self::Notifications => Some(ShellComponent::NotificationCenter),
            Self::ActiveWindow | Self::Clock => None,
        }
    }

    pub const fn is_interactive(self) -> bool {
        matches!(self, Self::Launcher) || self.activation().is_some()
    }

    const fn width(self) -> u32 {
        match self {
            Self::Launcher | Self::Notifications | Self::ControlCenter => 32,
            Self::Clock => 96,
            Self::Workspaces | Self::SystemStatus => 112,
            Self::Media => 120,
            Self::ActiveWindow => 180,
        }
    }

    const fn minimum_panel_width(self) -> u32 {
        match self {
            Self::Launcher | Self::Notifications | Self::ControlCenter => 0,
            Self::Workspaces => 320,
            Self::SystemStatus => 520,
            Self::Clock => 560,
            Self::ActiveWindow => 800,
            Self::Media => 1_000,
        }
    }
}

impl std::str::FromStr for PanelWidgetKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "launcher" => Ok(Self::Launcher),
            "workspaces" => Ok(Self::Workspaces),
            "active-window" => Ok(Self::ActiveWindow),
            "media" => Ok(Self::Media),
            "system-status" => Ok(Self::SystemStatus),
            "clock" => Ok(Self::Clock),
            "notifications" => Ok(Self::Notifications),
            "control-center" => Ok(Self::ControlCenter),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for PanelWidgetKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Launcher => "launcher",
            Self::Workspaces => "workspaces",
            Self::ActiveWindow => "active-window",
            Self::Media => "media",
            Self::SystemStatus => "system-status",
            Self::Clock => "clock",
            Self::Notifications => "notifications",
            Self::ControlCenter => "control-center",
        };
        formatter.write_str(name)
    }
}

fn push_left(
    widgets: &mut Vec<PanelWidget>,
    cursor: &mut u32,
    right_limit: u32,
    kind: PanelWidgetKind,
    width: u32,
    height: u32,
) {
    let end = cursor.saturating_add(width);
    if end.saturating_add(WIDGET_GAP) > right_limit {
        return;
    }
    widgets.push(widget(kind, *cursor, width, height));
    *cursor = end.saturating_add(WIDGET_GAP);
}

fn push_right(
    widgets: &mut Vec<PanelWidget>,
    left_limit: u32,
    cursor: &mut u32,
    kind: PanelWidgetKind,
    width: u32,
    height: u32,
) {
    let Some(start) = cursor.checked_sub(width) else {
        return;
    };
    if start < left_limit.saturating_add(WIDGET_GAP) {
        return;
    }
    widgets.push(widget(kind, start, width, height));
    *cursor = start.saturating_sub(WIDGET_GAP);
}

fn push_center_group(
    widgets: &mut Vec<PanelWidget>,
    left_limit: u32,
    right_limit: u32,
    extent: LogicalSize,
    configured: &[PanelWidgetKind],
    height: u32,
) {
    let visible = configured
        .iter()
        .copied()
        .filter(|kind| extent.width >= kind.minimum_panel_width())
        .collect::<Vec<_>>();
    let width = visible
        .iter()
        .fold(0_u32, |total, kind| total.saturating_add(kind.width()));
    let gaps = WIDGET_GAP.saturating_mul(visible.len().saturating_sub(1) as u32);
    let total = width.saturating_add(gaps);
    let mut cursor = extent.width.saturating_sub(total) / 2;
    if cursor < left_limit || cursor.saturating_add(total) > right_limit {
        return;
    }
    for kind in visible {
        widgets.push(widget(kind, cursor, kind.width(), height));
        cursor = cursor
            .saturating_add(kind.width())
            .saturating_add(WIDGET_GAP);
    }
}

fn widget(kind: PanelWidgetKind, x: u32, width: u32, height: u32) -> PanelWidget {
    PanelWidget {
        kind,
        bounds: LogicalRect::new(
            i32::try_from(x).unwrap_or(i32::MAX),
            i32::try_from(WIDGET_VERTICAL_INSET).unwrap(),
            width,
            height,
        ),
    }
}

fn contains(bounds: LogicalRect, position: (f64, f64)) -> bool {
    if !position.0.is_finite() || !position.1.is_finite() {
        return false;
    }
    let left = f64::from(bounds.origin.x);
    let top = f64::from(bounds.origin.y);
    let right = left + f64::from(bounds.size.width);
    let bottom = top + f64::from(bounds.size.height);
    position.0 >= left && position.0 < right && position.1 >= top && position.1 < bottom
}

fn physical_rect(
    logical: LogicalRect,
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
) -> Option<Rect2D> {
    if logical_extent.is_empty() || physical_extent.is_empty() {
        return None;
    }
    let left = scale_edge(
        logical.origin.x.max(0) as u32,
        logical_extent.width,
        physical_extent.width,
    );
    let top = scale_edge(
        logical.origin.y.max(0) as u32,
        logical_extent.height,
        physical_extent.height,
    );
    let right = scale_edge(
        logical.origin.x.max(0) as u32 + logical.size.width,
        logical_extent.width,
        physical_extent.width,
    );
    let bottom = scale_edge(
        logical.origin.y.max(0) as u32 + logical.size.height,
        logical_extent.height,
        physical_extent.height,
    );
    (right > left && bottom > top).then(|| {
        Rect2D::new(
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
            right - left,
            bottom - top,
        )
    })
}

fn scale_edge(value: u32, logical: u32, physical: u32) -> u32 {
    let scaled = u64::from(value) * u64::from(physical) / u64::from(logical);
    u32::try_from(scaled).unwrap_or(u32::MAX).min(physical)
}

fn widget_color(
    kind: PanelWidgetKind,
    state: PanelAppletState,
    interaction: PanelInteraction,
) -> [f32; 4] {
    if interaction.pressed == Some(kind) {
        return [0.24, 0.43, 0.47, 0.98];
    }
    if state.availability() == PanelAppletAvailability::Failed
        || state.emphasis() == PanelAppletEmphasis::Critical
    {
        return [0.42, 0.09, 0.11, 0.98];
    }
    if interaction.active == Some(kind) {
        return [0.17, 0.34, 0.38, 0.98];
    }
    if interaction.hovered == Some(kind) && kind.is_interactive() {
        return [0.16, 0.22, 0.25, 0.98];
    }
    match state.availability() {
        PanelAppletAvailability::Pending => return [0.10, 0.105, 0.115, 0.82],
        PanelAppletAvailability::Unavailable => return [0.075, 0.078, 0.085, 0.62],
        PanelAppletAvailability::Failed | PanelAppletAvailability::Ready => {}
    }
    match state.emphasis() {
        PanelAppletEmphasis::Attention => return [0.35, 0.23, 0.06, 0.98],
        PanelAppletEmphasis::Active => return [0.08, 0.28, 0.18, 0.98],
        PanelAppletEmphasis::Critical | PanelAppletEmphasis::Normal => {}
    }
    match kind {
        PanelWidgetKind::Launcher => [0.12, 0.29, 0.32, 0.98],
        PanelWidgetKind::Workspaces => [0.13, 0.15, 0.18, 0.98],
        PanelWidgetKind::ActiveWindow => [0.105, 0.12, 0.145, 0.98],
        PanelWidgetKind::Media => [0.14, 0.12, 0.18, 0.98],
        PanelWidgetKind::SystemStatus => [0.11, 0.16, 0.17, 0.98],
        PanelWidgetKind::Clock => [0.18, 0.16, 0.11, 0.98],
        PanelWidgetKind::Notifications => [0.18, 0.12, 0.13, 0.98],
        PanelWidgetKind::ControlCenter => [0.12, 0.18, 0.14, 0.98],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_panel_contains_the_complete_widget_order() {
        let scene = PanelScene::build(LogicalSize::new(1_920, 40), &PanelConfig::default());
        let kinds = scene
            .widgets()
            .iter()
            .map(|widget| widget.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                PanelWidgetKind::Launcher,
                PanelWidgetKind::Workspaces,
                PanelWidgetKind::ActiveWindow,
                PanelWidgetKind::Clock,
                PanelWidgetKind::Media,
                PanelWidgetKind::SystemStatus,
                PanelWidgetKind::Notifications,
                PanelWidgetKind::ControlCenter,
            ]
        );
    }

    #[test]
    fn responsive_layout_stays_in_bounds_without_overlap() {
        for width in [96, 160, 320, 560, 800, 1_920] {
            let scene = PanelScene::build(LogicalSize::new(width, 40), &PanelConfig::default());
            for pair in scene.widgets().windows(2) {
                let left_end = pair[0].bounds.origin.x as u32 + pair[0].bounds.size.width;
                assert!(left_end <= pair[1].bounds.origin.x as u32);
            }
            assert!(scene.widgets().iter().all(|widget| {
                widget.bounds.origin.x >= 0
                    && widget.bounds.origin.x as u32 + widget.bounds.size.width <= width
            }));
        }
    }

    #[test]
    fn hit_testing_uses_half_open_widget_bounds() {
        let scene = PanelScene::build(LogicalSize::new(1_920, 40), &PanelConfig::default());
        let launcher = scene.widgets().first().unwrap();
        assert_eq!(
            scene.hit_test((f64::from(launcher.bounds.origin.x), 10.0)),
            Some(PanelWidgetKind::Launcher)
        );
        assert_eq!(
            scene.hit_test((
                f64::from(launcher.bounds.origin.x) + f64::from(launcher.bounds.size.width),
                10.0,
            )),
            None
        );
        assert_eq!(scene.hit_test((f64::NAN, 10.0)), None);
    }

    #[test]
    fn physical_draws_scale_retained_logical_geometry() {
        let scene = PanelScene::build(LogicalSize::new(960, 40), &PanelConfig::default());
        let draws = scene.physical_draws(
            Extent2D::new(1_920, 80),
            PanelInteraction::default(),
            &PanelAppletStore::default(),
        );
        assert_eq!(draws[0].rect, Rect2D::new(16, 12, 64, 56));
    }

    #[test]
    fn applet_failure_and_attention_are_visible_without_changing_geometry() {
        let scene = PanelScene::build(LogicalSize::new(1_920, 40), &PanelConfig::default());
        let mut applets = PanelAppletStore::default();
        let original = scene.physical_draws(
            Extent2D::new(1_920, 40),
            PanelInteraction::default(),
            &applets,
        );
        applets.apply(PanelAppletUpdate::new(
            PanelWidgetKind::Notifications,
            PanelAppletState::ready().with_emphasis(PanelAppletEmphasis::Attention),
        ));
        let attention = scene.physical_draws(
            Extent2D::new(1_920, 40),
            PanelInteraction::default(),
            &applets,
        );
        applets.apply(PanelAppletUpdate::new(
            PanelWidgetKind::SystemStatus,
            PanelAppletState::failed(),
        ));
        let failed = scene.physical_draws(
            Extent2D::new(1_920, 40),
            PanelInteraction::default(),
            &applets,
        );

        assert_eq!(
            original.iter().map(|draw| draw.rect).collect::<Vec<_>>(),
            attention.iter().map(|draw| draw.rect).collect::<Vec<_>>()
        );
        assert_ne!(original, attention);
        assert_ne!(attention, failed);
    }

    #[test]
    fn only_backed_widgets_publish_surface_activations() {
        assert_eq!(PanelWidgetKind::Launcher.activation(), None);
        assert!(PanelWidgetKind::Launcher.is_interactive());
        assert_eq!(
            PanelWidgetKind::Notifications.activation(),
            Some(ShellComponent::NotificationCenter)
        );
        assert_eq!(
            PanelWidgetKind::Workspaces.activation(),
            Some(ShellComponent::Overview)
        );
        assert_eq!(
            PanelWidgetKind::Media.activation(),
            Some(ShellComponent::ControlCenter)
        );
        assert!(PanelWidgetKind::Media.is_interactive());
        assert_eq!(PanelWidgetKind::Clock.activation(), None);
    }
}
