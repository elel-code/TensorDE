//! Pure presentation-mode policy for one rendered output frame.

use tensor_host::PresentMode;
use tensor_protocol::SurfacePresentationHint;

use crate::{layout::Rect, scene::SceneSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationDecisionReason {
    ClientRequiresVsync,
    SceneNotExclusive,
    CursorOverlay,
    CapturePending,
    DependentMultiPass,
    KmsUnavailable,
    AsyncEligible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PresentationDecision {
    pub(super) mode: PresentMode,
    pub(super) reason: PresentationDecisionReason,
}

pub(super) fn decide_presentation(
    scene: &SceneSnapshot,
    viewport: Rect,
    has_cursor_overlay: bool,
    capture_pending: bool,
    direct_single_pass: bool,
    kms_allows_async: bool,
) -> PresentationDecision {
    let Some(node) = scene.nodes().first().filter(|_| scene.nodes().len() == 1) else {
        return vsync(PresentationDecisionReason::SceneNotExclusive);
    };
    if node.presentation_hint != SurfacePresentationHint::Async {
        return vsync(PresentationDecisionReason::ClientRequiresVsync);
    }
    if node.placement.geometry != viewport
        || node.placement.visible != Some(viewport)
        || node.effects.opacity != crate::scene::UnitFraction::OPAQUE
        || node.effects.corner_radius != 0
        || node.effects.shadow.is_some()
        || node.effects.backdrop_blur.is_some()
        || node.focus_outline.is_some()
    {
        return vsync(PresentationDecisionReason::SceneNotExclusive);
    }
    if has_cursor_overlay {
        return vsync(PresentationDecisionReason::CursorOverlay);
    }
    if capture_pending {
        return vsync(PresentationDecisionReason::CapturePending);
    }
    if !direct_single_pass {
        return vsync(PresentationDecisionReason::DependentMultiPass);
    }
    if !kms_allows_async {
        return vsync(PresentationDecisionReason::KmsUnavailable);
    }
    PresentationDecision {
        mode: PresentMode::Async,
        reason: PresentationDecisionReason::AsyncEligible,
    }
}

const fn vsync(reason: PresentationDecisionReason) -> PresentationDecision {
    PresentationDecision {
        mode: PresentMode::Vsync,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ecs::{ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{EffectStyle, SceneNode},
    };

    use super::*;

    const VIEWPORT: Rect = Rect::new(0, 0, 1920, 1080);

    fn scene(hint: SurfacePresentationHint) -> SceneSnapshot {
        let mut node = SceneNode::new(
            ViewId::new(1),
            1,
            LayoutPlacement::new(VIEWPORT, VIEWPORT),
            EffectStyle::default(),
        );
        node.presentation_hint = hint;
        SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![node])
    }

    #[test]
    fn async_requires_the_complete_exclusive_hardware_path() {
        let decision = decide_presentation(
            &scene(SurfacePresentationHint::Async),
            VIEWPORT,
            false,
            false,
            true,
            true,
        );
        assert_eq!(decision.mode, PresentMode::Async);
        assert_eq!(decision.reason, PresentationDecisionReason::AsyncEligible);
    }

    #[test]
    fn every_safety_gate_fails_closed_to_vsync() {
        let async_scene = scene(SurfacePresentationHint::Async);
        assert_eq!(
            decide_presentation(&async_scene, VIEWPORT, true, false, true, true).reason,
            PresentationDecisionReason::CursorOverlay
        );
        assert_eq!(
            decide_presentation(&async_scene, VIEWPORT, false, true, true, true).reason,
            PresentationDecisionReason::CapturePending
        );
        assert_eq!(
            decide_presentation(&async_scene, VIEWPORT, false, false, false, true).reason,
            PresentationDecisionReason::DependentMultiPass
        );
        assert_eq!(
            decide_presentation(&async_scene, VIEWPORT, false, false, true, false).reason,
            PresentationDecisionReason::KmsUnavailable
        );
        assert_eq!(
            decide_presentation(
                &scene(SurfacePresentationHint::Vsync),
                VIEWPORT,
                false,
                false,
                true,
                true,
            )
            .reason,
            PresentationDecisionReason::ClientRequiresVsync
        );
    }
}
