use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const CONTROLLER_SETTING_KEYS: &[&str] = &[
    "allowAutoPlay",
    "cooldownSec",
    "endTimePercent",
    "fadeInDuration",
    "fadeOutDuration",
    "hideWhenPaused",
    "hideWhenStopped",
    "isClickable",
    "loopCount",
    "loopPlay",
    "mouseInactiveSec",
    "playbackSpeed",
    "resetOnClick",
    "resetOnRestart",
    "startDelay",
    "startTimePercent",
    "togglePlay",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneControllerKind {
    IdleVideoSwitch,
    ClickVideoSwitch,
    PropertyVideoSwitch,
}

impl SceneControllerKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::IdleVideoSwitch => "idle-video-switch",
            Self::ClickVideoSwitch => "click-video-switch",
            Self::PropertyVideoSwitch => "property-video-switch",
        }
    }

    fn from_wallpaper_engine_utility(
        utility: &str,
        script_properties: &Map<String, Value>,
    ) -> Self {
        if utility == "fullscreenlayer" || script_properties.contains_key("mouseInactiveSec") {
            Self::IdleVideoSwitch
        } else if utility == "composelayer"
            || script_properties.contains_key("isClickable")
            || script_properties.contains_key("togglePlay")
        {
            Self::ClickVideoSwitch
        } else {
            Self::PropertyVideoSwitch
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneControllerIr {
    controller_node_id: String,
    utility: String,
    target_layer: String,
    property: String,
    default_hide_target: bool,
    kind: SceneControllerKind,
    settings: BTreeMap<String, Value>,
}

impl SceneControllerIr {
    pub(super) fn from_wallpaper_engine_utility(
        controller_node_id: &str,
        utility: &str,
        target_layer: &str,
        default_hide_target: bool,
        script_properties: &Map<String, Value>,
    ) -> Self {
        let settings = CONTROLLER_SETTING_KEYS
            .iter()
            .filter_map(|key| {
                script_properties
                    .get(*key)
                    .map(|value| (scene_controller_property_name(key), value.clone()))
            })
            .collect();
        Self {
            controller_node_id: controller_node_id.to_owned(),
            utility: utility.to_owned(),
            target_layer: target_layer.to_owned(),
            property: format!("scene.controller.{controller_node_id}.active"),
            default_hide_target,
            kind: SceneControllerKind::from_wallpaper_engine_utility(utility, script_properties),
            settings,
        }
    }

    pub(super) fn controller_node_id(&self) -> &str {
        &self.controller_node_id
    }

    pub(super) fn target_layer(&self) -> &str {
        &self.target_layer
    }

    pub(super) fn default_hide_target(&self) -> bool {
        self.default_hide_target
    }

    pub(super) fn uses_native_idle_input_source(&self) -> bool {
        self.kind == SceneControllerKind::IdleVideoSwitch
    }

    pub(super) fn uses_native_idle_fade_ramp(&self) -> bool {
        self.uses_native_idle_input_source()
            && self
                .settings
                .get("fade_in_duration")
                .and_then(scene_ir_setting_number)
                .is_some_and(|value| value > 0.0)
    }

    pub(super) fn requires_external_input_source(&self) -> bool {
        !self.uses_native_idle_input_source()
    }

    pub(super) fn metadata_value(&self) -> Value {
        let mut controller = Map::new();
        controller.insert("runtime".to_owned(), Value::String("native".to_owned()));
        controller.insert(
            "kind".to_owned(),
            Value::String(self.kind.as_str().to_owned()),
        );
        controller.insert("utility".to_owned(), Value::String(self.utility.clone()));
        controller.insert(
            "target_layer".to_owned(),
            Value::String(self.target_layer.clone()),
        );
        controller.insert("property".to_owned(), Value::String(self.property.clone()));
        controller.insert(
            "default_hide_target".to_owned(),
            json!(self.default_hide_target),
        );
        for (key, value) in &self.settings {
            controller.insert(key.clone(), value.clone());
        }
        Value::Object(controller)
    }

    pub(super) fn property_binding_value(&self, target_node_id: &str) -> Value {
        json!({
            "property": self.property.clone(),
            "target_node": target_node_id,
            "target": "opacity",
            "scale": 1.0,
            "offset": 0.0
        })
    }

    pub(super) fn completed_feature_name(&self) -> String {
        format!("native-scene-controller-{}", self.kind.as_str())
    }
}

fn scene_controller_property_name(key: &str) -> String {
    let mut output = String::new();
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn scene_ir_setting_number(value: &Value) -> Option<f64> {
    let value = value.get("value").unwrap_or(value);
    let number = match value {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneNumericPropertyBindingIr {
    property: String,
    scale: f64,
    offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum SceneNumericPropertyBindingIrResult {
    Lowered {
        binding: SceneNumericPropertyBindingIr,
        used_script: bool,
    },
    UnsupportedScriptWithProperty,
}

impl SceneNumericPropertyBindingIr {
    pub(super) fn from_wallpaper_engine_parts(
        default_property: Option<String>,
        default_value: Option<f64>,
        script: Option<&str>,
    ) -> Option<SceneNumericPropertyBindingIrResult> {
        if let Some(script) = script {
            return match scene_script_linear_property_binding(
                script,
                default_property.as_deref(),
                default_value,
            ) {
                Some(binding) => Some(SceneNumericPropertyBindingIrResult::Lowered {
                    binding,
                    used_script: true,
                }),
                None if default_property.is_some() => {
                    Some(SceneNumericPropertyBindingIrResult::UnsupportedScriptWithProperty)
                }
                None => None,
            };
        }
        default_property.map(|property| SceneNumericPropertyBindingIrResult::Lowered {
            binding: SceneNumericPropertyBindingIr {
                property,
                scale: 1.0,
                offset: 0.0,
            },
            used_script: false,
        })
    }

    pub(super) fn property_binding_value(
        &self,
        target_node_id: &str,
        target: &str,
        target_scale: f64,
        target_offset: f64,
    ) -> Value {
        let scale = self.scale * target_scale;
        let offset = self.offset * target_scale + target_offset;
        json!({
            "property": self.property,
            "target_node": target_node_id,
            "target": target,
            "scale": scale,
            "offset": offset
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SceneScriptLinearExpression {
    property: Option<String>,
    scale: f64,
    offset: f64,
}

fn scene_script_linear_property_binding(
    script: &str,
    default_property: Option<&str>,
    default_value: Option<f64>,
) -> Option<SceneNumericPropertyBindingIr> {
    let expression = scene_script_return_expression(script)?;
    let expression =
        SceneScriptLinearParser::new(expression, default_property, default_value).parse()?;
    let property = expression.property?;
    if expression.scale.is_finite() && expression.offset.is_finite() {
        Some(SceneNumericPropertyBindingIr {
            property,
            scale: expression.scale,
            offset: expression.offset,
        })
    } else {
        None
    }
}

fn scene_script_return_expression(script: &str) -> Option<&str> {
    let script = script.trim();
    if let Some(index) = scene_script_return_keyword(script) {
        let returned = &script[index + "return".len()..];
        let end = scene_script_expression_end(returned).unwrap_or(returned.len());
        return scene_script_trim_expression(&returned[..end]);
    }
    if script.contains('{') || script.contains('=') {
        None
    } else {
        scene_script_trim_expression(script)
    }
}

fn scene_script_return_keyword(script: &str) -> Option<usize> {
    let mut search_offset = 0;
    while let Some(index) = script[search_offset..].find("return") {
        let index = search_offset + index;
        let before = script[..index].chars().next_back();
        let after = script[index + "return".len()..].chars().next();
        let before_boundary =
            before.is_none_or(|character| !scene_script_identifier_character(character));
        let after_boundary =
            after.is_none_or(|character| !scene_script_identifier_character(character));
        if before_boundary && after_boundary {
            return Some(index);
        }
        search_offset = index + "return".len();
    }
    None
}

fn scene_script_expression_end(expression: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut string_quote = None;
    let mut escaped = false;
    for (index, character) in expression.char_indices() {
        if let Some(quote) = string_quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                string_quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => string_quote = Some(character),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn scene_script_trim_expression(expression: &str) -> Option<&str> {
    let mut expression = expression.trim();
    while let Some(trimmed) = expression
        .strip_suffix(';')
        .or_else(|| expression.strip_suffix('}'))
    {
        expression = trimmed.trim();
    }
    if expression.is_empty() {
        None
    } else {
        Some(expression)
    }
}

impl SceneScriptLinearExpression {
    fn constant(offset: f64) -> Self {
        Self {
            property: None,
            scale: 0.0,
            offset,
        }
    }

    fn variable(property: String) -> Self {
        Self {
            property: Some(property),
            scale: 1.0,
            offset: 0.0,
        }
    }

    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            property: scene_script_merge_property(self.property, other.property)?,
            scale: self.scale + other.scale,
            offset: self.offset + other.offset,
        })
    }

    fn sub(self, other: Self) -> Option<Self> {
        Some(Self {
            property: scene_script_merge_property(self.property, other.property)?,
            scale: self.scale - other.scale,
            offset: self.offset - other.offset,
        })
    }

    fn mul(self, other: Self) -> Option<Self> {
        if self.property.is_some() && other.property.is_some() {
            return None;
        }
        if self.property.is_some() {
            return Some(Self {
                property: self.property,
                scale: self.scale * other.offset,
                offset: self.offset * other.offset,
            });
        }
        if other.property.is_some() {
            return Some(Self {
                property: other.property,
                scale: other.scale * self.offset,
                offset: other.offset * self.offset,
            });
        }
        Some(Self::constant(self.offset * other.offset))
    }

    fn div(self, other: Self) -> Option<Self> {
        if other.property.is_some() || other.offset == 0.0 {
            return None;
        }
        Some(Self {
            property: self.property,
            scale: self.scale / other.offset,
            offset: self.offset / other.offset,
        })
    }

    fn neg(self) -> Self {
        Self {
            property: self.property,
            scale: -self.scale,
            offset: -self.offset,
        }
    }
}

fn scene_script_merge_property(
    left: Option<String>,
    right: Option<String>,
) -> Option<Option<String>> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if left == right || scene_ir_normalize_key(&left) == scene_ir_normalize_key(&right) {
                Some(Some(left))
            } else {
                None
            }
        }
        (Some(property), None) | (None, Some(property)) => Some(Some(property)),
        (None, None) => Some(None),
    }
}

struct SceneScriptLinearParser<'a> {
    expression: &'a str,
    position: usize,
    default_property: Option<&'a str>,
    default_value: Option<f64>,
}

impl<'a> SceneScriptLinearParser<'a> {
    fn new(
        expression: &'a str,
        default_property: Option<&'a str>,
        default_value: Option<f64>,
    ) -> Self {
        Self {
            expression,
            position: 0,
            default_property,
            default_value,
        }
    }

    fn parse(mut self) -> Option<SceneScriptLinearExpression> {
        let expression = self.parse_expression()?;
        self.skip_whitespace();
        if self.position == self.expression.len() {
            Some(expression)
        } else {
            None
        }
    }

    fn parse_expression(&mut self) -> Option<SceneScriptLinearExpression> {
        let mut expression = self.parse_term()?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b'+') {
                expression = expression.add(self.parse_term()?)?;
            } else if self.consume_byte(b'-') {
                expression = expression.sub(self.parse_term()?)?;
            } else {
                return Some(expression);
            }
        }
    }

    fn parse_term(&mut self) -> Option<SceneScriptLinearExpression> {
        let mut expression = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b'*') {
                expression = expression.mul(self.parse_unary()?)?;
            } else if self.consume_byte(b'/') {
                expression = expression.div(self.parse_unary()?)?;
            } else {
                return Some(expression);
            }
        }
    }

    fn parse_unary(&mut self) -> Option<SceneScriptLinearExpression> {
        self.skip_whitespace();
        if self.consume_byte(b'+') {
            self.parse_unary()
        } else if self.consume_byte(b'-') {
            Some(self.parse_unary()?.neg())
        } else {
            self.parse_atom()
        }
    }

    fn parse_atom(&mut self) -> Option<SceneScriptLinearExpression> {
        self.skip_whitespace();
        if self.consume_byte(b'(') {
            let expression = self.parse_expression()?;
            self.skip_whitespace();
            return self.consume_byte(b')').then_some(expression);
        }
        if self.peek_byte().is_some_and(scene_script_number_start) {
            return self
                .parse_number()
                .map(SceneScriptLinearExpression::constant);
        }
        let identifier = self.parse_identifier()?;
        self.skip_whitespace();
        if self.consume_byte(b'(') {
            return self.parse_call(&identifier);
        }
        self.resolve_identifier(&identifier)
    }

    fn parse_call(&mut self, identifier: &str) -> Option<SceneScriptLinearExpression> {
        if scene_script_user_property_call(identifier) {
            self.skip_whitespace();
            let property = self.parse_string_literal()?;
            self.skip_call_remainder()?;
            return Some(SceneScriptLinearExpression::variable(property));
        }
        if scene_script_identity_numeric_call(identifier) {
            let expression = self.parse_expression()?;
            self.skip_whitespace();
            return self.consume_byte(b')').then_some(expression);
        }
        None
    }

    fn resolve_identifier(&self, identifier: &str) -> Option<SceneScriptLinearExpression> {
        match identifier {
            "value" => self
                .default_value
                .map(SceneScriptLinearExpression::constant),
            "true" => Some(SceneScriptLinearExpression::constant(1.0)),
            "false" => Some(SceneScriptLinearExpression::constant(0.0)),
            _ => scene_script_property_from_identifier(identifier, self.default_property)
                .map(SceneScriptLinearExpression::variable),
        }
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.position;
        let mut saw_digit = false;
        while let Some(byte) = self.peek_byte() {
            match byte {
                b'0'..=b'9' => {
                    saw_digit = true;
                    self.position += 1;
                }
                b'.' => self.position += 1,
                b'e' | b'E' => {
                    self.position += 1;
                    if self
                        .peek_byte()
                        .is_some_and(|byte| byte == b'+' || byte == b'-')
                    {
                        self.position += 1;
                    }
                }
                _ => break,
            }
        }
        if !saw_digit {
            return None;
        }
        self.expression[start..self.position].parse().ok()
    }

    fn parse_identifier(&mut self) -> Option<String> {
        let start = self.position;
        let first = self.peek_byte()?;
        if !scene_script_identifier_start_byte(first) {
            return None;
        }
        self.position += 1;
        while self
            .peek_byte()
            .is_some_and(scene_script_identifier_continue_byte)
        {
            self.position += 1;
        }
        Some(self.expression[start..self.position].to_owned())
    }

    fn parse_string_literal(&mut self) -> Option<String> {
        let quote = self.peek_byte()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        self.position += 1;
        let mut value = String::new();
        while let Some(byte) = self.peek_byte() {
            self.position += 1;
            if byte == quote {
                return Some(value);
            }
            if byte == b'\\' {
                let escaped = self.peek_byte()?;
                self.position += 1;
                value.push(escaped as char);
            } else {
                value.push(byte as char);
            }
        }
        None
    }

    fn skip_call_remainder(&mut self) -> Option<()> {
        let mut depth = 1usize;
        let mut quote = None;
        let mut escaped = false;
        while let Some(byte) = self.peek_byte() {
            self.position += 1;
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'(' => depth += 1,
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(());
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek_byte()
            .is_some_and(|byte| (byte as char).is_ascii_whitespace())
        {
            self.position += 1;
        }
    }

    fn consume_byte(&mut self, byte: u8) -> bool {
        if self.peek_byte() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.expression.as_bytes().get(self.position).copied()
    }
}

fn scene_script_property_from_identifier(
    identifier: &str,
    default_property: Option<&str>,
) -> Option<String> {
    if let Some(default_property) = default_property {
        let normalized_identifier = scene_ir_normalize_key(identifier);
        if matches!(identifier, "user" | "input" | "property")
            || normalized_identifier == scene_ir_normalize_key(default_property)
        {
            return Some(default_property.to_owned());
        }
    }
    let parts = identifier
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let normalized = scene_ir_normalize_key(part);
        if matches!(
            normalized.as_str(),
            "user" | "users" | "properties" | "property" | "input" | "inputs"
        ) {
            if let Some(property) = parts.get(index + 1)
                && scene_ir_normalize_key(property) != "value"
            {
                return Some((*property).to_owned());
            }
            if let Some(default_property) = default_property {
                return Some(default_property.to_owned());
            }
        }
    }
    None
}

fn scene_script_user_property_call(identifier: &str) -> bool {
    matches!(
        scene_ir_normalize_key(identifier).as_str(),
        "getuserproperty" | "userproperty" | "getproperty" | "wallpapergetuserproperty"
    )
}

fn scene_script_identity_numeric_call(identifier: &str) -> bool {
    matches!(
        scene_ir_normalize_key(identifier).as_str(),
        "number" | "parsefloat"
    )
}

fn scene_script_number_start(byte: u8) -> bool {
    byte.is_ascii_digit() || byte == b'.'
}

fn scene_script_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '$')
}

