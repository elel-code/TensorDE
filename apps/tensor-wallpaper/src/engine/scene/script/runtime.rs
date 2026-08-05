use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::{Array, CatchResultExt, Context, Function, Module, Object, Runtime, TypedArray};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::engine::scene::abi::{
    SceneObjectHandle, SceneScriptProgramRecord, SceneScriptSubscriptions, SceneScriptTarget,
};
use crate::engine::scene::event::{
    SceneMediaClockState, SceneMediaPlaybackState, StereoSpectrum64,
};
use crate::engine::scene::storage::SceneStorage;

use super::standard_library;
use crate::engine::scene::resolve_scene_user_properties;

mod host_math;

const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const DEFAULT_STACK_LIMIT: usize = 512 * 1024;
const DEFAULT_GC_THRESHOLD: usize = 8 * 1024 * 1024;
const MODULE_DEADLINE: Duration = Duration::from_millis(50);
const FRAME_DEADLINE: Duration = Duration::from_millis(1);
const NUMERIC_DELTA_LANES: usize = 7;

const HOST_PRELUDE: &str = include_str!("runtime/host_runtime.js");

#[derive(Debug, Clone, PartialEq)]
pub struct SceneScriptProgram {
    pub record: SceneScriptProgramRecord,
    pub source: String,
    pub properties_json: String,
    pub initial_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SceneScriptHostCatalog {
    layers: Vec<SceneScriptLayerCatalog>,
    effect_count: usize,
    canvas_size: [u32; 2],
    user_properties: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SceneScriptLayerCatalog {
    index: usize,
    object: u32,
    name: String,
    parent: Option<u32>,
    origin: [f32; 3],
    angles: [f32; 3],
    scale: [f32; 3],
    color: [f32; 3],
    alpha: f32,
    visible: bool,
    size: [f32; 2],
    alignment: String,
    text: bool,
    sound: bool,
    font: Option<String>,
    effects: Vec<SceneScriptEffectCatalog>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SceneScriptEffectCatalog {
    index: usize,
    binding: usize,
    name: String,
    visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneScriptPointerClick {
    pub object: SceneObjectHandle,
    pub button: u32,
    pub pointer: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneScriptFrameInput<'a> {
    pub scene_time_seconds: f64,
    pub frame_time_seconds: f64,
    pub dirty_events: SceneScriptSubscriptions,
    pub pointer: [f32; 2],
    pub pointer_clicks: &'a [SceneScriptPointerClick],
    pub audio_spectrum: &'a StereoSpectrum64,
    pub media: Option<SceneMediaClockState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneScriptDelta {
    pub object: SceneObjectHandle,
    pub target: SceneScriptTarget,
    pub selector: u32,
    pub numeric: [f32; 4],
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneScriptMemorySnapshot {
    pub malloc_bytes: i64,
    pub memory_used_bytes: i64,
    pub allocation_count: i64,
    pub object_count: i64,
    pub property_count: i64,
    pub string_count: i64,
}

pub struct SceneScriptRuntime {
    runtime: Runtime,
    context: Context,
    deadline: Rc<Cell<Option<Instant>>>,
    program_count: usize,
}

impl fmt::Debug for SceneScriptRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneScriptRuntime")
            .field("program_count", &self.program_count)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneScriptError {
    CreateRuntime(String),
    CreateContext(String),
    InstallHost(String),
    CompileModule { module: usize, message: String },
    Dispatch(String),
    DeadlineExceeded,
    InvalidDeltaTarget(u32),
    InvalidProjectProperties(String),
    InvalidDeltaNumber { module_object: u32 },
}

impl fmt::Display for SceneScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateRuntime(message) => {
                write!(formatter, "create SceneScript runtime: {message}")
            }
            Self::CreateContext(message) => {
                write!(formatter, "create SceneScript context: {message}")
            }
            Self::InstallHost(message) => write!(formatter, "install SceneScript host: {message}"),
            Self::CompileModule { module, message } => {
                write!(formatter, "compile SceneScript module {module}: {message}")
            }
            Self::Dispatch(message) => write!(formatter, "dispatch SceneScript batch: {message}"),
            Self::DeadlineExceeded => {
                formatter.write_str("SceneScript execution deadline exceeded")
            }
            Self::InvalidDeltaTarget(target) => {
                write!(formatter, "invalid script delta target {target}")
            }
            Self::InvalidProjectProperties(message) => {
                write!(formatter, "invalid scene project properties: {message}")
            }
            Self::InvalidDeltaNumber { module_object } => {
                write!(
                    formatter,
                    "script for object {module_object} returned a non-finite number"
                )
            }
        }
    }
}

impl std::error::Error for SceneScriptError {}

impl SceneScriptHostCatalog {
    fn from_storage(
        storage: &SceneStorage,
        user_property_overrides: &Map<String, Value>,
    ) -> Result<Self, SceneScriptError> {
        let user_properties = resolve_scene_user_properties(storage, user_property_overrides)
            .map_err(|error| SceneScriptError::InvalidProjectProperties(error.to_string()))?;
        let layers = storage
            .objects()
            .iter()
            .enumerate()
            .map(|(index, object)| {
                let mesh = storage
                    .meshes()
                    .iter()
                    .find(|mesh| mesh.object == object.id);
                SceneScriptLayerCatalog {
                    index,
                    object: object.id.0,
                    name: if object.name.is_some() {
                        storage
                            .string(object.name)
                            .expect("scene storage validates object name strings")
                    } else {
                        ""
                    }
                    .to_owned(),
                    parent: storage
                        .objects()
                        .iter()
                        .find(|candidate| candidate.we_id == object.parent_we_id)
                        .map(|parent| parent.id.0),
                    origin: [object.origin.x, object.origin.y, object.origin.z],
                    angles: [object.angles.x, object.angles.y, object.angles.z],
                    scale: [object.scale.x, object.scale.y, object.scale.z],
                    color: [object.color.x, object.color.y, object.color.z],
                    alpha: object.alpha,
                    visible: object.visible,
                    size: mesh.map_or([0.0; 2], |mesh| [mesh.width, mesh.height]),
                    alignment: "center".to_owned(),
                    text: object.kind == crate::engine::scene::abi::SceneObjectKind::Text,
                    sound: storage.resource(object.resource).is_some_and(|resource| {
                        resource.kind == crate::engine::scene::abi::SceneResourceKind::Audio
                    }),
                    font: (object.kind == crate::engine::scene::abi::SceneObjectKind::Text)
                        .then(|| storage.resource(object.resource))
                        .flatten()
                        .filter(|resource| {
                            resource.kind == crate::engine::scene::abi::SceneResourceKind::Font
                        })
                        .map(|resource| {
                            if resource.path.is_some() {
                                storage
                                    .string(resource.path)
                                    .expect("scene storage validates font resource paths")
                                    .to_owned()
                            } else {
                                String::new()
                            }
                        }),
                    effects: storage
                        .object_effects_for_object(object)
                        .iter()
                        .enumerate()
                        .map(|(effect_index, effect)| SceneScriptEffectCatalog {
                            index: effect_index,
                            binding: object.effect_start as usize + effect_index,
                            name: if effect.name.is_some() {
                                storage
                                    .string(effect.name)
                                    .expect("scene storage validates effect name strings")
                            } else {
                                ""
                            }
                            .to_owned(),
                            visible: effect.visible,
                        })
                        .collect(),
                }
            })
            .collect();
        Ok(Self {
            layers,
            effect_count: storage.object_effects().len(),
            canvas_size: [
                storage.project().logical_width,
                storage.project().logical_height,
            ],
            user_properties,
        })
    }

    #[cfg(test)]
    fn empty() -> Self {
        Self {
            layers: vec![SceneScriptLayerCatalog::test(0, 3, "")],
            effect_count: 0,
            canvas_size: [1920, 1080],
            user_properties: Map::new(),
        }
    }
}

#[cfg(test)]
impl SceneScriptLayerCatalog {
    fn test(index: usize, object: u32, name: &str) -> Self {
        Self {
            index,
            object,
            name: name.to_owned(),
            parent: None,
            origin: [0.0; 3],
            angles: [0.0; 3],
            scale: [1.0; 3],
            color: [1.0; 3],
            alpha: 1.0,
            visible: true,
            size: [128.0; 2],
            alignment: "center".to_owned(),
            text: false,
            sound: false,
            font: None,
            effects: Vec::new(),
        }
    }
}

impl SceneScriptRuntime {
    pub fn validate_user_property_overrides(
        storage: &SceneStorage,
        user_property_overrides: &Map<String, Value>,
    ) -> Result<(), SceneScriptError> {
        SceneScriptHostCatalog::from_storage(storage, user_property_overrides).map(|_| ())
    }

    pub fn from_storage(
        storage: &SceneStorage,
        user_property_overrides: &Map<String, Value>,
    ) -> Result<Option<Self>, SceneScriptError> {
        let host = SceneScriptHostCatalog::from_storage(storage, user_property_overrides)?;
        if storage.script_programs().is_empty() {
            return Ok(None);
        }
        let programs = storage
            .script_programs()
            .iter()
            .map(|record| SceneScriptProgram {
                record: *record,
                source: storage
                    .string(record.source)
                    .expect("scene storage validates script source strings")
                    .to_owned(),
                properties_json: storage
                    .string(record.properties_json)
                    .expect("scene storage validates script property strings")
                    .to_owned(),
                initial_text: if record.initial_text.is_some() {
                    storage
                        .string(record.initial_text)
                        .expect("scene storage validates script initial text strings")
                } else {
                    ""
                }
                .to_owned(),
            })
            .collect::<Vec<_>>();
        Self::new(&programs, &host).map(Some)
    }

    fn new(
        programs: &[SceneScriptProgram],
        host: &SceneScriptHostCatalog,
    ) -> Result<Self, SceneScriptError> {
        let runtime =
            Runtime::new().map_err(|error| SceneScriptError::CreateRuntime(error.to_string()))?;
        runtime.set_memory_limit(DEFAULT_MEMORY_LIMIT);
        runtime.set_max_stack_size(DEFAULT_STACK_LIMIT);
        runtime.set_gc_threshold(DEFAULT_GC_THRESHOLD);
        standard_library::install(&runtime);
        let deadline = Rc::new(Cell::new(None::<Instant>));
        let interrupt_deadline = Rc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt_deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline)
        })));
        let context = Context::full(&runtime)
            .map_err(|error| SceneScriptError::CreateContext(error.to_string()))?;
        let result = context.with(|ctx| {
            ctx.eval::<(), _>(host_math::HOST_MATH_PRELUDE)
                .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
            ctx.eval::<(), _>(HOST_PRELUDE)
                .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
            install_host_catalog(ctx.clone(), host)?;
            for (module_index, program) in programs.iter().enumerate() {
                deadline.set(Some(Instant::now() + MODULE_DEADLINE));
                let result = register_program(ctx.clone(), module_index, program).map_err(
                    |error| match error {
                        SceneScriptError::CompileModule { module, message } => {
                            SceneScriptError::CompileModule {
                                module,
                                message: format!(
                                    "object {} target {:?}: {message}",
                                    program.record.object.0, program.record.target
                                ),
                            }
                        }
                        error => error,
                    },
                );
                deadline.set(None);
                result?;
            }
            Ok(())
        });
        result?;
        Ok(Self {
            runtime,
            context,
            deadline,
            program_count: programs.len(),
        })
    }

    pub fn dispatch_into(
        &self,
        input: SceneScriptFrameInput<'_>,
        deltas: &mut Vec<SceneScriptDelta>,
    ) -> Result<(), SceneScriptError> {
        self.context.with(|ctx| {
            let dispatch: Function = ctx
                .globals()
                .get("__tensor_wallpaperDispatch")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            let spectrum_left: Object = ctx
                .globals()
                .get("__tensor_wallpaperSpectrumLeft")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            let spectrum_right: Object = ctx
                .globals()
                .get("__tensor_wallpaperSpectrumRight")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            if input.dirty_events.contains(SceneScriptSubscriptions::AUDIO) {
                for (index, (left, right)) in input
                    .audio_spectrum
                    .left
                    .iter()
                    .zip(&input.audio_spectrum.right)
                    .enumerate()
                {
                    spectrum_left
                        .set(index as u32, *left)
                        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
                    spectrum_right
                        .set(index as u32, *right)
                        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
                }
            }
            if input.dirty_events.contains(SceneScriptSubscriptions::MEDIA) {
                let set_media: Function = ctx
                    .globals()
                    .get("__tensor_wallpaperSetMedia")
                    .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
                set_media
                    .call::<_, ()>((
                        media_playback_state(input.media),
                        input
                            .media
                            .map(|media| media.clock_ns as f64 / 1_000_000_000.0)
                            .unwrap_or(0.0),
                        input
                            .media
                            .and_then(|media| media.duration_ns)
                            .map(|duration| duration as f64 / 1_000_000_000.0)
                            .unwrap_or(0.0),
                    ))
                    .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            }
            let pointer_clicks = pointer_click_array(ctx.clone(), input.pointer_clicks)?;
            self.deadline.set(Some(Instant::now() + FRAME_DEADLINE));
            let dispatch_result = dispatch
                .call((
                    input.scene_time_seconds,
                    input.frame_time_seconds,
                    input.dirty_events.0,
                    input.pointer[0],
                    input.pointer[1],
                    pointer_clicks,
                ))
                .catch(&ctx)
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()));
            let deadline_expired = self
                .deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline);
            self.deadline.set(None);
            let batch: Object = match dispatch_result {
                Err(_) if deadline_expired => return Err(SceneScriptError::DeadlineExceeded),
                result => result?,
            };
            decode_batch_into(batch, deltas)
        })
    }

    pub fn program_count(&self) -> usize {
        self.program_count
    }

    pub fn run_gc(&self) {
        self.runtime.run_gc();
    }

    pub fn memory_snapshot(&self) -> SceneScriptMemorySnapshot {
        let usage = self.runtime.memory_usage();
        SceneScriptMemorySnapshot {
            malloc_bytes: usage.malloc_size,
            memory_used_bytes: usage.memory_used_size,
            allocation_count: usage.malloc_count,
            object_count: usage.obj_count,
            property_count: usage.prop_count,
            string_count: usage.str_count,
        }
    }
}

