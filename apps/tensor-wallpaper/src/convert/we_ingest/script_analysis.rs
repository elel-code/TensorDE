//! Oxc-based cold-path analysis of authored SceneScript modules.

use std::fmt;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentExpression, CallExpression, Expression, IdentifierReference,
    MemberExpression, NewExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SceneScriptAnalysis {
    pub(super) exports_update: bool,
    pub(super) exports_init: bool,
    pub(super) handles_media: bool,
    pub(super) handles_user_properties: bool,
    pub(super) handles_pointer_click: bool,
    pub(super) uses_runtime: bool,
    pub(super) uses_frame_time: bool,
    pub(super) uses_audio: bool,
    pub(super) uses_pointer: bool,
    pub(super) uses_local_time: bool,
    pub(super) uses_local_time_seconds: bool,
    pub(super) uses_local_time_subseconds: bool,
    pub(super) uses_scene_api: bool,
    pub(super) may_mutate_effect_visibility: bool,
    pub(super) has_unresolved_effect_visibility_target: bool,
    pub(super) effect_visibility_targets: Vec<SceneEffectVisibilityTarget>,
    pub(super) may_assign_font: bool,
    pub(super) imports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SceneEffectVisibilityTarget {
    pub(super) layer_name: String,
    pub(super) effect_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneScriptParseError {
    diagnostics: Vec<String>,
}

impl fmt::Display for SceneScriptParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} JavaScript parse diagnostic(s)",
            self.diagnostics.len()
        )?;
        for diagnostic in &self.diagnostics {
            write!(formatter, ": {diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SceneScriptParseError {}

pub(super) fn analyze_scene_script(
    source: &str,
) -> Result<SceneScriptAnalysis, SceneScriptParseError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(SceneScriptParseError {
            diagnostics: parsed
                .diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.to_string())
                .collect(),
        });
    }
    let exported = |name: &str| {
        parsed
            .module_record
            .exported_bindings
            .keys()
            .any(|binding| binding.as_str() == name)
    };
    let mut visitor = CapabilityVisitor::default();
    visitor.visit_program(&parsed.program);
    Ok(SceneScriptAnalysis {
        exports_update: exported("update"),
        exports_init: exported("init"),
        handles_media: exported("mediaPlaybackChanged")
            || exported("mediaTimelineChanged")
            || exported("mediaPropertiesChanged"),
        handles_user_properties: exported("applyUserProperties"),
        handles_pointer_click: exported("cursorClick"),
        uses_runtime: visitor.uses_runtime,
        uses_frame_time: visitor.uses_frame_time,
        uses_audio: visitor.uses_audio,
        uses_pointer: visitor.uses_pointer,
        uses_local_time: visitor.uses_local_time,
        uses_local_time_seconds: visitor.uses_local_time_seconds,
        uses_local_time_subseconds: visitor.uses_local_time_subseconds,
        uses_scene_api: visitor.uses_scene_api,
        may_mutate_effect_visibility: visitor.may_mutate_effect_visibility,
        has_unresolved_effect_visibility_target: visitor.has_unresolved_effect_visibility_target,
        effect_visibility_targets: visitor.effect_visibility_targets,
        may_assign_font: visitor.assigns_font,
        imports: parsed
            .module_record
            .requested_modules
            .keys()
            .map(ToString::to_string)
            .collect(),
    })
}

#[derive(Debug, Default)]
struct CapabilityVisitor {
    uses_runtime: bool,
    uses_frame_time: bool,
    uses_audio: bool,
    uses_pointer: bool,
    uses_local_time: bool,
    uses_local_time_seconds: bool,
    uses_local_time_subseconds: bool,
    uses_scene_api: bool,
    may_mutate_effect_visibility: bool,
    has_unresolved_effect_visibility_target: bool,
    effect_visibility_targets: Vec<SceneEffectVisibilityTarget>,
    assigns_font: bool,
}

