use super::*;
use crate::engine::scene::abi::SceneStringId;
use crate::engine::scene::event::SceneEventSequence;
use crate::engine::scene::user_property::resolve_raw_scene_user_properties;

fn program(
    target: SceneScriptTarget,
    subscriptions: SceneScriptSubscriptions,
    source: &str,
) -> SceneScriptProgram {
    SceneScriptProgram {
        record: SceneScriptProgramRecord {
            object: SceneObjectHandle(3),
            target,
            selector: 0,
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
        pointer_clicks: &[],
        audio_spectrum: &StereoSpectrum64 {
            left: [0.25; 64],
            right: [0.75; 64],
        },
        media: None,
    }
}

fn dispatch(
    runtime: &SceneScriptRuntime,
    input: SceneScriptFrameInput<'_>,
) -> Result<Vec<SceneScriptDelta>, SceneScriptError> {
    let mut deltas = Vec::new();
    runtime.dispatch_into(input, &mut deltas)?;
    Ok(deltas)
}

#[test]
fn executes_es_module_and_returns_typed_vector_delta() {
    let runtime = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::Origin,
        SceneScriptSubscriptions::FRAME,
        "export function update(value) { value.y += Math.sin(engine.runtime) * 4; return value; }",
    )], &SceneScriptHostCatalog::empty())
    .expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].object, SceneObjectHandle(3));
    assert_eq!(deltas[0].target, SceneScriptTarget::Origin);
    assert!((deltas[0].numeric[1] - (20.0 + 2.0_f32.sin() * 4.0)).abs() < 0.0001);
}

#[test]
fn scalar_vector_result_expands_to_all_lanes() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Scale,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return 2.5; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].target, SceneScriptTarget::Scale);
    assert_eq!(&deltas[0].numeric[..3], &[2.5, 2.5, 2.5]);
}

#[test]
fn event_mask_publishes_initial_value_once_then_skips_unsubscribed_modules() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::AUDIO,
            "export function update(value) { return value * 0.5; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let first =
        dispatch(&runtime, input(SceneScriptSubscriptions::POINTER)).expect("initial dispatch");
    assert_eq!(first[0].numeric[0], 10.0);
    assert!(
        dispatch(&runtime, input(SceneScriptSubscriptions::POINTER))
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
    let runtime =
        SceneScriptRuntime::new(&[text], &SceneScriptHostCatalog::empty()).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::LOCAL_TIME)).expect("dispatch");
    assert_eq!(deltas[0].text.as_deref(), Some("idle:bound"));
}

#[test]
fn console_log_and_error_retain_a_bounded_diagnostic_ring() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { console.log(engine.frametime); console.error('frame', value); return value; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("console runtime");

    for _ in 0..300 {
        dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("console dispatch");
    }
    let retained = runtime.context.with(|ctx| {
        ctx.globals()
            .get::<_, u32>("__tensor_wallpaperConsoleRetainedCount")
            .expect("retained console count")
    });
    assert_eq!(retained, 256);
}

#[test]
fn runtime_clock_text_produces_distinct_retained_values() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Text,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return Math.floor(engine.runtime).toString().padStart(2, '0'); }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let first = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("first clock");
    let mut next = input(SceneScriptSubscriptions::FRAME);
    next.scene_time_seconds = 3.0;
    let second = dispatch(&runtime, next).expect("second clock");
    assert_eq!(first[0].text.as_deref(), Some("02"));
    assert_eq!(second[0].text.as_deref(), Some("03"));
}

#[test]
fn local_storage_retains_values_and_separates_screen_from_global_scope() {
    let programs = [
        program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            r#"export function init(value) {
                localStorage.set('position', {x: 12, y: 34, z: 0});
                localStorage.set('position', {x: 56, y: 78, z: 0}, localStorage.LOCATION_GLOBAL);
                return value;
            }"#,
        ),
        program(
            SceneScriptTarget::Origin,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return localStorage.get('position'); }",
        ),
        program(
            SceneScriptTarget::Origin,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return localStorage.get('position', localStorage.LOCATION_GLOBAL); }",
        ),
    ];
    let runtime =
        SceneScriptRuntime::new(&programs, &SceneScriptHostCatalog::empty()).expect("runtime");

    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");

    assert_eq!(&deltas[1].numeric[..3], &[12.0, 34.0, 0.0]);
    assert_eq!(&deltas[2].numeric[..3], &[56.0, 78.0, 0.0]);
}

