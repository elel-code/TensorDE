//! Strict Rust preprocessing for package-owned Wallpaper Engine shader stages.
//!
//! This is deliberately a small C-preprocessor subset, not a GLSL frontend.
//! It resolves virtual package includes, specializes combos, and expands
//! authored macros before the generated-stage source is lowered to Slang
//! Slang. Unsupported active syntax is an explicit conversion error.

use std::collections::{BTreeMap, BTreeSet};

use vulkan_renderer_build::ShaderStage;

use super::compiler_environment::inject_we_compiler_preamble;
use super::{AuthoredProgramSpec, shader_error};
use crate::convert::we_ingest::ingest::WeIngestError;
use crate::convert::we_ingest::ingest::asset_source::WeAssetSource;

mod expression;
use expression::evaluate_expression;

const MAX_INCLUDE_DEPTH: usize = 64;
const MAX_MACRO_EXPANSION_DEPTH: usize = 64;

#[derive(Clone)]
enum MacroDefinition {
    Object(String),
    Function {
        parameters: Vec<String>,
        replacement: String,
    },
}

struct ConditionalBranch {
    parent_active: bool,
    branch_taken: bool,
    active: bool,
    saw_else: bool,
}

struct PackagePreprocessor<'a> {
    source: &'a WeAssetSource,
    macros: BTreeMap<String, MacroDefinition>,
    include_stack: Vec<String>,
}

pub(super) fn specialize_stage(
    source: &WeAssetSource,
    spec: &AuthoredProgramSpec,
    stage: ShaderStage,
    stage_source: &str,
    definitions: &[(String, String)],
) -> Result<String, WeIngestError> {
    let extension = match stage {
        ShaderStage::Vertex => "vert",
        ShaderStage::Fragment => "frag",
        ShaderStage::Compute => {
            return Err(shader_error(
                &spec.program_key,
                "program",
                "package-owned compute stages are not supported",
            ));
        }
    };
    let source_path = format!("shaders/{}.{}", spec.source_key, extension);
    let mut preprocessor = PackagePreprocessor::new(source, definitions)
        .map_err(|message| shader_error(&spec.program_key, stage.slang_name(), message))?;
    preprocessor
        .process_source(&source_path, &inject_we_compiler_preamble(stage_source))
        .map_err(|message| shader_error(&spec.program_key, stage.slang_name(), message))
}