fn register_program<'js>(
    ctx: rquickjs::Ctx<'js>,
    module_index: usize,
    program: &SceneScriptProgram,
) -> Result<(), SceneScriptError> {
    let set_current_layer: Function = ctx
        .globals()
        .get("__tensor_wallpaperSetCurrentLayer")
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    set_current_layer
        .call::<_, ()>((program.record.object.0,))
        .catch(&ctx)
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let module_name = format!("tensor-wallpaper:scene-script/{module_index}");
    let module = Module::declare(ctx.clone(), module_name, program.source.as_bytes())
        .catch(&ctx)
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let (module, evaluation) =
        module
            .eval()
            .catch(&ctx)
            .map_err(|error| SceneScriptError::CompileModule {
                module: module_index,
                message: error.to_string(),
            })?;
    evaluation
        .finish::<()>()
        .catch(&ctx)
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let namespace =
        module
            .namespace()
            .catch(&ctx)
            .map_err(|error| SceneScriptError::CompileModule {
                module: module_index,
                message: error.to_string(),
            })?;
    let metadata = Object::new(ctx.clone()).map_err(|error| SceneScriptError::CompileModule {
        module: module_index,
        message: error.to_string(),
    })?;
    let initial = Array::new(ctx.clone()).map_err(|error| SceneScriptError::CompileModule {
        module: module_index,
        message: error.to_string(),
    })?;
    for (index, value) in program.record.initial_numeric.iter().enumerate() {
        initial
            .set(index, *value)
            .map_err(|error| SceneScriptError::CompileModule {
                module: module_index,
                message: error.to_string(),
            })?;
    }
    metadata
        .set("object", program.record.object.0)
        .and_then(|_| metadata.set("target", program.record.target.to_u32()))
        .and_then(|_| metadata.set("selector", program.record.selector))
        .and_then(|_| metadata.set("subscriptions", program.record.subscriptions.0))
        .and_then(|_| metadata.set("initial", initial))
        .and_then(|_| metadata.set("initialText", program.initial_text.as_str()))
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let properties = ctx
        .json_parse(if program.properties_json.is_empty() {
            "{}"
        } else {
            &program.properties_json
        })
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let register: Function = ctx
        .globals()
        .get("__tensor_wallpaperRegister")
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    register
        .call::<_, ()>((namespace, metadata, properties))
        .catch(&ctx)
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })
}