fn scene_script_identifier_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn scene_script_identifier_continue_byte(byte: u8) -> bool {
    scene_script_identifier_start_byte(byte) || byte.is_ascii_digit() || byte == b'.'
}

fn scene_ir_normalize_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_controller_ir_lowers_wallpaper_engine_utility_metadata() {
        let mut script_properties = Map::new();
        script_properties.insert("mouseInactiveSec".to_owned(), json!({ "value": 70 }));
        script_properties.insert("fadeInDuration".to_owned(), json!(0.5));
        let controller = SceneControllerIr::from_wallpaper_engine_utility(
            "node-idle",
            "fullscreenlayer",
            "Idle Layer",
            true,
            &script_properties,
        );

        assert!(controller.uses_native_idle_input_source());
        assert!(controller.uses_native_idle_fade_ramp());
        assert!(!controller.requires_external_input_source());
        assert_eq!(
            controller.completed_feature_name(),
            "native-scene-controller-idle-video-switch"
        );
        assert_eq!(
            controller.metadata_value(),
            json!({
                "runtime": "native",
                "kind": "idle-video-switch",
                "utility": "fullscreenlayer",
                "target_layer": "Idle Layer",
                "property": "scene.controller.node-idle.active",
                "default_hide_target": true,
                "fade_in_duration": 0.5,
                "mouse_inactive_sec": { "value": 70 }
            })
        );
        assert_eq!(
            controller.property_binding_value("target-node"),
            json!({
                "property": "scene.controller.node-idle.active",
                "target_node": "target-node",
                "target": "opacity",
                "scale": 1.0,
                "offset": 0.0
            })
        );
    }

    #[test]
    fn click_controller_ir_marks_external_input_requirement() {
        let mut script_properties = Map::new();
        script_properties.insert("togglePlay".to_owned(), json!(true));
        let controller = SceneControllerIr::from_wallpaper_engine_utility(
            "node-click",
            "composelayer",
            "Click Layer",
            true,
            &script_properties,
        );

        assert!(controller.requires_external_input_source());
        assert_eq!(
            controller.completed_feature_name(),
            "native-scene-controller-click-video-switch"
        );
        assert_eq!(controller.metadata_value()["kind"], "click-video-switch");
        assert_eq!(controller.metadata_value()["toggle_play"], true);
    }

    #[test]
    fn numeric_scenescript_ir_lowers_linear_user_property_expressions() {
        let lowered = SceneNumericPropertyBindingIr::from_wallpaper_engine_parts(
            Some("panel_x".to_owned()),
            Some(10.0),
            Some("return value + user * 2 + 5;"),
        )
        .unwrap();

        assert_eq!(
            lowered,
            SceneNumericPropertyBindingIrResult::Lowered {
                binding: SceneNumericPropertyBindingIr {
                    property: "panel_x".to_owned(),
                    scale: 2.0,
                    offset: 15.0,
                },
                used_script: true,
            }
        );
        let SceneNumericPropertyBindingIrResult::Lowered { binding, .. } = lowered else {
            unreachable!();
        };
        assert_eq!(
            binding.property_binding_value("node-panel", "x", 1.0, 0.0),
            json!({
                "property": "panel_x",
                "target_node": "node-panel",
                "target": "x",
                "scale": 2.0,
                "offset": 15.0
            })
        );
    }

    #[test]
    fn numeric_scenescript_ir_reports_unreduced_user_scripts() {
        let lowered = SceneNumericPropertyBindingIr::from_wallpaper_engine_parts(
            Some("panel_x".to_owned()),
            Some(10.0),
            Some("return Math.sin(user);"),
        );

        assert_eq!(
            lowered,
            Some(SceneNumericPropertyBindingIrResult::UnsupportedScriptWithProperty)
        );
    }
}