#[test]
fn local_storage_rejects_non_string_keys_and_unknown_locations() {
    for source in [
        "export function init() { localStorage.get(1); }",
        "export function init() { localStorage.clear('instance'); }",
    ] {
        let error = SceneScriptRuntime::new(
            &[program(
                SceneScriptTarget::Alpha,
                SceneScriptSubscriptions::FRAME,
                source,
            )],
            &SceneScriptHostCatalog::empty(),
        )
        .expect_err("invalid localStorage access must fail");
        assert!(error.to_string().contains("localStorage"));
    }
}

#[test]
fn vec3_initial_values_and_operations_match_scenescript_value_semantics() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Origin,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { return value.add(new Vec3(2, 4, 6)).mix(new Vec3(20, 30, 40), 0.5); }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");

    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");

    assert_eq!(&deltas[0].numeric[..3], &[16.0, 27.0, 38.0]);
}

#[test]
fn user_property_resolution_is_exact_and_type_strict() {
    let authored = r#"{"jia":{"value":true},"speed":{"value":1}}"#;
    let overrides = [("jia".to_owned(), Value::Bool(false))]
        .into_iter()
        .collect();
    let resolved = resolve_raw_scene_user_properties(authored, &overrides).expect("properties");
    assert_eq!(resolved["jia"], Value::Bool(false));
    assert_eq!(resolved["speed"], Value::from(1));

    for invalid in [
        [("Jia".to_owned(), Value::Bool(false))]
            .into_iter()
            .collect(),
        [("jia".to_owned(), Value::from(0))].into_iter().collect(),
    ] {
        assert!(resolve_raw_scene_user_properties(authored, &invalid).is_err());
    }
    assert!(
        resolve_raw_scene_user_properties(r#"{"group":{}}"#, &Map::new())
            .expect("non-runtime project entry")
            .is_empty()
    );
    let invalid_group = [("group".to_owned(), Value::Null)].into_iter().collect();
    assert!(resolve_raw_scene_user_properties(r#"{"group":{}}"#, &invalid_group).is_err());
    assert!(resolve_raw_scene_user_properties(r#"{"jia":true}"#, &Map::new()).is_err());
}

#[test]
fn user_property_override_updates_exact_script_property_binding() {
    let mut text = program(
        SceneScriptTarget::Text,
        SceneScriptSubscriptions::FRAME,
        r#"export var scriptProperties = createScriptProperties()
            .addCheckbox({name: 'enabled', value: true}).finish();
            export function update() { return String(scriptProperties.enabled); }"#,
    );
    text.properties_json = r#"{"enabled":{"user":"jia","value":true}}"#.to_owned();
    let host = SceneScriptHostCatalog {
        layers: SceneScriptHostCatalog::empty().layers,
        effect_count: 0,
        canvas_size: [1920, 1080],
        user_properties: [("jia".to_owned(), Value::Bool(false))]
            .into_iter()
            .collect(),
    };
    let runtime = SceneScriptRuntime::new(&[text], &host).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].text.as_deref(), Some("false"));
}

#[test]
fn sound_layer_init_precedes_initial_user_properties_and_controls_playback_state() {
    let host = SceneScriptHostCatalog {
        layers: vec![
            SceneScriptLayerCatalog::test(0, 3, "controller"),
            SceneScriptLayerCatalog {
                sound: true,
                ..SceneScriptLayerCatalog::test(1, 4, "song")
            },
        ],
        effect_count: 0,
        canvas_size: [1920, 1080],
        user_properties: [("music".to_owned(), Value::String("1".to_owned()))]
            .into_iter()
            .collect(),
    };
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            r#"let song = 'song';
                export function init() {
                    song = thisScene.getLayer(song);
                    song.stop();
                }
                export function applyUserProperties(properties) {
                    if (properties.music === '1' && !song.isPlaying()) song.play();
                }
                export function update() { return song.isPlaying() ? 1 : 0; }"#,
        )],
        &host,
    )
    .expect("sound controller runtime");

    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 1.0);
}

#[test]
fn non_sound_layers_do_not_expose_sound_playback_methods() {
    let error = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "export function init() { thisLayer.play(); }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect_err("image layer playback must remain a strict host miss");
    assert!(error.to_string().contains("not a function"));
}

