//! Semantic ECS component binding objects to authored transform tracks.

use crate::engine::scene::abi::SceneObjectHandle;
use crate::engine::scene::storage::SceneStorage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformAnimationComponent {
    track_indices: Vec<u32>,
}

impl TransformAnimationComponent {
    pub(super) fn from_storage(storage: &SceneStorage, object: SceneObjectHandle) -> Option<Self> {
        let track_indices = storage
            .object_transform_tracks()
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                (track.object == object)
                    .then(|| u32::try_from(index).ok())
                    .flatten()
            })
            .collect::<Vec<_>>();
        (!track_indices.is_empty()).then_some(Self { track_indices })
    }

    pub fn track_indices(&self) -> &[u32] {
        &self.track_indices
    }
}
