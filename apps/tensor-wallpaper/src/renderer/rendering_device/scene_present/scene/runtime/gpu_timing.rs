//! Optional shared-renderer GPU timestamp diagnostics.

use serde::Serialize;
use vulkan_renderer::{
    Backend, CommandEncoder, TimestampQuery, TimestampQueryResults, TimestampQuerySet,
    TimestampQuerySetDescriptor, TimestampWriteStage,
};

use super::shared_scene::SharedSceneGpuResources;

const FRAME_INTERVAL: usize = 0;
const PARTICLE_INTERVAL: usize = 1;
const EFFECT_BATCH_INTERVAL: usize = 2;
const FIXED_INTERVAL_COUNT: usize = 3;

#[derive(Debug, Clone)]
pub(super) struct SceneGpuTimingPassDescriptor {
    pub label: String,
    pub pass_record_indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub(super) struct SceneGpuTimingGraphDescriptor {
    pub graph_index: u32,
    pub passes: Vec<SceneGpuTimingPassDescriptor>,
}

#[derive(Debug, Clone)]
struct SceneGpuTimingInterval {
    category: &'static str,
    label: String,
    graph_index: Option<u32>,
    pass_record_indices: Vec<u32>,
    queries: [TimestampQuery; 2],
    stats: GpuDurationStats,
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

#[derive(Debug)]
pub(super) struct SceneGpuTiming {
    query_sets: Vec<TimestampQuerySet>,
    pending: Vec<bool>,
    intervals: Vec<SceneGpuTimingInterval>,
    graph_interval_indices: Vec<usize>,
    graph_pass_interval_indices: Vec<Vec<usize>>,
}

impl SceneGpuTiming {
    pub(super) fn create(
        device: &Backend,
        scene: &SharedSceneGpuResources,
        frame_slot_count: usize,
        enabled: bool,
    ) -> Result<Option<Self>, String> {
        if !enabled {
            return Ok(None);
        }
        let graphs = scene.gpu_timing_graphs()?;
        let interval_count = FIXED_INTERVAL_COUNT
            .checked_add(graphs.len())
            .and_then(|count| {
                graphs
                    .iter()
                    .try_fold(count, |count, graph| count.checked_add(graph.passes.len()))
            })
            .ok_or_else(|| "scene GPU timing interval count overflows".to_owned())?;
        let query_count = u32::try_from(interval_count)
            .ok()
            .and_then(|count| count.checked_mul(2))
            .ok_or_else(|| "scene GPU timing query count exceeds u32".to_owned())?;
        let mut intervals = Vec::with_capacity(interval_count);
        for (category, label) in [
            ("frame", "offscreen-frame"),
            ("compute", "particle-update"),
            ("batch", "effect-batches"),
        ] {
            push_interval(&mut intervals, category, label.into(), None, Vec::new())?;
        }
        let mut graph_interval_indices = Vec::with_capacity(graphs.len());
        let mut graph_pass_interval_indices = Vec::with_capacity(graphs.len());
        for graph in graphs {
            graph_interval_indices.push(intervals.len());
            push_interval(
                &mut intervals,
                "graph",
                format!("graph-{}", graph.graph_index),
                Some(graph.graph_index),
                Vec::new(),
            )?;
            let mut pass_indices = Vec::with_capacity(graph.passes.len());
            for pass in graph.passes {
                pass_indices.push(intervals.len());
                push_interval(
                    &mut intervals,
                    "pass",
                    pass.label,
                    Some(graph.graph_index),
                    pass.pass_record_indices,
                )?;
            }
            graph_pass_interval_indices.push(pass_indices);
        }
        let query_sets = (0..frame_slot_count)
            .map(|frame_slot| {
                device
                    .create_timestamp_query_set(&TimestampQuerySetDescriptor {
                        label: Some(format!("tensor-wallpaper-scene-gpu-timing-frame-{frame_slot}")),
                        count: query_count,
                    })
                    .map_err(|error| format!("create shared scene GPU timestamps: {error}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Some(Self {
            query_sets,
            pending: vec![false; frame_slot_count],
            intervals,
            graph_interval_indices,
            graph_pass_interval_indices,
        }))
    }

    pub(super) fn collect_slot(&mut self, frame_slot: usize) -> Result<(), String> {
        let pending = self
            .pending
            .get_mut(frame_slot)
            .ok_or_else(|| format!("scene GPU timing frame slot {frame_slot} is missing"))?;
        if !*pending {
            return Ok(());
        }
        let results = self
            .query_sets
            .get(frame_slot)
            .ok_or_else(|| format!("scene GPU timing query set {frame_slot} is missing"))?
            .read_completed()
            .map_err(|error| format!("read shared scene GPU timestamps: {error}"))?;
        observe_results(&mut self.intervals, &results)?;
        *pending = false;
        Ok(())
    }

    pub(super) fn collect_all(&mut self) -> Result<(), String> {
        for frame_slot in 0..self.query_sets.len() {
            self.collect_slot(frame_slot)?;
        }
        Ok(())
    }

    pub(super) fn frame(&self, frame_slot: usize) -> Result<SceneGpuTimingFrame<'_>, String> {
        let queries = self
            .query_sets
            .get(frame_slot)
            .ok_or_else(|| format!("scene GPU timing frame slot {frame_slot} is missing"))?;
        Ok(SceneGpuTimingFrame {
            queries,
            intervals: &self.intervals,
            graph_interval_indices: &self.graph_interval_indices,
            graph_pass_interval_indices: &self.graph_pass_interval_indices,
        })
    }

