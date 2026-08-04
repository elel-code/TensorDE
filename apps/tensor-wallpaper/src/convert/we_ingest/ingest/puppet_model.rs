//! Typed MDLV mesh and clipping-table lowering.

use super::WeIrBuilder;
use crate::convert::we_ingest::ir::{
    WeIrMesh, WeIrMeshClippingSlice, WeIrMeshClippingSliceRole, WeIrMeshClippingSubdraw,
    WeIrMeshSourceRecord, WeIrMeshVertex, WeIrUnsupported,
};
use crate::convert::we_ingest::mdl::{MdlClippingSubdraw, MdlMeshEntry, mdl_entry_vertex_bounds};

/// WE emits clipping subdraws ordered by min(target_source_ordinal) ascending
/// (stable on original file index for ties). Store and emit in that order so
/// mesh_clipping_subdraws, pass_index, and packed index slices match the WE
/// D3D11 subdraw emission order under alpha blend.
fn clipping_subdraw_emit_order(subdraws: &[MdlClippingSubdraw]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..subdraws.len()).collect();
    order.sort_by_key(|&index| {
        (
            subdraws[index]
                .target_source_ordinals
                .iter()
                .copied()
                .min()
                .unwrap_or(u32::MAX),
            index,
        )
    });
    order
}

