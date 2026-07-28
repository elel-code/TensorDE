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
        audio_spectrum32: &[0.5; 32],
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
        user_properties: [("jia".to_owned(), Value::Bool(false))]
            .into_iter()
            .collect(),
    };
    let runtime = SceneScriptRuntime::new(&[text], &host).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].text.as_deref(), Some("false"));
}

#[test]
fn authored_font_assignment_must_match_the_v21_baked_text_resource() {
    let host = SceneScriptHostCatalog {
        layers: vec![SceneScriptLayerCatalog {
            index: 0,
            object: 3,
            name: "Title".to_owned(),
            text: true,
            font: Some("fonts/authored.ttf".to_owned()),
            effects: Vec::new(),
        }],
        effect_count: 0,
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
            "import { mix } from 'WEMath'; export function update() { return mix(0, 1, 0.25); }",
        ),
    ];
    let runtime =
        SceneScriptRuntime::new(&programs, &SceneScriptHostCatalog::empty()).expect("runtime");
    let deltas = dispatch(&runtime, input(SceneScriptSubscriptions::FRAME)).expect("dispatch");
    assert_eq!(deltas[0].numeric[0], 1.0);
    assert_eq!(deltas[1].numeric[0], 0.25);
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
fn scene_effect_visibility_uses_typed_binding_selectors_and_targeted_clicks() {
    let host = SceneScriptHostCatalog {
        layers: vec![
            SceneScriptLayerCatalog {
                index: 0,
                object: 3,
                name: "controller".to_owned(),
                text: false,
                font: None,
                effects: Vec::new(),
            },
            SceneScriptLayerCatalog {
                index: 1,
                object: 12,
                name: "身体".to_owned(),
                text: false,
                font: None,
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
            },
        ],
        effect_count: 2,
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