fn install_host_catalog<'js>(
    ctx: rquickjs::Ctx<'js>,
    host: &SceneScriptHostCatalog,
) -> Result<(), SceneScriptError> {
    let json = serde_json::to_string(host)
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    let value = ctx
        .json_parse(json.as_str())
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    let install: Function = ctx
        .globals()
        .get("__tensor_wallpaperInstallHost")
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    install
        .call::<_, ()>((value,))
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))
}

fn pointer_click_array<'js>(
    ctx: rquickjs::Ctx<'js>,
    clicks: &[SceneScriptPointerClick],
) -> Result<Array<'js>, SceneScriptError> {
    if clicks.is_empty() {
        return ctx
            .globals()
            .get("__tensor_wallpaperEmptyClicks")
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()));
    }
    let array =
        Array::new(ctx.clone()).map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    for (index, click) in clicks.iter().enumerate() {
        let event = Object::new(ctx.clone())
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
        let position = Object::new(ctx.clone())
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
        position
            .set("x", click.pointer[0])
            .and_then(|_| position.set("y", click.pointer[1]))
            .and_then(|_| event.set("object", click.object.0))
            .and_then(|_| event.set("button", click.button))
            .and_then(|_| event.set("position", position))
            .and_then(|_| array.set(index, event))
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    }
    Ok(array)
}

