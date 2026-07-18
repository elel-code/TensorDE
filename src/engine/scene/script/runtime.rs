use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::{Array, Context, Function, Module, Object, Runtime, TypedArray};

use crate::engine::scene::abi::{
    SceneObjectHandle, SceneScriptProgramRecord, SceneScriptSubscriptions, SceneScriptTarget,
};
use crate::engine::scene::storage::SceneStorage;

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

    globalThis.engine = {
        runtime: 0,
        frametime: 0,
        AUDIO_RESOLUTION_16: 16,
        AUDIO_RESOLUTION_32: 32,
        AUDIO_RESOLUTION_64: 64,
        registerAudioBuffers() { return audio; },
        registerAsset(path) { return path; },
    };
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

    globalThis.__gilderRegister = (namespace, metadata, properties) => {
        if (namespace.scriptProperties && properties) {
            for (const [name, bound] of Object.entries(properties)) {
                namespace.scriptProperties[name] =
                    bound && typeof bound === 'object' && 'value' in bound ? bound.value : bound;
            }
        }
        if (typeof namespace.update !== 'function') return;
        programs.push({
            update: namespace.update,
            object: metadata.object,
            target: metadata.target,
            subscriptions: metadata.subscriptions,
            initial: metadata.initial,
            initialText: metadata.initialText,
        });
    };

    globalThis.__gilderDispatch = (time, frameTime, eventMask, pointerX, pointerY, spectrum) => {
        engine.runtime = time;
        engine.frametime = frameTime;
        engine.pointer = { x: pointerX, y: pointerY };
        for (let i = 0; i < 32; i++) {
            const value = spectrum[i] || 0;
            audio.average[i] = value;
            audio.peak[i] = value;
        }
        const numeric = new Float64Array(programs.length * 7);
        const texts = [];
        let numericCount = 0;
        for (const program of programs) {
            if ((program.subscriptions & eventMask) === 0) continue;
            let value;
            if (program.target <= 4) {
                value = { x: program.initial[0], y: program.initial[1], z: program.initial[2] };
            } else if (program.target === 5) {
                value = program.initial[0];
            } else if (program.target === 6) {
                value = program.initial[0] !== 0;
            } else {
                value = program.initialText;
            }
            const resolved = program.update(value);
            const output = resolved === undefined ? value : resolved;
            if (program.target === 7) {
                texts.push([program.object, String(output)]);
                continue;
            }
            const base = numericCount * 7;
            numeric[base] = program.object;
            numeric[base + 1] = program.target;
            numeric[base + 2] = 1;
            if (program.target <= 4) {
                numeric[base + 3] = Number(output.x);
                numeric[base + 4] = Number(output.y);
                numeric[base + 5] = Number(output.z);
            } else {
                numeric[base + 3] = program.target === 6 ? (output ? 1 : 0) : Number(output);
            }
            numericCount++;
        }
        return { numeric, numericCount, texts };
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneScriptFrameInput<'a> {
    pub scene_time_seconds: f64,
    pub frame_time_seconds: f64,
    pub dirty_events: SceneScriptSubscriptions,
    pub pointer: [f32; 2],
    pub audio_spectrum32: &'a [f32; 32],
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneScriptDelta {
    pub object: SceneObjectHandle,
    pub target: SceneScriptTarget,
    pub numeric: [f32; 4],
    pub text: Option<String>,
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

impl SceneScriptRuntime {
    pub fn from_storage(storage: &SceneStorage) -> Result<Option<Self>, SceneScriptError> {
        if storage.script_programs().is_empty() {
            return Ok(None);
        }
        let programs = storage
            .script_programs()
            .iter()
            .map(|record| SceneScriptProgram {
                record: *record,
                source: storage.string(record.source).unwrap_or_default().to_owned(),
                properties_json: storage
                    .string(record.properties_json)
                    .unwrap_or("{}")
                    .to_owned(),
                initial_text: storage
                    .string(record.initial_text)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect::<Vec<_>>();
        Self::new(&programs).map(Some)
    }

    pub fn new(programs: &[SceneScriptProgram]) -> Result<Self, SceneScriptError> {
        let runtime =
            Runtime::new().map_err(|error| SceneScriptError::CreateRuntime(error.to_string()))?;
        runtime.set_memory_limit(DEFAULT_MEMORY_LIMIT);
        runtime.set_max_stack_size(DEFAULT_STACK_LIMIT);
        runtime.set_gc_threshold(DEFAULT_GC_THRESHOLD);
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

    pub fn dispatch(
        &self,
        input: SceneScriptFrameInput<'_>,
    ) -> Result<Vec<SceneScriptDelta>, SceneScriptError> {
        self.deadline.set(Some(Instant::now() + FRAME_DEADLINE));
        let result = self.context.with(|ctx| {
            let dispatch: Function = ctx
                .globals()
                .get("__gilderDispatch")
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            let spectrum = TypedArray::<f32>::new_copy(ctx.clone(), input.audio_spectrum32)
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            let batch: Object = dispatch
                .call((
                    input.scene_time_seconds,
                    input.frame_time_seconds,
                    input.dirty_events.0,
                    input.pointer[0],
                    input.pointer[1],
                    spectrum,
                ))
                .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
            decode_batch(batch)
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
}

fn register_program<'js>(
    ctx: rquickjs::Ctx<'js>,
    module_index: usize,
    program: &SceneScriptProgram,
) -> Result<(), SceneScriptError> {
    let module_name = format!("gilder:scene-script/{module_index}");
    let module =
        Module::declare(ctx.clone(), module_name, program.source.as_bytes()).map_err(|error| {
            SceneScriptError::CompileModule {
                module: module_index,
                message: error.to_string(),
            }
        })?;
    let (module, evaluation) = module
        .eval()
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    evaluation
        .finish::<()>()
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })?;
    let namespace = module
        .namespace()
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
        .map_err(|error| SceneScriptError::CompileModule {
            module: module_index,
            message: error.to_string(),
        })
}

fn decode_batch(batch: Object<'_>) -> Result<Vec<SceneScriptDelta>, SceneScriptError> {
    let numeric: TypedArray<f64> = batch
        .get("numeric")
        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    let numeric_count: usize = batch
        .get("numericCount")
        .map_err(|error| SceneScriptError::Dispatch(error.to_string()))?;
    let values: &[f64] = numeric.as_ref();
    let mut deltas = Vec::with_capacity(numeric_count);
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
            numeric: [0.0; 4],
            text: Some(text),
        });
    }
    Ok(deltas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::abi::SceneStringId;

    fn program(
        target: SceneScriptTarget,
        subscriptions: SceneScriptSubscriptions,
        source: &str,
    ) -> SceneScriptProgram {
        SceneScriptProgram {
            record: SceneScriptProgramRecord {
                object: SceneObjectHandle(3),
                target,
                source: SceneStringId::NONE,
                properties_json: SceneStringId::NONE,
                initial_text: SceneStringId::NONE,
                subscriptions,
                initial_numeric: [10.0, 20.0, 30.0, 1.0],
            },
            source: source.to_owned(),
            properties_json: "{}".to_owned(),
            initial_text: "idle".to_owned(),
        }
    }

    fn input(events: SceneScriptSubscriptions) -> SceneScriptFrameInput<'static> {
        SceneScriptFrameInput {
            scene_time_seconds: 2.0,
            frame_time_seconds: 1.0 / 200.0,
            dirty_events: events,
            pointer: [0.25, 0.75],
            audio_spectrum32: &[0.5; 32],
        }
    }

    #[test]
    fn executes_es_module_and_returns_typed_vector_delta() {
        let runtime = SceneScriptRuntime::new(&[program(
            SceneScriptTarget::Origin,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { value.y += Math.sin(engine.runtime) * 4; return value; }",
        )])
        .expect("runtime");
        let deltas = runtime
            .dispatch(input(SceneScriptSubscriptions::FRAME))
            .expect("dispatch");
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].object, SceneObjectHandle(3));
        assert_eq!(deltas[0].target, SceneScriptTarget::Origin);
        assert!((deltas[0].numeric[1] - (20.0 + 2.0_f32.sin() * 4.0)).abs() < 0.0001);
    }

    #[test]
    fn event_mask_skips_unsubscribed_modules() {
        let runtime = SceneScriptRuntime::new(&[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::AUDIO,
            "export function update(value) { return value * 0.5; }",
        )])
        .expect("runtime");
        assert!(
            runtime
                .dispatch(input(SceneScriptSubscriptions::POINTER))
                .expect("dispatch")
                .is_empty()
        );
    }

    #[test]
    fn script_properties_and_text_results_survive_module_boundary() {
        let mut text = program(
            SceneScriptTarget::Text,
            SceneScriptSubscriptions::LOCAL_TIME,
            r#"export var scriptProperties = createScriptProperties()
                .addText({name: 'suffix', value: 'default'}).finish();
                export function update(value) { return `${value}:${scriptProperties.suffix}`; }"#,
        );
        text.properties_json = r#"{"suffix":{"value":"bound"}}"#.to_owned();
        let runtime = SceneScriptRuntime::new(&[text]).expect("runtime");
        let deltas = runtime
            .dispatch(input(SceneScriptSubscriptions::LOCAL_TIME))
            .expect("dispatch");
        assert_eq!(deltas[0].text.as_deref(), Some("idle:bound"));
    }

    #[test]
    fn runaway_script_is_interrupted() {
        let runtime = SceneScriptRuntime::new(&[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { while (true) {} }",
        )])
        .expect("runtime");
        assert_eq!(
            runtime.dispatch(input(SceneScriptSubscriptions::FRAME)),
            Err(SceneScriptError::DeadlineExceeded)
        );
    }
}