#[test]
fn authored_font_assignment_must_match_the_v21_baked_text_resource() {
    let host = SceneScriptHostCatalog {
        layers: vec![SceneScriptLayerCatalog {
            text: true,
            font: Some("fonts/authored.ttf".to_owned()),
            ..SceneScriptLayerCatalog::test(0, 3, "Title")
        }],
        effect_count: 0,
        canvas_size: [1920, 1080],
        user_properties: Map::new(),
    };
    let matching = program(
        SceneScriptTarget::Visible,
        SceneScriptSubscriptions::USER_PROPERTY,
        r#"const font = engine.registerAsset('fonts/authored.ttf');
            export function applyUserProperties() {
                thisLayer.font = font;
                thisScene.getLayer('Title').font = font;
            }"#,
    );
    SceneScriptRuntime::new(&[matching], &host).expect("matching baked font");

    let different = program(
        SceneScriptTarget::Visible,
        SceneScriptSubscriptions::USER_PROPERTY,
        r#"const font = engine.registerAsset('fonts/different.ttf');
            export function applyUserProperties() { thisLayer.font = font; }"#,
    );
    let error = SceneScriptRuntime::new(&[different], &host).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("is not baked into this v21 artifact")
    );
}

#[test]
fn runaway_script_is_interrupted() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { while (true) {} }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    assert_eq!(
        dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)),
        Err(SceneScriptError::DeadlineExceeded)
    );
}

#[test]
fn init_runs_once_and_retained_value_advances_without_rust_allocation_contract() {
    let runtime = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::Alpha,
        SceneScriptSubscriptions::FRAME,
        "let calls = 0; export function init(value) { return value + 2; } export function update(value) { calls++; return value + calls; }",
    )], &SceneScriptHostCatalog::empty())
    .expect("runtime");
    let mut deltas = Vec::with_capacity(1);
    runtime
        .dispatch_into(input(SceneScriptSubscriptions::FRAME), &mut deltas)
        .expect("first dispatch");
    assert_eq!(deltas[0].numeric[0], 13.0);
    runtime
        .dispatch_into(input(SceneScriptSubscriptions::FRAME), &mut deltas)
        .expect("second dispatch");
    assert_eq!(deltas[0].numeric[0], 15.0);
    assert_eq!(deltas.capacity(), 1);
}

#[test]
fn empty_pointer_click_frames_do_not_allocate_js_arrays() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "export function update(value) { return value; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let mut deltas = Vec::with_capacity(1);
    runtime
        .dispatch_into(input(SceneScriptSubscriptions::FRAME), &mut deltas)
        .expect("initial dispatch");
    let baseline = runtime.memory_snapshot();
    for _ in 0..256 {
        runtime
            .dispatch_into(input(SceneScriptSubscriptions::FRAME), &mut deltas)
            .expect("retained dispatch");
    }
    let retained = runtime.memory_snapshot();
    assert_eq!(retained.object_count, baseline.object_count);
}

#[test]
fn dynamic_text_frames_do_not_retain_js_strings_or_arrays() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Text,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return `clock:${engine.runtime.toFixed(3)}`; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let mut deltas = Vec::with_capacity(1);
    runtime
        .dispatch_into(input(SceneScriptSubscriptions::FRAME), &mut deltas)
        .expect("initial dispatch");
    runtime.run_gc();
    let baseline = runtime.memory_snapshot();
    for frame in 0..16_384 {
        let mut frame_input = input(SceneScriptSubscriptions::FRAME);
        frame_input.scene_time_seconds = f64::from(frame) / 240.0;
        runtime
            .dispatch_into(frame_input, &mut deltas)
            .expect("retained dynamic-text dispatch");
    }
    runtime.run_gc();
    let retained = runtime.memory_snapshot();
    assert_eq!(retained.object_count, baseline.object_count);
    assert_eq!(retained.string_count, baseline.string_count);
    assert_eq!(retained.allocation_count, baseline.allocation_count);
}

#[test]
fn builtin_we_math_module_is_shared_by_authored_modules() {
    let programs = [
        program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "import * as WEMath from 'WEMath'; export function update(value) { return WEMath.clamp(value, 0, 1); }",
        ),
        program(
            SceneScriptTarget::Alpha,
            SceneScriptSubscriptions::FRAME,
            "import { mix, smoothStep } from 'WEMath'; export function update() { return mix(0, 1, smoothStep(0, 1, 0.5)); }",
        ),
    ];
    let runtime =
        SceneScriptRuntime::new(&programs, &SceneScriptHostCatalog::empty()).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 1.0);
    assert_eq!(deltas[1].numeric[0], 0.5);
}

