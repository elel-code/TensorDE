//! Oxc-based cold-path analysis of authored SceneScript modules.

use std::fmt;

use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, MemberExpression, NewExpression};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SceneScriptAnalysis {
    pub(super) exports_update: bool,
    pub(super) exports_init: bool,
    pub(super) handles_media: bool,
    pub(super) handles_user_properties: bool,
    pub(super) uses_runtime: bool,
    pub(super) uses_frame_time: bool,
    pub(super) uses_audio: bool,
    pub(super) uses_pointer: bool,
    pub(super) uses_local_time: bool,
    pub(super) imports: Vec<String>,
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
        uses_runtime: visitor.uses_runtime,
        uses_frame_time: visitor.uses_frame_time,
        uses_audio: visitor.uses_audio,
        uses_pointer: visitor.uses_pointer,
        uses_local_time: visitor.uses_local_time,
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
}

impl<'a> Visit<'a> for CapabilityVisitor {
    fn visit_member_expression(&mut self, expression: &MemberExpression<'a>) {
        if expression.is_specific_member_access("engine", "runtime") {
            self.uses_runtime = true;
        }
        if expression.is_specific_member_access("engine", "frametime") {
            self.uses_frame_time = true;
        }
        if expression.is_specific_member_access("Date", "now") {
            self.uses_local_time = true;
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

    fn visit_call_expression(&mut self, expression: &CallExpression<'a>) {
        match expression.callee_name() {
            Some("registerAudioBuffers") => self.uses_audio = true,
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
                // engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_32)
                const note = "new Date(); engine.runtime";
                export function update(value) { return value; }
            "#,
        )
        .expect("analysis");
        assert!(!analysis.uses_audio);
        assert!(!analysis.uses_runtime);
        assert!(!analysis.uses_local_time);
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
}