pub(super) fn strip_specialized_stage_comments(source: &str) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    for (_, line, _) in logical_lines(source)? {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

impl<'a> PackagePreprocessor<'a> {
    fn new(source: &'a WeAssetSource, definitions: &[(String, String)]) -> Result<Self, String> {
        let mut macros = BTreeMap::new();
        for (name, value) in definitions {
            validate_identifier(name, "initial macro")?;
            macros.insert(name.clone(), MacroDefinition::Object(value.clone()));
        }
        Ok(Self {
            source,
            macros,
            include_stack: Vec::new(),
        })
    }

    fn process_source(&mut self, path: &str, source: &str) -> Result<String, String> {
        if self.include_stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(format!(
                "include nesting exceeds {MAX_INCLUDE_DEPTH} at {path}"
            ));
        }
        if self.include_stack.iter().any(|active| active == path) {
            let mut chain = self.include_stack.clone();
            chain.push(path.to_owned());
            return Err(format!("cyclic shader include: {}", chain.join(" -> ")));
        }
        self.include_stack.push(path.to_owned());
        let result = self.process_source_inner(path, source);
        self.include_stack.pop();
        result
    }

    fn process_source_inner(&mut self, path: &str, source: &str) -> Result<String, String> {
        let lines = logical_lines(source)?;
        let mut output = String::new();
        let mut branches = Vec::<ConditionalBranch>::new();
        let mut pending_macro_line = None::<(usize, String)>;
        for (line_number, line, line_comment) in lines {
            let trimmed = line.trim_start();
            let Some(directive) = trimmed.strip_prefix('#') else {
                if is_active(&branches) {
                    let (start, pending) =
                        pending_macro_line.get_or_insert_with(|| (line_number, String::new()));
                    if !pending.is_empty() {
                        pending.push('\n');
                    }
                    pending.push_str(&line);
                    match self.expand_text(pending, &mut Vec::new()) {
                        Ok(expanded) => {
                            output.push_str(&expanded);
                            if let Some(comment) = line_comment {
                                if !expanded.ends_with(char::is_whitespace) {
                                    output.push(' ');
                                }
                                output.push_str(&comment);
                            }
                            output.push('\n');
                            pending_macro_line = None;
                        }
                        Err(error) if error == "unterminated parenthesis" => {}
                        Err(error) => return directive_error(path, *start, error),
                    }
                } else if pending_macro_line.is_some() {
                    return directive_error(
                        path,
                        line_number,
                        "macro invocation crosses an inactive conditional branch",
                    );
                }
                continue;
            };
            if let Some((start, _)) = pending_macro_line.as_ref() {
                return directive_error(
                    path,
                    *start,
                    "macro invocation crosses a preprocessor directive",
                );
            }
            let (command, argument) = split_directive(directive);
            match command {
                "if" => {
                    let parent_active = is_active(&branches);
                    let active =
                        parent_active && self.evaluate_condition(argument, path, line_number)? != 0;
                    branches.push(ConditionalBranch {
                        parent_active,
                        branch_taken: active,
                        active,
                        saw_else: false,
                    });
                }
                "ifdef" => {
                    let parent_active = is_active(&branches);
                    let name = directive_identifier(argument, path, line_number, "#ifdef")?;
                    let active = parent_active && self.macros.contains_key(name);
                    branches.push(ConditionalBranch {
                        parent_active,
                        branch_taken: active,
                        active,
                        saw_else: false,
                    });
                }
                "ifndef" => {
                    let parent_active = is_active(&branches);
                    let name = directive_identifier(argument, path, line_number, "#ifndef")?;
                    let active = parent_active && !self.macros.contains_key(name);
                    branches.push(ConditionalBranch {
                        parent_active,
                        branch_taken: active,
                        active,
                        saw_else: false,
                    });
                }
                "elif" => {
                    let Some(branch) = branches.last_mut() else {
                        return directive_error(path, line_number, "#elif has no matching #if");
                    };
                    if branch.saw_else {
                        return directive_error(path, line_number, "#elif follows #else");
                    }
                    if !branch.parent_active || branch.branch_taken {
                        branch.active = false;
                    } else {
                        let active = self.evaluate_condition(argument, path, line_number)? != 0;
                        branch.active = active;
                        branch.branch_taken |= active;
                    }
                }
                "else" => {
                    if !argument.trim().is_empty() {
                        return directive_error(path, line_number, "#else has trailing tokens");
                    }
                    let Some(branch) = branches.last_mut() else {
                        return directive_error(path, line_number, "#else has no matching #if");
                    };
                    if branch.saw_else {
                        return directive_error(path, line_number, "duplicate #else");
                    }
                    branch.active = branch.parent_active && !branch.branch_taken;
                    branch.branch_taken = true;
                    branch.saw_else = true;
                }
                "endif" => {
                    if !argument.trim().is_empty() {
                        return directive_error(path, line_number, "#endif has trailing tokens");
                    }
                    // WE's HLSL toolchain defines a depth-zero #endif as a no-op. Community
                    // shader templates rely on that rule before later, otherwise balanced
                    // conditional blocks; all other malformed conditional structure remains an
                    // explicit error in this parser.
                    let _ = branches.pop();
                }
                "define" if is_active(&branches) => {
                    let (name, definition) = parse_definition(argument)
                        .map_err(|error| directive_message(path, line_number, error))?;
                    self.macros.insert(name, definition);
                }
                "undef" if is_active(&branches) => {
                    let name = directive_identifier(argument, path, line_number, "#undef")?;
                    self.macros.remove(name);
                }
                "include" if is_active(&branches) => {
                    let include = parse_include(argument)
                        .map_err(|error| directive_message(path, line_number, error))?;
                    let (include_path, include_source) = self
                        .source
                        .read_shader_include(path, include)
                        .map_err(|error| directive_message(path, line_number, error.to_string()))?;
                    output.push_str(&self.process_source(&include_path, &include_source)?);
                }
                "version" if is_active(&branches) => {
                    if argument.trim().is_empty() {
                        return directive_error(path, line_number, "#version has no version");
                    }
                }
                "extension" if is_active(&branches) => {
                    return directive_error(
                        path,
                        line_number,
                        "active #extension is not supported by generated stage lowering",
                    );
                }
                "" if is_active(&branches) => {
                    return directive_error(path, line_number, "empty preprocessor directive");
                }
                _ if !is_active(&branches) => {}
                _ => {
                    return directive_error(
                        path,
                        line_number,
                        format!("unsupported active preprocessor directive #{command}"),
                    );
                }
            }
        }
        if let Some((line, _)) = pending_macro_line {
            return directive_error(path, line, "unterminated macro invocation");
        }
        if !branches.is_empty() {
            return Err(format!(
                "{path}: unterminated conditional compilation block"
            ));
        }
        Ok(prune_unreachable_direct_returns(output))
    }

    fn evaluate_condition(
        &self,
        expression: &str,
        path: &str,
        line_number: usize,
    ) -> Result<i64, String> {
        evaluate_expression(expression, &self.macros)
            .map_err(|error| directive_message(path, line_number, error))
    }

    fn expand_text(&self, text: &str, stack: &mut Vec<String>) -> Result<String, String> {
        if stack.len() >= MAX_MACRO_EXPANSION_DEPTH {
            return Err(format!(
                "macro expansion exceeds {MAX_MACRO_EXPANSION_DEPTH} levels"
            ));
        }
        let mut output = String::with_capacity(text.len());
        let mut offset = 0;
        while offset < text.len() {
            let character = next_character(text, offset)?;
            if is_identifier_start(character) {
                let end = identifier_end(text, offset);
                let name = &text[offset..end];
                let definition = self.macros.get(name).cloned();
                match definition {
                    Some(MacroDefinition::Object(replacement)) => {
                        if stack.iter().any(|active| active == name) {
                            return Err(format!("recursive object macro {name}"));
                        }
                        stack.push(name.to_owned());
                        output.push_str(&self.expand_text(&replacement, stack)?);
                        stack.pop();
                    }
                    Some(MacroDefinition::Function {
                        parameters,
                        replacement,
                    }) if text[end..].starts_with('(') => {
                        let (after, arguments) = parse_macro_arguments(text, end)?;
                        if arguments.len() != parameters.len() {
                            return Err(format!(
                                "macro {name} expects {} arguments, found {}",
                                parameters.len(),
                                arguments.len()
                            ));
                        }
                        if stack.iter().any(|active| active == name) {
                            return Err(format!("recursive function macro {name}"));
                        }
                        let mut values = BTreeMap::new();
                        for (parameter, argument) in parameters.iter().zip(arguments) {
                            values.insert(
                                parameter.as_str(),
                                self.expand_text(argument.trim(), stack)?,
                            );
                        }
                        let replacement = substitute_parameters(&replacement, &values)?;
                        stack.push(name.to_owned());
                        output.push_str(&self.expand_text(&replacement, stack)?);
                        stack.pop();
                        offset = after;
                        continue;
                    }
                    _ => output.push_str(name),
                }
                offset = end;
                continue;
            }
            if character == '"' || character == '\'' {
                let end = quoted_end(text, offset, character)?;
                output.push_str(&text[offset..end]);
                offset = end;
                continue;
            }
            output.push(character);
            offset += character.len_utf8();
        }
        Ok(output)
    }
}

