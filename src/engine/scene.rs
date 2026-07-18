//! Backend-independent scene engine boundary.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/project-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.*`
//! - `references/godot/servers/rendering/storage/*`

pub mod abi;
pub mod binary;
pub mod event;
pub mod rendering_device_graph;
pub mod script;
pub mod semantic_world;
pub mod server;
pub mod storage;

pub use abi::*;
pub use binary::{
    SceneBinaryDocument, SceneBinaryError, read_scene_binary, read_scene_binary_bytes,
    write_scene_binary,
};
pub use event::{
    SceneAudioSource, SceneAudioState, SceneEvent, SceneEventQueue, SceneEventSequence,
    SceneFrameEvents, SceneLocalTime, SceneMediaClockState, SceneMediaGeneration,
    SceneMediaPlaybackState, SceneMediaSessionId, ScenePointerEvent, ScenePointerEventKind,
    ScenePointerSource, ScenePointerState, SceneSequencedEvent, SceneVideoState,
};
pub use rendering_device_graph::{
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceEffectBatch,
    SceneRenderingDeviceEffectBatchFamily, SceneRenderingDeviceEffectBatchInstance,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMaterialSampledBinding,
    SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
    SceneRenderingDevicePuppetBoneMatrix, SceneRenderingDevicePuppetBonePalette,
    SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation,
};
pub use script::{
    SceneScriptDelta, SceneScriptError, SceneScriptFrameInput, SceneScriptProgram,
    SceneScriptRuntime,
};
pub use semantic_world::{
    MaterialBindingComponent, MeshBindingComponent, ObjectEffectBindingComponent, ParentComponent,
    PuppetBindingComponent, ResolvedAttachmentLink, ResolvedObjectEffectState, ResolvedObjectState,
    ResolvedPuppetBoneMatrix, ResolvedPuppetBonePalette, ResolvedSemanticFrame, SceneSemanticWorld,
    SceneSemanticWorldError, SemanticEntity, SemanticEntityRecord, SemanticMeshBinding,
    SemanticObjectEffectBinding, SemanticRenderPlanInputs, TransformAnimationComponent,
    TransformComponent, VisibilityComponent,
};
pub use server::{
    RendererSceneRenderPlan, RenderingServer, SceneEngineRenderPlan, SceneObjectRenderGraph,
};
pub use storage::{SceneStorage, SceneStorageError};

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenePoseObjectKind {
    Solid,
    Image,
    Text,
    Path,
    Puppet,
    ParticleEmitter,
    Video,
    Clear,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneLayerPose {
    pub layer_index: usize,
    pub position_transform_x: [f32; 4],
    pub position_transform_y: [f32; 4],
    pub layer_opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLayerPoseTimeline {
    pub frame_rate: u32,
    pub frame_count: u32,
    pub layer_indices: Vec<usize>,
    pub poses: Vec<SceneLayerPose>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneLayerPoseTimelineError {
    EmptyFrameRate,
    FrameCountOverflow,
    PoseCountOverflow,
    MissingLayerPose { layer_index: usize },
}

impl fmt::Display for SceneLayerPoseTimelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFrameRate => f.write_str("scene layer pose timeline frame rate is zero"),
            Self::FrameCountOverflow => {
                f.write_str("scene layer pose timeline frame count exceeds u32")
            }
            Self::PoseCountOverflow => {
                f.write_str("scene layer pose timeline pose count overflows")
            }
            Self::MissingLayerPose { layer_index } => {
                write!(
                    f,
                    "scene layer {layer_index} has no pose in retained timeline"
                )
            }
        }
    }
}

impl std::error::Error for SceneLayerPoseTimelineError {}

pub fn retained_layer_pose_timeline_from_frames(
    frame_rate: u32,
    frames: Vec<Vec<SceneLayerPose>>,
) -> Result<(Vec<SceneLayerPose>, Option<SceneLayerPoseTimeline>), SceneLayerPoseTimelineError> {
    if frame_rate == 0 {
        return Err(SceneLayerPoseTimelineError::EmptyFrameRate);
    }
    let Some(first_frame) = frames.first() else {
        return Ok((Vec::new(), None));
    };
    let base_poses = first_frame.clone();
    let mut layer_indices = frames
        .iter()
        .flat_map(|frame| frame.iter().map(|pose| pose.layer_index))
        .collect::<Vec<_>>();
    layer_indices.sort_unstable();
    layer_indices.dedup();
    if layer_indices.is_empty() {
        return Ok((base_poses, None));
    }
    let frame_count =
        u32::try_from(frames.len()).map_err(|_| SceneLayerPoseTimelineError::FrameCountOverflow)?;
    let capacity = layer_indices
        .len()
        .checked_mul(frames.len())
        .ok_or(SceneLayerPoseTimelineError::PoseCountOverflow)?;
    let mut first_pose_by_layer = vec![None; layer_indices.len()];
    for frame in &frames {
        for &pose in frame {
            let Ok(layer_slot) = layer_indices.binary_search(&pose.layer_index) else {
                continue;
            };
            if first_pose_by_layer[layer_slot].is_none() {
                first_pose_by_layer[layer_slot] = Some(pose);
            }
        }
    }
    let mut current_pose_by_layer = Vec::with_capacity(layer_indices.len());
    for (layer_slot, first_pose) in first_pose_by_layer.into_iter().enumerate() {
        current_pose_by_layer.push(first_pose.ok_or(
            SceneLayerPoseTimelineError::MissingLayerPose {
                layer_index: layer_indices[layer_slot],
            },
        )?);
    }
    let mut poses = vec![current_pose_by_layer[0]; capacity];
    for (frame_index, frame) in frames.iter().enumerate() {
        for &pose in frame {
            let Ok(layer_slot) = layer_indices.binary_search(&pose.layer_index) else {
                continue;
            };
            current_pose_by_layer[layer_slot] = pose;
        }
        for (layer_slot, &pose) in current_pose_by_layer.iter().enumerate() {
            poses[layer_slot * frames.len() + frame_index] = pose;
        }
    }
    Ok((
        base_poses,
        Some(SceneLayerPoseTimeline {
            frame_rate,
            frame_count,
            layer_indices,
            poses,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(layer_index: usize, x: f32) -> SceneLayerPose {
        SceneLayerPose {
            layer_index,
            position_transform_x: [1.0, 0.0, x, 0.0],
            position_transform_y: [0.0, 1.0, 0.0, 0.0],
            layer_opacity: 1.0,
        }
    }

    #[test]
    fn retained_pose_timeline_is_layer_major_and_carries_missing_frames() {
        let (base, timeline) = retained_layer_pose_timeline_from_frames(
            60,
            vec![vec![pose(7, 0.0)], vec![pose(3, 1.0), pose(7, 2.0)]],
        )
        .expect("timeline");
        let timeline = timeline.expect("retained timeline");
        assert_eq!(base, vec![pose(7, 0.0)]);
        assert_eq!(timeline.layer_indices, vec![3, 7]);
        assert_eq!(timeline.frame_count, 2);
        assert_eq!(
            timeline.poses,
            vec![pose(3, 1.0), pose(3, 1.0), pose(7, 0.0), pose(7, 2.0)]
        );
    }
}