fn media_playback_state(media: Option<SceneMediaClockState>) -> u32 {
    match media.map(|media| media.playback) {
        Some(SceneMediaPlaybackState::Playing) => 1,
        Some(SceneMediaPlaybackState::Paused) => 2,
        _ => 0,
    }
}

fn decode_batch_into(
    batch: Object<'_>,
    deltas: &mut Vec<SceneScriptDelta>,
) -> Result<(), SceneScriptError> {
    let numeric: TypedArray<f64> = batch
        .get("numeric")
        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    let numeric_count: usize = batch
        .get("numericCount")
        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    let values: &[f64] = numeric.as_ref();
    deltas.clear();
    deltas.reserve(numeric_count);
    for lanes in values.chunks_exact(NUMERIC_DELTA_LANES).take(numeric_count) {
        let object = lanes[0] as u32;
        let target_raw = lanes[1] as u32;
        let target = SceneScriptTarget::from_u32(target_raw)
            .ok_or(SceneScriptError::InvalidDeltaTarget(target_raw))?;
        let mut output = [0.0_f32; 4];
        for (target, value) in output.iter_mut().zip(&lanes[3..7]) {
            if !value.is_finite() {
                return Err(SceneScriptError::InvalidDeltaNumber {
                    module_object: object,
                });
            }
            *target = *value as f32;
        }
        deltas.push(SceneScriptDelta {
            object: SceneObjectHandle(object),
            target,
            selector: lanes[2] as u32,
            numeric: output,
            text: None,
        });
    }
    let texts: Array = batch
        .get("texts")
        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    for entry in texts.iter::<Array>() {
        let entry = entry.map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
        let object = entry
            .get::<u32>(0)
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
        let text = entry
            .get::<String>(1)
            .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
        deltas.push(SceneScriptDelta {
            object: SceneObjectHandle(object),
            target: SceneScriptTarget::Text,
            selector: 0,
            numeric: [0.0; 4],
            text: Some(text),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