fn logical_lines(source: &str) -> Result<Vec<(usize, String, Option<String>)>, String> {
    let mut lines = Vec::new();
    let mut block_comment = false;
    let mut continuation = None::<(usize, String)>;
    for (index, raw_line) in source.trim_start_matches('\u{feff}').lines().enumerate() {
        let line_number = index + 1;
        let (mut line, line_comment) = strip_comments(raw_line, &mut block_comment)?;
        if line.trim_end().ends_with('\\') {
            let without_continuation = line.trim_end().trim_end_matches('\\').trim_end();
            if let Some((_, pending)) = &mut continuation {
                pending.push_str(without_continuation);
                pending.push(' ');
            } else {
                continuation = Some((line_number, format!("{without_continuation} ")));
            }
            continue;
        }
        if let Some((start, pending)) = &mut continuation {
            pending.push_str(line.trim_start());
            lines.push((*start, std::mem::take(pending), line_comment));
            continuation = None;
        } else {
            lines.push((line_number, std::mem::take(&mut line), line_comment));
        }
    }
    if block_comment {
        return Err("unterminated block comment".to_owned());
    }
    if let Some((line, _)) = continuation {
        return Err(format!("line {line}: unterminated line continuation"));
    }
    Ok(lines)
}

fn strip_comments(
    line: &str,
    block_comment: &mut bool,
) -> Result<(String, Option<String>), String> {
    let mut output = String::with_capacity(line.len());
    let characters = line.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < characters.len() {
        if *block_comment {
            if characters[index] == '*' && characters.get(index + 1) == Some(&'/') {
                *block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        let character = characters[index];
        if character == '/' && characters.get(index + 1) == Some(&'/') {
            let comment = characters[index..].iter().collect();
            return Ok((output, Some(comment)));
        }
        if character == '/' && characters.get(index + 1) == Some(&'*') {
            output.push(' ');
            *block_comment = true;
            index += 2;
            continue;
        }
        if character == '"' || character == '\'' {
            output.push(character);
            index += 1;
            let quote = character;
            while index < characters.len() {
                let current = characters[index];
                output.push(current);
                index += 1;
                if current == '\\' && index < characters.len() {
                    output.push(characters[index]);
                    index += 1;
                } else if current == quote {
                    break;
                }
            }
            continue;
        }
        output.push(character);
        index += 1;
    }
    Ok((output, None))
}

fn is_active(branches: &[ConditionalBranch]) -> bool {
    branches.last().is_none_or(|branch| branch.active)
}

/// After combo specialization, an include may leave one selected direct
/// return followed by its generic fallback in the same block. Slang treats
/// that fallback as an error under the required warnings-as-errors contract.
/// Remove only a later standalone return after an unconditional standalone
/// return at the same lexical brace depth; nested conditional returns remain
/// independent.
fn prune_unreachable_direct_returns(source: String) -> String {
    let mut terminated_blocks = vec![false];
    let mut retained = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let direct_return = starts_direct_return(trimmed) && trimmed.contains(';');
        if direct_return && terminated_blocks.last().copied().unwrap_or(false) {
            continue;
        }
        retained.push(line);
        if direct_return && let Some(terminated) = terminated_blocks.last_mut() {
            *terminated = true;
        }
        for character in line.chars() {
            match character {
                '{' => terminated_blocks.push(false),
                '}' if terminated_blocks.len() > 1 => {
                    terminated_blocks.pop();
                }
                _ => {}
            }
        }
    }
    retained.join("\n")
}

fn starts_direct_return(source: &str) -> bool {
    source
        .strip_prefix("return")
        .is_some_and(|tail| tail.starts_with(char::is_whitespace) || tail.starts_with(';'))
}

fn split_directive(directive: &str) -> (&str, &str) {
    let directive = directive.trim_start();
    let command_end = directive
        .find(|character: char| character.is_ascii_whitespace())
        .unwrap_or(directive.len());
    (&directive[..command_end], &directive[command_end..])
}

fn directive_identifier<'a>(
    argument: &'a str,
    path: &str,
    line_number: usize,
    directive: &str,
) -> Result<&'a str, String> {
    let argument = argument.trim();
    validate_identifier(argument, directive)
        .map_err(|error| directive_message(path, line_number, error))?;
    Ok(argument)
}

