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
use crate::engine::scene::event::{SceneMediaClockState, SceneMediaPlaybackState};
use crate::engine::scene::storage::SceneStorage;

use super::standard_library;
use crate::engine::scene::resolve_scene_user_properties;

const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const DEFAULT_STACK_LIMIT: usize = 512 * 1024;
const DEFAULT_GC_THRESHOLD: usize = 8 * 1024 * 1024;
const MODULE_DEADLINE: Duration = Duration::from_millis(50);
const FRAME_DEADLINE: Duration = Duration::from_millis(1);
const NUMERIC_DELTA_LANES: usize = 7;

const HOST_PRELUDE: &str = r#"
(() => {
    const programs = [];
    const audio = {
        average: new Float32Array(32),
        peak: new Float32Array(32),
    };
    const spectrum = new Float32Array(32);
    const pointer = { x: 0, y: 0 };
    const media = { state: 0, position: 0, duration: 0 };
    const texts = [];
    const emptyClicks = Object.freeze([]);
    globalThis.__gilderEmptyClicks = emptyClicks;
    let sceneEffects = [];
    let sceneEffectDirty = new Uint8Array(0);
    let sceneLayerByObject = new Map();
    let userProperties = Object.freeze(Object.create(null));
    let numeric = new Float64Array(0);
    const batch = { numeric, numericCount: 0, texts };
    globalThis.__gilderSpectrum = spectrum;
    globalThis.__gilderSetMedia = (state, position, duration) => {
        media.state = state;
        media.position = position;
        media.duration = duration;
    };
    globalThis.__gilderInstallHost = (host) => {
        const indexed = (selector, byIndex, byName, kind) => {
            if (typeof selector === 'number') {
                if (!Number.isSafeInteger(selector) || selector < 0) {
                    throw new TypeError(`SceneScript ${kind} numeric selector must be a non-negative safe integer`);
                }
                return byIndex.get(selector);
            }
            if (typeof selector === 'string') return byName.get(selector);
            throw new TypeError(`SceneScript ${kind} selector must be a string or integer`);
        };
        const layerByName = new Map();
        const layerByIndex = new Map();
        sceneLayerByObject.clear();
        sceneEffects = new Array(host.effectCount);
        sceneEffectDirty = new Uint8Array(host.effectCount);
        for (const definition of host.layers) {
            const effectByName = new Map();
            const effectByIndex = new Map();
            for (const effect of definition.effects) {
                const state = {
                    binding: effect.binding,
                    object: definition.object,
                    visible: effect.visible,
                };
                const proxy = Object.freeze({
                    get visible() { return state.visible; },
                    set visible(value) {
                        if (typeof value !== 'boolean') {
                            throw new TypeError('SceneScript effect visible requires a boolean');
                        }
                        state.visible = value;
                        sceneEffectDirty[state.binding] = 1;
                    },
                });
                sceneEffects[effect.binding] = state;
                if (!effectByName.has(effect.name)) effectByName.set(effect.name, proxy);
                effectByIndex.set(effect.index, proxy);
            }
            const state = { font: definition.font };
            const layer = Object.freeze({
                get font() { return state.font; },
                set font(value) {
                    if (typeof value !== 'string') {
                        throw new TypeError(`SceneScript layer ${definition.name} font requires an asset path string`);
                    }
                    if (!definition.text) {
                        state.font = value;
                        return;
                    }
                    if (definition.font === null) {
                        throw new TypeError(`SceneScript text layer ${definition.name} has no baked font resource`);
                    }
                    if (value !== definition.font) {
                        throw new RangeError(`SceneScript layer ${definition.name} font ${value} is not baked into this v21 artifact`);
                    }
                    state.font = value;
                },
                getEffect(selector) {
                    const effect = indexed(selector, effectByIndex, effectByName, 'effect');
                    if (effect === undefined) {
                        throw new RangeError(`SceneScript effect not found on layer ${definition.name}: ${selector}`);
                    }
                    return effect;
                },
            });
            if (!layerByName.has(definition.name)) layerByName.set(definition.name, layer);
            layerByIndex.set(definition.index, layer);
            sceneLayerByObject.set(definition.object, layer);
        }
        userProperties = Object.freeze(host.userProperties);
        globalThis.thisScene = Object.freeze({
            getLayer(selector) {
                const layer = indexed(selector, layerByIndex, layerByName, 'layer');
                if (layer === undefined) {
                    throw new RangeError(`SceneScript layer not found: ${selector}`);
                }
                return layer;
            },
        });
    };
    globalThis.__gilderSetCurrentLayer = (object) => {
        const layer = sceneLayerByObject.get(object);
        if (layer === undefined) {
            throw new RangeError(`SceneScript object has no layer: ${object}`);
        }
        globalThis.thisLayer = layer;
    };

    globalThis.engine = {
        runtime: 0,
        frametime: 0,
        AUDIO_RESOLUTION_16: 16,
        AUDIO_RESOLUTION_32: 32,
        AUDIO_RESOLUTION_64: 64,
        registerAudioBuffers() { return audio; },
        registerAsset(path) { return path; },
    };
    globalThis.MediaPlaybackEvent = Object.freeze({
        PLAYBACK_STOPPED: 0,
        PLAYBACK_PLAYING: 1,
        PLAYBACK_PAUSED: 2,
    });
    globalThis.WEMath = Object.freeze({
        clamp(value, minimum, maximum) {
            return Math.min(maximum, Math.max(minimum, value));
        },
        mix(left, right, amount) { return left + (right - left) * amount; },
        smoothstep(edge0, edge1, value) {
            const x = Math.min(1, Math.max(0, (value - edge0) / (edge1 - edge0)));
            return x * x * (3 - 2 * x);
        },
        deg2rad(value) { return value * Math.PI / 180; },
        rad2deg(value) { return value * 180 / Math.PI; },
    });
    globalThis.createScriptProperties = () => {
        const values = Object.create(null);
        const builder = {
            addSlider(definition) { values[definition.name] = definition.value; return builder; },
            addCheckbox(definition) { values[definition.name] = definition.value; return builder; },
            addCombo(definition) { values[definition.name] = definition.value; return builder; },
            addColor(definition) { values[definition.name] = definition.value; return builder; },
            addText(definition) { values[definition.name] = definition.value; return builder; },
            finish() { return values; },
        };
        return builder;
    };
    globalThis.thisObject = Object.freeze({
        getAnimation() { return this; },
        play() {},
        setFrame() {},
        addEndedCallback() {},
        frameCount: 1,
    });
    globalThis.input = {
        cursorPosition: pointer,
        cursorWorldPosition: pointer,
    };
    globalThis.shared = Object.create(null);
    globalThis.thisLayer = { font: null };

    function initialValue(metadata) {
        if (metadata.target <= 4) {
            return {
                x: metadata.initial[0],
                y: metadata.initial[1],
                z: metadata.initial[2],
            };
        }
        if (metadata.target === 5) return metadata.initial[0];
        if (metadata.target === 6) return metadata.initial[0] !== 0;
        if (metadata.target === 7) return metadata.initialText;
        return metadata.initial[0];
    }

    globalThis.__gilderRegister = (namespace, metadata, properties) => {
        const layer = sceneLayerByObject.get(metadata.object);
        if (layer === undefined) {
            throw new RangeError(`SceneScript object has no layer: ${metadata.object}`);
        }
        globalThis.thisLayer = layer;
        if (namespace.scriptProperties && properties) {
            for (const [name, bound] of Object.entries(properties)) {
                let value = bound;
                if (bound && typeof bound === 'object') {
                    if ('user' in bound) {
                        if (typeof bound.user !== 'string') {
                            throw new TypeError(`SceneScript property ${name} user binding must be a string`);
                        }
                        if (!Object.hasOwn(userProperties, bound.user)) {
                            throw new RangeError(`SceneScript property ${name} references unknown user property ${bound.user}`);
                        }
                        value = userProperties[bound.user];
                    } else if ('value' in bound) {
                        value = bound.value;
                    }
                }
                namespace.scriptProperties[name] = value;
            }
        }
        if (typeof namespace.applyUserProperties === 'function') {
            namespace.applyUserProperties(userProperties);
        }
        let value = initialValue(metadata);
        if (typeof namespace.init === 'function') {
            const initialized = namespace.init(value);
            if (initialized !== undefined) value = initialized;
        }
        programs.push({
            update: namespace.update,
            mediaPlaybackChanged: namespace.mediaPlaybackChanged,
            mediaTimelineChanged: namespace.mediaTimelineChanged,
            mediaPropertiesChanged: namespace.mediaPropertiesChanged,
            cursorClick: namespace.cursorClick,
            layer,
            object: metadata.object,
            target: metadata.target,
            subscriptions: metadata.subscriptions,
            value,
            published: false,
        });
    };

    function ensureNumericCapacity(entryCount) {
        const requiredLanes = entryCount * 7;
        if (numeric.length >= requiredLanes) return;
        const replacement = new Float64Array(requiredLanes);
        replacement.set(numeric);
        numeric = replacement;
        batch.numeric = numeric;
    }

    globalThis.__gilderDispatch = (time, frameTime, eventMask, pointerX, pointerY, clicks) => {
        engine.runtime = time;
        engine.frametime = frameTime;
        pointer.x = pointerX;
        pointer.y = pointerY;
        engine.pointer = pointer;
        if ((eventMask & 4) !== 0) {
            for (let i = 0; i < 32; i++) {
                const value = spectrum[i] || 0;
                audio.average[i] = value;
                audio.peak[i] = value;
            }
        }
        ensureNumericCapacity(programs.length + sceneEffects.length);
        texts.length = 0;
        let numericCount = 0;
        for (const program of programs) {
            const initialize = !program.published;
            if (!initialize && (program.subscriptions & eventMask) === 0) continue;
            globalThis.thisLayer = program.layer;
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaPlaybackChanged === 'function') {
                program.mediaPlaybackChanged(media);
            }
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaTimelineChanged === 'function') {
                program.mediaTimelineChanged(media);
            }
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaPropertiesChanged === 'function') {
                program.mediaPropertiesChanged(media);
            }
            if (typeof program.cursorClick === 'function') {
                for (const click of clicks) {
                    if (click.object === program.object) program.cursorClick(click);
                }
            }
            let output = program.value;
            if (typeof program.update === 'function' &&
                (program.subscriptions & eventMask) !== 0) {
                const resolved = program.update(program.value);
                output = resolved === undefined ? program.value : resolved;
            }
            program.value = output;
            program.published = true;
            if (program.target === 7) {
                texts.push([program.object, String(output)]);
                continue;
            }
            const base = numericCount * 7;
            numeric[base] = program.object;
            numeric[base + 1] = program.target;
            numeric[base + 2] = 0;
            if (program.target <= 4) {
                if (typeof output === 'number') {
                    const scalar = Number(output);
                    numeric[base + 3] = scalar;
                    numeric[base + 4] = scalar;
                    numeric[base + 5] = scalar;
                } else {
                    numeric[base + 3] = Number(output.x);
                    numeric[base + 4] = Number(output.y);
                    numeric[base + 5] = Number(output.z);
                }
            } else {
                numeric[base + 3] = program.target === 6 ? (output ? 1 : 0) : Number(output);
            }
            numericCount++;
        }
        for (let binding = 0; binding < sceneEffects.length; binding++) {
            if (sceneEffectDirty[binding] === 0) continue;
            const effect = sceneEffects[binding];
            if (effect === undefined) {
                throw new TypeError(`SceneScript host has no effect binding ${binding}`);
            }
            ensureNumericCapacity(numericCount + 1);
            const base = numericCount * 7;
            numeric[base] = effect.object;
            numeric[base + 1] = 9;
            numeric[base + 2] = binding;
            numeric[base + 3] = effect.visible ? 1 : 0;
            numeric[base + 4] = 0;
            numeric[base + 5] = 0;
            numeric[base + 6] = 0;
            sceneEffectDirty[binding] = 0;
            numericCount++;
        }
        batch.numericCount = numericCount;
        return batch;
    };
})();
"#;

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
    user_properties: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SceneScriptLayerCatalog {
    index: usize,
    object: u32,
    name: String,
    text: bool,
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
    pub audio_spectrum32: &'a [f32; 32],
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
            .map(|(index, object)| SceneScriptLayerCatalog {
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
                text: object.kind == crate::engine::scene::abi::SceneObjectKind::Text,
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
            })
            .collect();
        Ok(Self {
            layers,
            effect_count: storage.object_effects().len(),
            user_properties,
        })
    }

    #[cfg(test)]
    fn empty() -> Self {
        Self {
            layers: vec![SceneScriptLayerCatalog {
                index: 0,
                object: 3,
                name: String::new(),
                text: false,
                font: None,
                effects: Vec::new(),
            }],
            effect_count: 0,
            user_properties: Map::new(),
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
            ctx.eval::<(), _>(HOST_PRELUDE)
                .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
            install_host_catalog(ctx.clone(), host)?;
            for (module_index, program) in programs.iter().enumerate() {
                deadline.set(Some(Instant::now() + MODULE_DEADLINE));
                let result = register_program(ctx.clone(), module_index, program);
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
        self.deadline.set(Some(Instant::now() + FRAME_DEADLINE));
        let result = self.context.with(|ctx| {
            let dispatch: Function = ctx
                .globals()
                .get("__gilderDispatch")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            let spectrum: Object = ctx
                .globals()
                .get("__gilderSpectrum")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            if input.dirty_events.contains(SceneScriptSubscriptions::AUDIO) {
                for (index, value) in input.audio_spectrum32.iter().enumerate() {
                    spectrum
                        .set(index as u32, *value)
                        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
                }
            }
            if input.dirty_events.contains(SceneScriptSubscriptions::MEDIA) {
                let set_media: Function = ctx
                    .globals()
                    .get("__gilderSetMedia")
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
            let batch: Object = dispatch
                .call((
                    input.scene_time_seconds,
                    input.frame_time_seconds,
                    input.dirty_events.0,
                    input.pointer[0],
                    input.pointer[1],
                    pointer_click_array(ctx.clone(), input.pointer_clicks)?,
                ))
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            decode_batch_into(batch, deltas)
        });
        let deadline_expired = self
            .deadline
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline);
        self.deadline.set(None);
        if result.is_err() && deadline_expired {
            return Err(SceneScriptError::DeadlineExceeded);
        }
        result
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
        .get("__gilderSetCurrentLayer")
        .map_err(|error| SceneScriptError::InstallHost(error.to_string()))?;
    set_current_layer
        .call::<_, ()>((program.record.object.0,))
        .catch(&ctx)
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let module_name = format!("gilder:scene-script/{module_index}");
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
        .get("__gilderRegister")
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
        .get("__gilderInstallHost")
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
            .get("__gilderEmptyClicks")
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