impl<'a> Visit<'a> for CapabilityVisitor {
    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if identifier.name == "thisScene" {
            self.uses_scene_api = true;
        }
    }

    fn visit_member_expression(&mut self, expression: &MemberExpression<'a>) {
        if expression.is_specific_member_access("engine", "runtime") {
            self.uses_runtime = true;
        }
        if expression.is_specific_member_access("engine", "frametime") {
            self.uses_frame_time = true;
        }
        if expression.is_specific_member_access("Date", "now") {
            self.uses_local_time = true;
            self.uses_local_time_subseconds = true;
        }
        if expression.static_property_name().is_some_and(|property| {
            matches!(
                property,
                "pointer" | "cursor" | "mousePosition" | "cursorPosition" | "cursorWorldPosition"
            )
        }) {
            self.uses_pointer = true;
        }
        walk::walk_member_expression(self, expression);
    }

    fn visit_assignment_expression(&mut self, expression: &AssignmentExpression<'a>) {
        if let Some(target) = expression.left.as_member_expression() {
            if target.static_property_name() == Some("font") {
                self.assigns_font = true;
            }
            if target.static_property_name() == Some("visible")
                && matches!(target.object(), Expression::CallExpression(call) if call.callee_name() == Some("getEffect"))
            {
                self.may_mutate_effect_visibility = true;
                if let Some(target) = literal_effect_visibility_target(target.object()) {
                    self.effect_visibility_targets.push(target);
                } else {
                    self.has_unresolved_effect_visibility_target = true;
                }
            }
        }
        walk::walk_assignment_expression(self, expression);
    }

    fn visit_call_expression(&mut self, expression: &CallExpression<'a>) {
        match expression.callee_name() {
            Some("registerAudioBuffers") => self.uses_audio = true,
            Some("getMilliseconds") => {
                self.uses_local_time = true;
                self.uses_local_time_subseconds = true;
            }
            Some("getSeconds") => {
                self.uses_local_time = true;
                self.uses_local_time_seconds = true;
            }
            Some("getDate" | "getDay" | "getFullYear" | "getHours" | "getMinutes" | "getMonth") => {
                self.uses_local_time = true
            }
            _ => {}
        }
        walk::walk_call_expression(self, expression);
    }

    fn visit_new_expression(&mut self, expression: &NewExpression<'a>) {
        if expression.callee.is_specific_id("Date") {
            self.uses_local_time = true;
        }
        walk::walk_new_expression(self, expression);
    }
}

fn literal_effect_visibility_target(
    expression: &Expression<'_>,
) -> Option<SceneEffectVisibilityTarget> {
    let Expression::CallExpression(effect_call) = expression else {
        return None;
    };
    if effect_call.callee_name() != Some("getEffect") {
        return None;
    }
    let effect_name = literal_string_argument(effect_call)?;
    let effect_callee = effect_call.callee.as_member_expression()?;
    let Expression::CallExpression(layer_call) = effect_callee.object() else {
        return None;
    };
    if layer_call.callee_name() != Some("getLayer") {
        return None;
    }
    let layer_name = literal_string_argument(layer_call)?;
    Some(SceneEffectVisibilityTarget {
        layer_name: layer_name.to_owned(),
        effect_name: effect_name.to_owned(),
    })
}