fn parse_definition(argument: &str) -> Result<(String, MacroDefinition), String> {
    let argument = argument.trim_start();
    let name_end = identifier_end(argument, 0);
    if name_end == 0 {
        return Err("#define has no macro name".to_owned());
    }
    let name = argument[..name_end].to_owned();
    validate_identifier(&name, "#define")?;
    let tail = &argument[name_end..];
    if tail.starts_with('(') {
        let close = matching_parenthesis(tail, 0)?;
        let parameters = tail[1..close]
            .split(',')
            .map(str::trim)
            .filter(|parameter| !parameter.is_empty())
            .map(|parameter| {
                validate_identifier(parameter, "function macro parameter")?;
                Ok(parameter.to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !tail[1..close].trim().is_empty() && parameters.is_empty() {
            return Err(format!("function macro {name} has an empty parameter"));
        }
        let mut names = BTreeSet::new();
        if parameters.iter().any(|parameter| !names.insert(parameter)) {
            return Err(format!("function macro {name} repeats a parameter"));
        }
        let replacement = tail[close + 1..].trim().to_owned();
        if replacement.contains("##") || replacement.contains('#') {
            return Err(format!(
                "function macro {name} uses unsupported # or ## operators"
            ));
        }
        return Ok((
            name,
            MacroDefinition::Function {
                parameters,
                replacement,
            },
        ));
    }
    Ok((name, MacroDefinition::Object(tail.trim().to_owned())))
}

fn parse_include(argument: &str) -> Result<&str, String> {
    let argument = argument.trim();
    let Some((open, close)) = argument.chars().next().and_then(|open| match open {
        '"' => Some(('"', '"')),
        '<' => Some(('<', '>')),
        _ => None,
    }) else {
        return Err("#include must use \"path\" or <path>".to_owned());
    };
    let Some(end) = argument[open.len_utf8()..].find(close) else {
        return Err("unterminated #include path".to_owned());
    };
    let end = open.len_utf8() + end;
    if !argument[end + close.len_utf8()..].trim().is_empty() {
        return Err("#include has trailing tokens".to_owned());
    }
    let path = &argument[open.len_utf8()..end];
    if path.is_empty() {
        return Err("#include path is empty".to_owned());
    }
    Ok(path)
}

fn substitute_parameters(
    replacement: &str,
    values: &BTreeMap<&str, String>,
) -> Result<String, String> {
    let mut output = String::with_capacity(replacement.len());
    let mut offset = 0;
    while offset < replacement.len() {
        let character = next_character(replacement, offset)?;
        if is_identifier_start(character) {
            let end = identifier_end(replacement, offset);
            let identifier = &replacement[offset..end];
            output.push_str(values.get(identifier).map_or(identifier, String::as_str));
            offset = end;
            continue;
        }
        if character == '"' || character == '\'' {
            let end = quoted_end(replacement, offset, character)?;
            output.push_str(&replacement[offset..end]);
            offset = end;
            continue;
        }
        output.push(character);
        offset += character.len_utf8();
    }
    Ok(output)
}

fn parse_macro_arguments(source: &str, open: usize) -> Result<(usize, Vec<&str>), String> {
    let close = matching_parenthesis(source, open)?;
    let body = &source[open + 1..close];
    if body.trim().is_empty() {
        return Ok((close + 1, Vec::new()));
    }
    let mut arguments = Vec::new();
    let mut start = open + 1;
    let mut depth = 0u32;
    let mut offset = open + 1;
    while offset < close {
        let character = next_character(source, offset)?;
        if character == '"' || character == '\'' {
            offset = quoted_end(source, offset, character)?;
            continue;
        }
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| "macro argument delimiter underflow".to_owned())?;
            }
            ',' if depth == 0 => {
                arguments.push(&source[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
        offset += character.len_utf8();
    }
    arguments.push(&source[start..close]);
    Ok((close + 1, arguments))
}

fn matching_parenthesis(source: &str, open: usize) -> Result<usize, String> {
    if !source[open..].starts_with('(') {
        return Err("expected opening parenthesis".to_owned());
    }
    let mut depth = 0u32;
    let mut offset = open;
    while offset < source.len() {
        let character = next_character(source, offset)?;
        if character == '"' || character == '\'' {
            offset = quoted_end(source, offset, character)?;
            continue;
        }
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| "parenthesis underflow".to_owned())?;
            if depth == 0 {
                return Ok(offset);
            }
        }
        offset += character.len_utf8();
    }
    Err("unterminated parenthesis".to_owned())
}

fn quoted_end(source: &str, start: usize, quote: char) -> Result<usize, String> {
    let mut offset = start + quote.len_utf8();
    while offset < source.len() {
        let character = next_character(source, offset)?;
        offset += character.len_utf8();
        if character == '\\' {
            let escaped = next_character(source, offset)?;
            offset += escaped.len_utf8();
        } else if character == quote {
            return Ok(offset);
        }
    }
    Err("unterminated quoted string".to_owned())
}

fn next_character(source: &str, offset: usize) -> Result<char, String> {
    source[offset..]
        .chars()
        .next()
        .ok_or_else(|| "unexpected end of source".to_owned())
}

fn validate_identifier(identifier: &str, context: &str) -> Result<(), String> {
    let mut characters = identifier.chars();
    let Some(first) = characters.next() else {
        return Err(format!("{context} has no identifier"));
    };
    if !is_identifier_start(first) || !characters.all(is_identifier_continue) {
        return Err(format!("{context} has invalid identifier {identifier:?}"));
    }
    Ok(())
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_ascii_digit()
}

fn identifier_end(source: &str, start: usize) -> usize {
    let mut end = start;
    while end < source.len() {
        let character = source[end..]
            .chars()
            .next()
            .expect("identifier scan is within source");
        if !is_identifier_continue(character) {
            break;
        }
        end += character.len_utf8();
    }
    end
}

fn directive_error<T>(
    path: &str,
    line: usize,
    message: impl std::fmt::Display,
) -> Result<T, String> {
    Err(directive_message(path, line, message))
}

fn directive_message(path: &str, line: usize, message: impl std::fmt::Display) -> String {
    format!("{path}:{line}: {message}")
}

#[cfg(test)]
mod tests;
