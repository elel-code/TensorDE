use std::fs;

use super::*;

#[test]
fn specializes_virtual_includes_conditions_and_function_macros() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-package-preprocess-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("shaders")).unwrap();
    fs::write(
        root.join("shaders/common.h"),
        "#define SQUARE(x) ((x) * (x))\n#if HLSL\n#define INCLUDED 3\n#endif\n",
    )
    .unwrap();
    let source = WeAssetSource::open(root.clone()).unwrap();
    let spec = AuthoredProgramSpec {
        program_key: "package/test".to_owned(),
        source_key: "effects/test".to_owned(),
        texture_slot_mask: 0,
    };

    let specialized = specialize_stage(
        &source,
        &spec,
        ShaderStage::Fragment,
        "#include \"common.h\"\n#if MODE == 2\nfloat value = SQUARE(INCLUDED);\n#else\nfloat bad = 0.0;\n#endif\nfloat multiline = SQUARE(\nINCLUDED\n);",
        &[
            ("HLSL".to_owned(), "1".to_owned()),
            ("MODE".to_owned(), "2".to_owned()),
        ],
    )
    .unwrap();

    assert!(specialized.contains("float value = ((3) * (3));"));
    assert!(specialized.contains("float multiline = ((3) * (3));"));
    assert!(!specialized.contains("float bad"));
    assert!(!specialized.contains('#'));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn specialization_retains_only_the_active_uniform_metadata_annotation() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-package-preprocess-metadata-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let source = WeAssetSource::open(root.clone()).unwrap();
    let spec = AuthoredProgramSpec {
        program_key: "package/test".to_owned(),
        source_key: "effects/test".to_owned(),
        texture_slot_mask: 0,
    };
    let specialized = specialize_stage(
        &source,
        &spec,
        ShaderStage::Fragment,
        r#"
#if CUSTOM
uniform float u_value; // {"material":"custom","default":2}
#else
uniform float u_value; // {"material":"default","default":1}
#endif
"#,
        &[("CUSTOM".to_owned(), "1".to_owned())],
    )
    .expect("specialized source");
    let metadata = super::super::uniform_metadata::parse_shader_uniform_metadata(&specialized)
        .expect("active metadata");

    assert_eq!(metadata.material_parameter("u_value"), Some("custom"));
    assert_eq!(metadata.material_default("u_value"), Some("2"));
    assert!(!specialized.contains("\"default\",\"default\":1"));
    let compiler_source = strip_specialized_stage_comments(&specialized).expect("compiler view");
    assert!(compiler_source.contains("uniform float u_value;"));
    assert!(!compiler_source.contains("material"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn evaluates_combo_condition_precedence_and_undefined_identifiers() {
    let mut macros = BTreeMap::new();
    macros.insert("A".to_owned(), MacroDefinition::Object("2".to_owned()));
    macros.insert("B".to_owned(), MacroDefinition::Object("4".to_owned()));

    assert_eq!(
        evaluate_expression("A + B == 6 && !MISSING", &macros).unwrap(),
        1
    );
    assert_eq!(
        evaluate_expression("defined(A) && !defined(MISSING)", &macros).unwrap(),
        1
    );
}

#[test]
fn prunes_only_same_block_returns_left_unreachable_by_specialization() {
    let lowered = prune_unreachable_direct_returns(
        "float selected() {\n    return 7.0;\n    return 0.0;\n}\nfloat conditional(bool enabled) {\n    if (enabled) {\n        return 1.0;\n    }\n    return 0.0;\n}"
            .to_owned(),
    );

    assert!(lowered.contains("return 7.0;"));
    assert!(!lowered.contains("return 0.0;\n}\nfloat conditional"));
    assert!(lowered.contains("if (enabled) {\n        return 1.0;\n    }\n    return 0.0;"));
}

#[test]
fn treats_we_stray_endif_as_a_no_op_before_later_conditionals() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-package-preprocess-stray-endif-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let source = WeAssetSource::open(root.clone()).unwrap();
    let spec = AuthoredProgramSpec {
        program_key: "package/test".to_owned(),
        source_key: "effects/test".to_owned(),
        texture_slot_mask: 0,
    };

    let specialized = specialize_stage(
        &source,
        &spec,
        ShaderStage::Vertex,
        "#if FIRST\nfloat first = 1.0;\n#endif\n#endif\n#if SECOND\nfloat second = 2.0;\n#endif",
        &[
            ("FIRST".to_owned(), "1".to_owned()),
            ("SECOND".to_owned(), "1".to_owned()),
        ],
    )
    .expect("WE stray #endif is a defined no-op");

    assert!(specialized.contains("float first = 1.0;"));
    assert!(specialized.contains("float second = 2.0;"));
    assert!(!specialized.contains('#'));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_other_unbalanced_conditional_directives() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-package-preprocess-unbalanced-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let source = WeAssetSource::open(root.clone()).unwrap();
    let spec = AuthoredProgramSpec {
        program_key: "package/test".to_owned(),
        source_key: "effects/test".to_owned(),
        texture_slot_mask: 0,
    };

    for (stage_source, expected) in [
        ("#else", "#else has no matching #if"),
        ("#elif 1", "#elif has no matching #if"),
        (
            "#if 1\nfloat value = 1.0;",
            "unterminated conditional compilation block",
        ),
    ] {
        let error = specialize_stage(&source, &spec, ShaderStage::Vertex, stage_source, &[])
            .expect_err("unbalanced conditional must fail strictly")
            .to_string();
        assert!(error.contains(expected), "unexpected error: {error}");
    }
    fs::remove_dir_all(root).unwrap();
}
