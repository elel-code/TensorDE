//! Optional GPU timestamp measurement for the Vulkan scene frame.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `references/gilder/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::effect_target::SceneEffectTargetTimingCommand;

const FRAME_TIMESTAMP_QUERY_COUNT: u32 = 2;
const EFFECT_BATCH_TIMESTAMP_QUERY_COUNT: u32 = 2;
const GRAPH_TIMESTAMP_QUERY_COUNT: u32 = 6;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneGraphGpuTimingSnapshot {
    pub graph_index: u32,
    pub sample_count: u64,
    pub total_micros: f64,
    pub average_micros: Option<f64>,
    pub min_micros: Option<f64>,
    pub max_micros: Option<f64>,
    pub effect_target_average_micros: Option<f64>,
    pub scene_color_average_micros: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneEffectCommandGpuTimingSnapshot {
    pub graph_index: u32,
    pub graph_command_index: u32,
    pub command_kind: &'static str,
    pub sample_count: u64,
    pub average_micros: Option<f64>,
    pub min_micros: Option<f64>,
    pub max_micros: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneGpuTimingSnapshot {
    pub measurement_scope: &'static str,
    pub timestamp_period_nanoseconds: f32,
    pub timestamp_valid_bits: u32,
    pub frame_sample_count: u64,
    pub frame_total_micros: f64,
    pub frame_average_micros: Option<f64>,
    pub frame_min_micros: Option<f64>,
    pub frame_max_micros: Option<f64>,
    pub effect_batch_measurement_scope: &'static str,
    pub effect_batch_sample_count: u64,
    pub effect_batch_total_micros: f64,
    pub effect_batch_average_micros: Option<f64>,
    pub effect_batch_min_micros: Option<f64>,
    pub effect_batch_max_micros: Option<f64>,
    pub graph_measurement_scope: &'static str,
    pub graphs: Vec<NativeVulkanSceneGraphGpuTimingSnapshot>,
    pub effect_command_measurement_scope: &'static str,
    pub effect_commands: Vec<NativeVulkanSceneEffectCommandGpuTimingSnapshot>,
}

pub(super) struct SceneGpuTiming {
    query_pool: vk::QueryPool,
    query_count: u32,
    timestamp_period_nanoseconds: f32,
    timestamp_valid_bits: u32,
    pending: bool,
    frame: GpuDurationStats,
    effect_batch: GpuDurationStats,
    graph_indices: Vec<u32>,
    graphs: Vec<GpuDurationStats>,
    graph_effect_targets: Vec<GpuDurationStats>,
    graph_scene_colors: Vec<GpuDurationStats>,
    effect_commands: Vec<SceneEffectTargetTimingCommand>,
    effect_command_stats: Vec<GpuDurationStats>,
}

#[derive(Debug, Clone, Copy, Default)]
struct GpuDurationStats {
    sample_count: u64,
    total_micros: f64,
    min_micros: Option<f64>,
    max_micros: Option<f64>,
}

impl GpuDurationStats {
    fn observe(&mut self, micros: f64) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.total_micros += micros;
        self.min_micros = Some(self.min_micros.map_or(micros, |value| value.min(micros)));
        self.max_micros = Some(self.max_micros.map_or(micros, |value| value.max(micros)));
    }

    fn average_micros(self) -> Option<f64> {
        (self.sample_count != 0).then_some(self.total_micros / self.sample_count as f64)
    }
}

impl SceneGpuTiming {
    pub(super) fn create(
        device: &Device,
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        queue_family_index: u32,
        enabled: bool,
        graph_indices: &[u32],
        effect_commands: &[SceneEffectTargetTimingCommand],
    ) -> Result<Option<Self>, String> {
        if !enabled {
            return Ok(None);
        }
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let timestamp_valid_bits = queue_families
            .get(queue_family_index as usize)
            .ok_or_else(|| {
                format!(
                    "selected Vulkan queue family {queue_family_index} is missing for GPU timing"
                )
            })?
            .timestamp_valid_bits;
        if timestamp_valid_bits == 0 {
            return Err(format!(
                "selected Vulkan queue family {queue_family_index} does not support timestamps"
            ));
        }
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let timestamp_period_nanoseconds = properties.limits.timestamp_period;
        if !timestamp_period_nanoseconds.is_finite() || timestamp_period_nanoseconds <= 0.0 {
            return Err(format!(
                "selected Vulkan device reported invalid timestampPeriod {timestamp_period_nanoseconds}"
            ));
        }
        let graph_count = u32::try_from(graph_indices.len())
            .map_err(|_| "scene GPU timing graph count exceeds u32".to_owned())?;
        let query_count = graph_count
            .checked_mul(GRAPH_TIMESTAMP_QUERY_COUNT)
            .and_then(|count| count.checked_add(FRAME_TIMESTAMP_QUERY_COUNT))
            .and_then(|count| count.checked_add(EFFECT_BATCH_TIMESTAMP_QUERY_COUNT))
            .and_then(|count| {
                u32::try_from(effect_commands.len())
                    .ok()?
                    .checked_mul(2)
                    .and_then(|effect_count| count.checked_add(effect_count))
            })
            .ok_or_else(|| "scene GPU timing query count exceeds u32".to_owned())?;
        let create_info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(query_count)
            .build();
        let query_pool = unsafe { device.create_query_pool(&create_info, None) }
            .map_err(|err| format!("vkCreateQueryPool(scene GPU timing): {err:?}"))?;
        Ok(Some(Self {
            query_pool,
            query_count,
            timestamp_period_nanoseconds,
            timestamp_valid_bits,
            pending: false,
            frame: GpuDurationStats::default(),
            effect_batch: GpuDurationStats::default(),
            graph_indices: graph_indices.to_vec(),
            graphs: vec![GpuDurationStats::default(); graph_indices.len()],
            graph_effect_targets: vec![GpuDurationStats::default(); graph_indices.len()],
            graph_scene_colors: vec![GpuDurationStats::default(); graph_indices.len()],
            effect_commands: effect_commands.to_vec(),
            effect_command_stats: vec![GpuDurationStats::default(); effect_commands.len()],
        }))
    }

    pub(super) fn collect_completed(&mut self, device: &Device) -> Result<(), String> {
        if !self.pending {
            return Ok(());
        }
        let mut bytes = vec![0u8; self.query_count as usize * size_of::<u64>()];
        unsafe {
            device
                .get_query_pool_results(
                    self.query_pool,
                    0,
                    self.query_count,
                    &mut bytes,
                    size_of::<u64>() as u64,
                    vk::QueryResultFlags::_64,
                )
                .map_err(|err| format!("vkGetQueryPoolResults(scene GPU timing): {err:?}"))?;
        }
        self.frame.observe(query_duration_micros(
            &bytes,
            0,
            1,
            self.timestamp_valid_bits,
            self.timestamp_period_nanoseconds,
        ));
        self.effect_batch.observe(query_duration_micros(
            &bytes,
            effect_batch_start_query(),
            effect_batch_start_query() + 1,
            self.timestamp_valid_bits,
            self.timestamp_period_nanoseconds,
        ));
        for (graph_position, graph) in self.graphs.iter_mut().enumerate() {
            let start_query = graph_start_query(graph_position);
            graph.observe(query_duration_micros(
                &bytes,
                start_query,
                start_query + 1,
                self.timestamp_valid_bits,
                self.timestamp_period_nanoseconds,
            ));
            self.graph_effect_targets[graph_position].observe(query_duration_micros(
                &bytes,
                start_query + 2,
                start_query + 3,
                self.timestamp_valid_bits,
                self.timestamp_period_nanoseconds,
            ));
            self.graph_scene_colors[graph_position].observe(query_duration_micros(
                &bytes,
                start_query + 4,
                start_query + 5,
                self.timestamp_valid_bits,
                self.timestamp_period_nanoseconds,
            ));
        }
        for (command_position, stats) in self.effect_command_stats.iter_mut().enumerate() {
            let start_query =
                effect_command_start_query(self.graph_indices.len(), command_position);
            stats.observe(query_duration_micros(
                &bytes,
                start_query,
                start_query + 1,
                self.timestamp_valid_bits,
                self.timestamp_period_nanoseconds,
            ));
        }
        self.pending = false;
        Ok(())
    }

    pub(super) fn record_start(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            device.cmd_reset_query_pool(command_buffer, self.query_pool, 0, self.query_count);
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.query_pool,
                0,
            );
        }
    }

    pub(super) fn record_finish(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.query_pool,
                1,
            );
        }
    }

    pub(super) fn record_graph_start(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.query_pool,
                graph_start_query(graph_position),
            );
        }
    }

    pub(super) fn record_effect_batch_start(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
    ) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.query_pool,
                effect_batch_start_query(),
            );
        }
    }

    pub(super) fn record_effect_batch_finish(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
    ) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.query_pool,
                effect_batch_start_query() + 1,
            );
        }
    }

    pub(super) fn record_graph_finish(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.query_pool,
                graph_start_query(graph_position) + 1,
            );
        }
    }

    pub(super) fn record_graph_effect_target_start(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        self.record_graph_phase_timestamp(device, command_buffer, graph_position, 2);
    }

    pub(super) fn record_graph_effect_target_finish(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        self.record_graph_phase_timestamp(device, command_buffer, graph_position, 3);
    }

    pub(super) fn record_graph_scene_color_start(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        self.record_graph_phase_timestamp(device, command_buffer, graph_position, 4);
    }

    pub(super) fn record_graph_scene_color_finish(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
    ) {
        self.record_graph_phase_timestamp(device, command_buffer, graph_position, 5);
    }

    fn record_graph_phase_timestamp(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        graph_position: usize,
        offset: u32,
    ) {
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.query_pool,
                graph_start_query(graph_position) + offset,
            );
        }
    }

    pub(super) fn record_effect_command(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        source_position: usize,
        starting: bool,
    ) {
        let Some(command_position) = self
            .effect_commands
            .iter()
            .position(|command| command.source_position == source_position)
        else {
            return;
        };
        let query = effect_command_start_query(self.graph_indices.len(), command_position)
            + u32::from(!starting);
        unsafe {
            device.cmd_write_timestamp2(
                command_buffer,
                if starting {
                    vk::PipelineStageFlags2::TOP_OF_PIPE
                } else {
                    vk::PipelineStageFlags2::BOTTOM_OF_PIPE
                },
                self.query_pool,
                query,
            );
        }
    }

    pub(super) fn mark_submitted(&mut self) {
        self.pending = true;
    }

    pub(super) fn snapshot(&self) -> NativeVulkanSceneGpuTimingSnapshot {
        NativeVulkanSceneGpuTimingSnapshot {
            measurement_scope: "top-of-pipe-to-bottom-of-pipe",
            timestamp_period_nanoseconds: self.timestamp_period_nanoseconds,
            timestamp_valid_bits: self.timestamp_valid_bits,
            frame_sample_count: self.frame.sample_count,
            frame_total_micros: self.frame.total_micros,
            frame_average_micros: self.frame.average_micros(),
            frame_min_micros: self.frame.min_micros,
            frame_max_micros: self.frame.max_micros,
            effect_batch_measurement_scope: "scene-level-effect-family-batch",
            effect_batch_sample_count: self.effect_batch.sample_count,
            effect_batch_total_micros: self.effect_batch.total_micros,
            effect_batch_average_micros: self.effect_batch.average_micros(),
            effect_batch_min_micros: self.effect_batch.min_micros,
            effect_batch_max_micros: self.effect_batch.max_micros,
            graph_measurement_scope: "graph-loop-iteration",
            graphs: self
                .graph_indices
                .iter()
                .copied()
                .zip(self.graphs.iter().copied())
                .zip(self.graph_effect_targets.iter().copied())
                .zip(self.graph_scene_colors.iter().copied())
                .map(|(((graph_index, stats), effect_target), scene_color)| {
                    NativeVulkanSceneGraphGpuTimingSnapshot {
                        graph_index,
                        sample_count: stats.sample_count,
                        total_micros: stats.total_micros,
                        average_micros: stats.average_micros(),
                        min_micros: stats.min_micros,
                        max_micros: stats.max_micros,
                        effect_target_average_micros: effect_target.average_micros(),
                        scene_color_average_micros: scene_color.average_micros(),
                    }
                })
                .collect(),
            effect_command_measurement_scope: "effect-target-command",
            effect_commands: self
                .effect_commands
                .iter()
                .zip(self.effect_command_stats.iter().copied())
                .map(
                    |(command, stats)| NativeVulkanSceneEffectCommandGpuTimingSnapshot {
                        graph_index: command.graph_index,
                        graph_command_index: command.graph_command_index,
                        command_kind: command.command_kind,
                        sample_count: stats.sample_count,
                        average_micros: stats.average_micros(),
                        min_micros: stats.min_micros,
                        max_micros: stats.max_micros,
                    },
                )
                .collect(),
        }
    }

    pub(super) fn destroy(self, device: &Device) {
        unsafe {
            device.destroy_query_pool(self.query_pool, None);
        }
    }
}