fn literal_string_argument<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    if call.arguments.len() != 1 {
        return None;
    }
    match call.arguments.first()? {
        Argument::StringLiteral(literal) => Some(literal.value.as_str()),
        Argument::TemplateLiteral(literal) if literal.expressions.is_empty() => literal
            .quasis
            .first()?
            .value
            .cooked
            .map(|value| value.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_exports_imports_and_runtime_capabilities_from_ast() {
        let analysis = analyze_scene_script(
            r#"
                import { Vec3 } from './math.js';
                const audio = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_32);
                export function update(value) {
                    value.x += Math.sin(engine.runtime) + engine.frametime;
                    return value;
                }
            "#,
        )
        .expect("analysis");
        assert!(analysis.exports_update);
        assert!(analysis.uses_runtime);
        assert!(analysis.uses_frame_time);
        assert!(analysis.uses_audio);
        assert_eq!(analysis.imports, ["./math.js"]);
    }

    #[test]
    fn comments_and_string_literals_do_not_create_false_capabilities() {
        let analysis = analyze_scene_script(
            r#"
                // engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_32); thisScene.getLayer('x')
                const note = "new Date(); engine.runtime; thisScene['getLayer']('x')";
                export function update(value) { return value; }
            "#,
        )
        .expect("analysis");
        assert!(!analysis.uses_audio);
        assert!(!analysis.uses_runtime);
        assert!(!analysis.uses_local_time);
        assert!(!analysis.uses_scene_api);
    }

    #[test]
    fn invalid_javascript_is_rejected_before_gscene_emission() {
        assert!(analyze_scene_script("export function update( {").is_err());
    }

    #[test]
    fn all_media_exports_and_world_cursor_are_capabilities() {
        let analysis = analyze_scene_script(
            "export function mediaTimelineChanged(event) {} export function update(value) { return input.cursorWorldPosition.x + value; }",
        )
        .expect("analysis");
        assert!(analysis.handles_media);
        assert!(analysis.uses_pointer);
    }

    #[test]
    fn detects_scene_mutation_api_even_through_computed_members() {
        let analysis = analyze_scene_script(
            "export function update(value) { thisScene[key]('body').visible = value; return value; }",
        )
        .expect("analysis");
        assert!(analysis.uses_scene_api);
    }

    #[test]
    fn detects_effect_visibility_mutation_without_matching_comments_or_strings() {
        let mutation = analyze_scene_script(
            "export function update(value) { thisScene.getLayer('body').getEffect('shine').visible = value; return value; }",
        )
        .expect("analysis");
        assert!(mutation.may_mutate_effect_visibility);
        assert!(!mutation.has_unresolved_effect_visibility_target);
        assert_eq!(
            mutation.effect_visibility_targets,
            [SceneEffectVisibilityTarget {
                layer_name: "body".to_owned(),
                effect_name: "shine".to_owned(),
            }]
        );

        let inert = analyze_scene_script(
            r#"const note = "getEffect('shine').visible = false"; export function update(value) { return value; }"#,
        )
        .expect("analysis");
        assert!(!inert.may_mutate_effect_visibility);
    }

    #[test]
    fn dynamic_effect_visibility_selector_requires_conservative_addressability() {
        let analysis = analyze_scene_script(
            "export function update(value) { thisScene.getLayer(layerName).getEffect('shine').visible = value; return value; }",
        )
        .expect("analysis");
        assert!(analysis.may_mutate_effect_visibility);
        assert!(analysis.has_unresolved_effect_visibility_target);
        assert!(analysis.effect_visibility_targets.is_empty());
    }

    #[test]
    fn distinguishes_font_assignments_from_other_user_property_callbacks() {
        let font = analyze_scene_script(
            "export function applyUserProperties() { thisScene.getLayer('title').font = engine.registerAsset('fonts/title.ttf'); }",
        )
        .expect("font callback");
        assert!(font.handles_user_properties);
        assert!(font.may_assign_font);

        let sound = analyze_scene_script(
            "export function applyUserProperties() { thisScene.getLayer('music').stop(); }",
        )
        .expect("sound callback");
        assert!(sound.handles_user_properties);
        assert!(!sound.may_assign_font);
    }

    #[test]
    fn detects_cursor_click_as_a_typed_export() {
        let analysis = analyze_scene_script(
            "export function cursorClick(event) {} export function update(value) { return value; }",
        )
        .expect("analysis");
        assert!(analysis.handles_pointer_click);
    }

    #[test]
    fn distinguishes_second_and_subsecond_clock_dependencies() {
        let seconds = analyze_scene_script(
            "export function update() { const now = new Date(); return now.getSeconds(); }",
        )
        .expect("seconds");
        assert!(seconds.uses_local_time_seconds);
        assert!(!seconds.uses_local_time_subseconds);

        let milliseconds = analyze_scene_script(
            "export function update() { return new Date().getMilliseconds() + Date.now(); }",
        )
        .expect("milliseconds");
        assert!(milliseconds.uses_local_time_subseconds);
    }
}