impl WeIrBuilder {
    pub(super) fn add_mdl_meshes(
        &mut self,
        object: u32,
        image_path: &str,
        entries: &[MdlMeshEntry],
        material_handles: &[Option<u32>],
        clipping_mask_resources: &[Vec<Option<u32>>],
    ) -> (u32, u32) {
        let mesh_start = self.meshes.len() as u32;
        if entries.is_empty() {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("mdl-has-no-mesh-entries:{image_path}"),
                expected_subsystem: "convert/we_ingest MDLV0023 mesh blocks".to_owned(),
                containment: "object-kept-without-mdl-mesh".to_owned(),
            });
            return (mesh_start, 0);
        }

        for (entry_index, entry) in entries.iter().enumerate() {
            if entry.vertices.is_empty() || entry.indices.is_empty() {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: Some(entry_index as u32),
                    feature: format!("mdl-empty-mesh-entry:{image_path}:{entry_index}"),
                    expected_subsystem: "convert/we_ingest MDLV0023 mesh blocks".to_owned(),
                    containment: "empty-entry-skipped".to_owned(),
                });
                continue;
            }
            let invalid_index = entry
                .indices
                .iter()
                .copied()
                .find(|index| *index >= entry.vertices.len() as u32);
            if let Some(index) = invalid_index {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: Some(entry_index as u32),
                    feature: format!(
                        "mdl-mesh-index-out-of-range:{image_path}:{entry_index}:{index}"
                    ),
                    expected_subsystem: "convert/we_ingest MDLV0023 index block".to_owned(),
                    containment: "invalid-entry-skipped".to_owned(),
                });
                continue;
            }

            let (bounds_min, bounds_max) = mdl_entry_vertex_bounds(entry);
            let vertex_start = self.mesh_vertices.len() as u32;
            let index_start = self.mesh_indices.len() as u32;
            self.mesh_vertices
                .extend(entry.vertices.iter().map(|vertex| WeIrMeshVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    blend_indices: vertex.blend_indices,
                    blend_weights: vertex.blend_weights,
                }));
            self.mesh_indices.extend(entry.indices.iter().copied());
            let material = material_handles
                .get(entry_index)
                .copied()
                .flatten()
                .or_else(|| material_handles.first().copied().flatten());
            let mesh = self.meshes.len() as u32;
            self.meshes.push(WeIrMesh {
                object,
                material,
                vertex_start,
                vertex_count: entry.vertices.len() as u32,
                index_start,
                index_count: entry.indices.len() as u32,
                width: bounds_max.x - bounds_min.x,
                height: bounds_max.y - bounds_min.y,
                bounds_min,
                bounds_max,
            });
            self.mesh_source_records
                .extend(
                    entry
                        .source_records
                        .iter()
                        .map(|record| WeIrMeshSourceRecord {
                            mesh,
                            source_index: record.source_index,
                            local_index_offset: record.local_index_offset,
                            index_start: record.index_start,
                            index_count: record.index_count,
                        }),
                );
            for &file_index in clipping_subdraw_emit_order(&entry.clipping_subdraws).iter() {
                let subdraw = &entry.clipping_subdraws[file_index];
                let target_source_start = self.mesh_clipping_source_ordinals.len() as u32;
                self.mesh_clipping_source_ordinals
                    .extend(subdraw.target_source_ordinals.iter().copied());
                let target_source_count = subdraw.target_source_ordinals.len() as u32;
                let mask_source_start = self.mesh_clipping_source_ordinals.len() as u32;
                self.mesh_clipping_source_ordinals
                    .extend(subdraw.mask_source_ordinals.iter().copied());
                let mask_source_count = subdraw.mask_source_ordinals.len() as u32;
                self.mesh_clipping_subdraws.push(WeIrMeshClippingSubdraw {
                    mesh,
                    source_qword: subdraw.source_qword,
                    mask: subdraw.mask_resource.clone(),
                    mask_resource: clipping_mask_resources
                        .get(entry_index)
                        .and_then(|resources| resources.get(file_index))
                        .copied()
                        .flatten(),
                    raw_flags: subdraw.raw_flags,
                    target_source_start,
                    target_source_count,
                    mask_source_start,
                    mask_source_count,
                });
            }
            self.materialize_mdl_clipping_slices(mesh, entry);
        }
        (mesh_start, self.meshes.len() as u32 - mesh_start)
    }

    fn materialize_mdl_clipping_slices(&mut self, mesh: u32, entry: &MdlMeshEntry) {
        if entry.source_records.is_empty() || entry.clipping_subdraws.is_empty() {
            return;
        }
        let targets = entry
            .clipping_subdraws
            .iter()
            .flat_map(|subdraw| subdraw.target_source_ordinals.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        let emit_order = clipping_subdraw_emit_order(&entry.clipping_subdraws);
        let source_count = entry.source_records.len() as u32;
        // WE treats each record's mask ordinals as the inclusive boundaries of one contiguous
        // source-record span. The current clipped target is omitted from that mask draw, while
        // earlier clipped targets remain part of later masks. Visible draws are split at each
        // mask-span end and omit every clipped target, but intentionally include mask sources.
        let mask_spans = emit_order
            .iter()
            .map(|file_index| {
                let masks = &entry.clipping_subdraws[*file_index].mask_source_ordinals;
                let start = masks
                    .iter()
                    .copied()
                    .min()
                    .unwrap_or(source_count)
                    .min(source_count);
                let end = masks
                    .iter()
                    .copied()
                    .max()
                    .map(|ordinal| ordinal.saturating_add(1))
                    .unwrap_or(start)
                    .min(source_count);
                (start, end)
            })
            .collect::<Vec<_>>();
        let prefix_end = mask_spans
            .first()
            .map(|(_, end)| *end)
            .unwrap_or(source_count);
        let visible_prefix = (0..prefix_end)
            .filter(|ordinal| !targets.contains(ordinal))
            .collect::<Vec<_>>();
        self.push_mdl_clipping_slice(
            mesh,
            u32::MAX,
            WeIrMeshClippingSliceRole::VisiblePrefix,
            entry,
            &visible_prefix,
        );
        for (emit_index, &file_index) in emit_order.iter().enumerate() {
            let record = &entry.clipping_subdraws[file_index];
            let (mask_start, mask_end) = mask_spans[emit_index];
            let mask_sources = (mask_start..mask_end)
                .filter(|ordinal| !record.target_source_ordinals.contains(ordinal))
                .collect::<Vec<_>>();
            self.push_mdl_clipping_slice(
                mesh,
                emit_index as u32,
                WeIrMeshClippingSliceRole::MaskProducer,
                entry,
                &mask_sources,
            );
            self.push_mdl_clipping_slice(
                mesh,
                emit_index as u32,
                WeIrMeshClippingSliceRole::ClippedTarget,
                entry,
                &record.target_source_ordinals,
            );
            let segment_start = mask_end;
            let segment_end = mask_spans
                .get(emit_index + 1)
                .map(|(_, end)| *end)
                .unwrap_or(source_count);
            let visible_remainder = (segment_start..segment_end)
                .filter(|ordinal| !targets.contains(ordinal))
                .collect::<Vec<_>>();
            self.push_mdl_clipping_slice(
                mesh,
                emit_index as u32,
                WeIrMeshClippingSliceRole::VisibleRemainder,
                entry,
                &visible_remainder,
            );
        }
    }

    fn push_mdl_clipping_slice(
        &mut self,
        mesh: u32,
        subdraw: u32,
        role: WeIrMeshClippingSliceRole,
        entry: &MdlMeshEntry,
        ordinals: &[u32],
    ) {
        let index_start = self.mesh_indices.len() as u32;
        for ordinal in ordinals {
            let Some(source) = entry.source_records.get(*ordinal as usize) else {
                continue;
            };
            let start = source.index_start as usize;
            let end = start
                .saturating_add(source.index_count as usize)
                .min(entry.indices.len());
            self.mesh_indices
                .extend_from_slice(&entry.indices[start.min(entry.indices.len())..end]);
        }
        let index_count = self.mesh_indices.len() as u32 - index_start;
        if index_count != 0 {
            self.mesh_clipping_slices.push(WeIrMeshClippingSlice {
                mesh,
                subdraw,
                role,
                index_start,
                index_count,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clipping_subdraw_emit_order;
    use crate::convert::we_ingest::mdl::MdlClippingSubdraw;

    fn subdraw(targets: &[u32]) -> MdlClippingSubdraw {
        MdlClippingSubdraw {
            source_qword: 0,
            mask_resource: String::new(),
            raw_flags: 0,
            target_source_ordinals: targets.to_vec(),
            mask_source_ordinals: vec![0],
        }
    }

    #[test]
    fn clipping_subdraw_emit_order_matches_we_min_target_ordinal_sort() {
        // File order mirrors the eye mesh packing that previously produced
        // Tensor Wallpaper [2622,1875,2073,1335]; WE order is min-ordinal sort.
        let subdraws = vec![
            subdraw(&[21, 24, 26, 42, 43]), // file 0 -> ic family 2622
            subdraw(&[12, 15, 18]),         // file 1 -> 1875
            subdraw(&[10]),                 // file 2 -> 2073
            subdraw(&[11]),                 // file 3 -> 1335
        ];
        assert_eq!(
            clipping_subdraw_emit_order(&subdraws),
            vec![2, 3, 1, 0],
            "WE clipped-target order is min(target_source_ordinal) ascending"
        );
    }

    #[test]
    fn clipping_subdraw_emit_order_is_stable_on_equal_min_ordinal() {
        // Both mins are 5; original file indices break the tie.
        let subdraws = vec![subdraw(&[5, 9]), subdraw(&[5, 8]), subdraw(&[7])];
        assert_eq!(clipping_subdraw_emit_order(&subdraws), vec![0, 1, 2]);
    }
}