#[test]
fn media_callback_runs_before_update_on_media_event() {
    let runtime = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::Alpha,
        SceneScriptSubscriptions::FRAME.union(SceneScriptSubscriptions::MEDIA),
        "let state = 0; export function mediaPlaybackChanged(event) { state = event.state; } export function update() { return state; }",
    )], &SceneScriptHostCatalog::empty())
    .expect("runtime");
    let mut frame = input(SceneScriptSubscriptions::MEDIA);
    frame.media = Some(SceneMediaClockState {
        sequence: SceneEventSequence(1),
        playback: SceneMediaPlaybackState::Playing,
        clock_ns: 5_000_000_000,
        duration_ns: Some(60_000_000_000),
        ..SceneMediaClockState::default()
    });
    let deltas = dispatch(&runtime, frame).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 1.0);
}

#[test]
fn audio_spectrum_drives_typed_effect_target_through_quickjs() {
    let runtime = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::TechCircleSectorWidth,
        SceneScriptSubscriptions::FRAME.union(SceneScriptSubscriptions::AUDIO),
        "const audio = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_32); export function update(value) { return value + audio.average[0]; }",
    )], &SceneScriptHostCatalog::empty())
    .expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::AUDIO)).expect("dispatch");
    assert_eq!(deltas[0].target, SceneScriptTarget::TechCircleSectorWidth);
    assert_eq!(deltas[0].numeric[0], 10.5);
}

#[test]
fn audio_buffers_keep_stereo_channels_and_derive_each_resolution() {
    let runtime = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::Alpha,
        SceneScriptSubscriptions::AUDIO,
        "const a16 = engine.registerAudioBuffers(); const a32 = engine.registerAudioBuffers(32); const a64 = engine.registerAudioBuffers(64); export function update() { return a64.left[2] + a64.right[2] * 10 + a32.left[1] * 100 + a16.right[0] * 1000; }",
    )], &SceneScriptHostCatalog::empty())
    .expect("runtime");
    let spectrum = StereoSpectrum64 {
        left: std::array::from_fn(|band| band as f32),
        right: std::array::from_fn(|band| (band * 2) as f32),
    };
    let frame = SceneScriptFrameInput {
        audio_spectrum: &spectrum,
        ..input(SceneScriptSubscriptions::AUDIO)
    };
    let deltas = dispatch(&runtime, frame).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 6_342.0);
}

#[test]
fn register_audio_buffers_rejects_noncanonical_resolution() {
    let error = SceneScriptRuntime::new(&[program(
        SceneScriptTarget::Alpha,
        SceneScriptSubscriptions::AUDIO,
        "const audio = engine.registerAudioBuffers(24); export function update() { return audio.average[0]; }",
    )], &SceneScriptHostCatalog::empty())
    .expect_err("resolution 24 must be rejected");
    assert!(
        error
            .to_string()
            .contains("Resolution must be either 16, 32 or 64")
    );
}

#[test]
fn material_scalar_delta_preserves_the_typed_constant_selector() {
    let mut material = program(
        SceneScriptTarget::MaterialScalar,
        SceneScriptSubscriptions::FRAME,
        "export function update(value) { return value + 2; }",
    );
    material.record.selector = 41;
    let runtime =
        SceneScriptRuntime::new(&[material], &SceneScriptHostCatalog::empty()).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].target, SceneScriptTarget::MaterialScalar);
    assert_eq!(deltas[0].selector, 41);
    assert_eq!(deltas[0].numeric[0], 12.0);
}

#[test]
fn layer_parent_scene_queries_and_cross_layer_writes_use_typed_deltas() {
    let mut child = SceneScriptLayerCatalog::test(0, 3, "child");
    child.parent = Some(12);
    let host = SceneScriptHostCatalog {
        layers: vec![child, SceneScriptLayerCatalog::test(1, 12, "parent")],
        effect_count: 0,
        canvas_size: [2560, 1440],
        user_properties: Map::new(),
    };
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Visible,
            SceneScriptSubscriptions::FRAME,
            r#"export function init(value) {
                const parent = thisLayer.getParent();
                if (parent !== thisScene.getLayer('parent')) throw new Error('wrong parent');
                if (thisScene.getLayerCount() !== 2) throw new Error('wrong layer count');
                if (thisScene.getLayerIndex(parent) !== 1) throw new Error('wrong layer index');
                if (engine.canvasSize.x !== 2560 || engine.canvasSize.y !== 1440) throw new Error('wrong canvas');
                Object.defineProperty(thisLayer, 'held', {writable: true, value: false});
                thisLayer.held = true;
                parent.origin = new Vec3(10, 20, 30);
                parent.scale = new Vec3(2, 3, 4);
                parent.visible = false;
                return value && thisLayer.held;
            }"#,
        )],
        &host,
    )
    .expect("runtime");

    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    let find = |target| {
        deltas
            .iter()
            .find(|delta| delta.object == SceneObjectHandle(12) && delta.target == target)
            .expect("cross-layer delta")
    };
    assert_eq!(
        &find(SceneScriptTarget::Origin).numeric[..3],
        &[10.0, 20.0, 30.0]
    );
    assert_eq!(
        &find(SceneScriptTarget::Scale).numeric[..3],
        &[2.0, 3.0, 4.0]
    );
    assert_eq!(find(SceneScriptTarget::Visible).numeric[0], 0.0);
}