fn graph_start_query(graph_position: usize) -> u32 {
    FRAME_TIMESTAMP_QUERY_COUNT
        + EFFECT_BATCH_TIMESTAMP_QUERY_COUNT
        + u32::try_from(graph_position).expect("GPU timing graph position fits u32")
            * GRAPH_TIMESTAMP_QUERY_COUNT
}

fn effect_batch_start_query() -> u32 {
    FRAME_TIMESTAMP_QUERY_COUNT
}

fn effect_command_start_query(graph_count: usize, command_position: usize) -> u32 {
    FRAME_TIMESTAMP_QUERY_COUNT
        + EFFECT_BATCH_TIMESTAMP_QUERY_COUNT
        + u32::try_from(graph_count).expect("GPU timing graph count fits u32")
            * GRAPH_TIMESTAMP_QUERY_COUNT
        + u32::try_from(command_position).expect("GPU timing command position fits u32") * 2
}

fn query_duration_micros(
    bytes: &[u8],
    start_query: u32,
    end_query: u32,
    valid_bits: u32,
    timestamp_period_nanoseconds: f32,
) -> f64 {
    let start = timestamp_value(bytes, start_query);
    let end = timestamp_value(bytes, end_query);
    let ticks = timestamp_delta(start, end, valid_bits);
    ticks as f64 * f64::from(timestamp_period_nanoseconds) / 1_000.0
}

fn timestamp_value(bytes: &[u8], query: u32) -> u64 {
    let start = query as usize * size_of::<u64>();
    let end = start + size_of::<u64>();
    u64::from_ne_bytes(bytes[start..end].try_into().expect("timestamp query bytes"))
}

fn timestamp_delta(start: u64, end: u64, valid_bits: u32) -> u64 {
    let mask = if valid_bits >= u64::BITS {
        u64::MAX
    } else {
        (1u64 << valid_bits) - 1
    };
    end.wrapping_sub(start) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_delta_handles_queue_counter_wrap() {
        assert_eq!(timestamp_delta(250, 5, 8), 11);
        assert_eq!(timestamp_delta(u64::MAX - 2, 4, 64), 7);
    }

    #[test]
    fn effect_command_queries_follow_all_graph_queries() {
        assert_eq!(effect_command_start_query(3, 0), 22);
        assert_eq!(effect_command_start_query(3, 2), 26);
    }
}