    pub(super) fn mark_submitted(&mut self, frame_slot: usize) -> Result<(), String> {
        let pending = self
            .pending
            .get_mut(frame_slot)
            .ok_or_else(|| format!("scene GPU timing frame slot {frame_slot} is missing"))?;
        if *pending {
            return Err(format!(
                "scene GPU timing frame slot {frame_slot} was resubmitted before collection"
            ));
        }
        *pending = true;
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Result<serde_json::Value, String> {
        let query_set = self
            .query_sets
            .first()
            .ok_or_else(|| "scene GPU timing has no query set".to_owned())?;
        serde_json::to_value(SceneGpuTimingSnapshot {
            measurement_scope: "offscreen-command-buffer",
            timestamp_period_nanoseconds: query_set.timestamp_period_nanoseconds(),
            timestamp_valid_bits: query_set.timestamp_valid_bits(),
            intervals: self
                .intervals
                .iter()
                .map(|interval| SceneGpuTimingIntervalSnapshot {
                    category: interval.category,
                    label: &interval.label,
                    graph_index: interval.graph_index,
                    pass_record_indices: &interval.pass_record_indices,
                    sample_count: interval.stats.sample_count,
                    total_micros: interval.stats.total_micros,
                    average_micros: interval.stats.average_micros(),
                    min_micros: interval.stats.min_micros,
                    max_micros: interval.stats.max_micros,
                })
                .collect(),
        })
        .map_err(|error| format!("serialize scene GPU timing: {error}"))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneGpuTimingFrame<'a> {
    queries: &'a TimestampQuerySet,
    intervals: &'a [SceneGpuTimingInterval],
    graph_interval_indices: &'a [usize],
    graph_pass_interval_indices: &'a [Vec<usize>],
}

impl SceneGpuTimingFrame<'_> {
    pub(super) fn reset_and_start_frame(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        encoder
            .reset_timestamp_queries(
                self.queries,
                TimestampQuerySet::query(0),
                self.queries.count(),
            )
            .map_err(|error| format!("reset shared scene GPU timestamps: {error}"))?;
        self.start(encoder, FRAME_INTERVAL)
    }

    pub(super) fn finish_frame(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.finish(encoder, FRAME_INTERVAL)
    }

    pub(super) fn start_particle(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.start(encoder, PARTICLE_INTERVAL)
    }

    pub(super) fn finish_particle(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.finish(encoder, PARTICLE_INTERVAL)
    }

    pub(super) fn start_effect_batches(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.start(encoder, EFFECT_BATCH_INTERVAL)
    }

    pub(super) fn finish_effect_batches(self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.finish(encoder, EFFECT_BATCH_INTERVAL)
    }

    pub(super) fn start_graph(
        self,
        encoder: &mut CommandEncoder,
        graph_position: usize,
    ) -> Result<(), String> {
        self.start(encoder, self.graph_interval(graph_position)?)
    }

    pub(super) fn finish_graph(
        self,
        encoder: &mut CommandEncoder,
        graph_position: usize,
    ) -> Result<(), String> {
        self.finish(encoder, self.graph_interval(graph_position)?)
    }

    pub(super) fn start_pass(
        self,
        encoder: &mut CommandEncoder,
        graph_position: usize,
        pass_position: usize,
    ) -> Result<(), String> {
        self.start(encoder, self.pass_interval(graph_position, pass_position)?)
    }

    pub(super) fn finish_pass(
        self,
        encoder: &mut CommandEncoder,
        graph_position: usize,
        pass_position: usize,
    ) -> Result<(), String> {
        self.finish(encoder, self.pass_interval(graph_position, pass_position)?)
    }

    fn graph_interval(self, graph_position: usize) -> Result<usize, String> {
        self.graph_interval_indices
            .get(graph_position)
            .copied()
            .ok_or_else(|| format!("scene GPU timing graph position {graph_position} is missing"))
    }

    fn pass_interval(self, graph_position: usize, pass_position: usize) -> Result<usize, String> {
        self.graph_pass_interval_indices
            .get(graph_position)
            .and_then(|passes| passes.get(pass_position))
            .copied()
            .ok_or_else(|| {
                format!(
                    "scene GPU timing graph {graph_position} pass position {pass_position} is missing"
                )
            })
    }

    fn start(self, encoder: &mut CommandEncoder, interval: usize) -> Result<(), String> {
        self.write(encoder, interval, 0, TimestampWriteStage::TopOfPipe)
    }

    fn finish(self, encoder: &mut CommandEncoder, interval: usize) -> Result<(), String> {
        self.write(encoder, interval, 1, TimestampWriteStage::BottomOfPipe)
    }

    fn write(
        self,
        encoder: &mut CommandEncoder,
        interval: usize,
        boundary: usize,
        stage: TimestampWriteStage,
    ) -> Result<(), String> {
        let query = self
            .intervals
            .get(interval)
            .and_then(|interval| interval.queries.get(boundary))
            .copied()
            .ok_or_else(|| format!("scene GPU timing interval {interval} is missing"))?;
        encoder
            .write_timestamp(self.queries, query, stage)
            .map_err(|error| format!("write shared scene GPU timestamp: {error}"))
    }
}

fn push_interval(
    intervals: &mut Vec<SceneGpuTimingInterval>,
    category: &'static str,
    label: String,
    graph_index: Option<u32>,
    pass_record_indices: Vec<u32>,
) -> Result<(), String> {
    let first = u32::try_from(intervals.len())
        .ok()
        .and_then(|index| index.checked_mul(2))
        .ok_or_else(|| "scene GPU timing interval query index exceeds u32".to_owned())?;
    intervals.push(SceneGpuTimingInterval {
        category,
        label,
        graph_index,
        pass_record_indices,
        queries: [
            TimestampQuerySet::query(first),
            TimestampQuerySet::query(first + 1),
        ],
        stats: GpuDurationStats::default(),
    });
    Ok(())
}

fn observe_results(
    intervals: &mut [SceneGpuTimingInterval],
    results: &TimestampQueryResults,
) -> Result<(), String> {
    for interval in intervals {
        let micros = results
            .duration_micros(interval.queries[0], interval.queries[1])
            .map_err(|error| format!("convert shared scene GPU timestamp interval: {error}"))?;
        interval.stats.observe(micros);
    }
    Ok(())
}

#[derive(Serialize)]
struct SceneGpuTimingSnapshot<'a> {
    measurement_scope: &'static str,
    timestamp_period_nanoseconds: f32,
    timestamp_valid_bits: u32,
    intervals: Vec<SceneGpuTimingIntervalSnapshot<'a>>,
}

#[derive(Serialize)]
struct SceneGpuTimingIntervalSnapshot<'a> {
    category: &'static str,
    label: &'a str,
    graph_index: Option<u32>,
    pass_record_indices: &'a [u32],
    sample_count: u64,
    total_micros: f64,
    average_micros: Option<f64>,
    min_micros: Option<f64>,
    max_micros: Option<f64>,
}