#[test]
fn unparented_layer_returns_undefined_and_direct_writes_reject_wrong_types() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Visible,
            SceneScriptSubscriptions::FRAME,
            "export function update() { return thisLayer.getParent() === undefined; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 1.0);

    let error = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Visible,
            SceneScriptSubscriptions::FRAME,
            "export function init(value) { thisLayer.origin = 12; return value; }",
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect_err("invalid direct layer vector assignment must fail");
    assert!(error.to_string().contains("origin requires a Vec3"));
}

#[test]
fn visible_property_init_receives_numeric_scalar_before_truthiness_coercion() {
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Visible,
            SceneScriptSubscriptions::FRAME,
            r#"let initialValue;
                export function init(value) {
                    initialValue = typeof value === 'number' ? value : value.x;
                }
                export function update() { return initialValue * 1.5; }"#,
        )],
        &SceneScriptHostCatalog::empty(),
    )
    .expect("runtime");

    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].target, SceneScriptTarget::Visible);
    assert_eq!(deltas[0].numeric[0], 1.0);
}

#[test]
fn scene_effect_visibility_uses_typed_binding_selectors_and_targeted_clicks() {
    let host = SceneScriptHostCatalog {
        layers: vec![
            SceneScriptLayerCatalog::test(0, 3, "controller"),
            SceneScriptLayerCatalog {
                effects: vec![
                    SceneScriptEffectCatalog {
                        index: 0,
                        binding: 0,
                        name: "盔甲".to_owned(),
                        visible: true,
                    },
                    SceneScriptEffectCatalog {
                        index: 1,
                        binding: 1,
                        name: "盔甲水波".to_owned(),
                        visible: true,
                    },
                ],
                ..SceneScriptLayerCatalog::test(1, 12, "身体")
            },
        ],
        effect_count: 2,
        canvas_size: [1920, 1080],
        user_properties: [("jia".to_owned(), Value::Bool(true))]
            .into_iter()
            .collect(),
    };
    let source = r#"
        export function update(value) {
            thisScene.getLayer('身体').getEffect('盔甲').visible = shared.yi;
            thisScene.getLayer('身体').getEffect('盔甲水波').visible = shared.yi;
            return value;
        }
        export function cursorClick(event) { shared.yi = !shared.yi; }
        export function applyUserProperties(properties) { shared.yi = properties.jia; }
    "#;
    let runtime = SceneScriptRuntime::new(
        &[program(
            SceneScriptTarget::Visible,
            SceneScriptSubscriptions::FRAME
                .union(SceneScriptSubscriptions::USER_PROPERTY)
                .union(SceneScriptSubscriptions::POINTER_CLICK),
            source,
        )],
        &host,
    )
    .expect("runtime");

    let initial =
        dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("initial dispatch");
    assert_eq!(initial.len(), 3);
    assert_eq!(initial[1].object, SceneObjectHandle(12));
    assert_eq!(initial[1].target, SceneScriptTarget::EffectVisible);
    assert_eq!(initial[1].selector, 0);
    assert_eq!(initial[1].numeric[0], 1.0);
    assert_eq!(initial[2].selector, 1);
    assert_eq!(initial[2].numeric[0], 1.0);

    let click = [SceneScriptPointerClick {
        object: SceneObjectHandle(3),
        button: 0x110,
        pointer: [0.5, 0.5],
    }];
    let mut frame =
        input(SceneScriptSubscriptions::FRAME.union(SceneScriptSubscriptions::POINTER_CLICK));
    frame.pointer_clicks = &click;
    let clicked = dispatch(&runtime, frame).expect("click dispatch");
    assert_eq!(clicked[1].target, SceneScriptTarget::EffectVisible);
    assert_eq!(clicked[1].numeric[0], 0.0);
    assert_eq!(clicked[2].numeric[0], 0.0);
}
