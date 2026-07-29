//! Pretty-printer aligned with the official KDL test-suite translation rules.
//!
//! See `references/kdl/tests/README.md` (Translation Rules).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::parse::chars::{is_disallowed_literal, is_non_identifier_char};
use crate::value::{Document, Entry, Node, Value};

/// Write a document using the official test-suite pretty-print conventions.
pub fn format_document(doc: &Document<'_>) -> String {
    let mut out = String::new();
    for node in &doc.nodes {
        format_node(node, 0, &mut out);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn format_node(node: &Node<'_>, indent: usize, out: &mut String) {
    write_indent(indent, out);
    if let Some(ty) = &node.type_name {
        let _ = write!(out, "({})", format_ident_or_string(ty.as_str()));
    }
    out.push_str(&format_ident_or_string(node.name.as_str()));

    for entry in &node.entries {
        if let Entry::Argument { type_name, value } = entry {
            out.push(' ');
            if let Some(ty) = type_name {
                let _ = write!(out, "({})", format_ident_or_string(ty.as_str()));
            }
            out.push_str(&format_value(value));
        }
    }

    // Rightmost property wins, then alphabetical by key (BTreeMap).
    let mut props = BTreeMap::<&str, (Option<&str>, &Value<'_>)>::new();
    for entry in &node.entries {
        if let Entry::Property {
            key,
            type_name,
            value,
        } = entry
        {
            props.insert(
                key.as_str(),
                (type_name.as_ref().map(|t| t.as_str()), value),
            );
        }
    }
    for (k, (ty, v)) in props {
        out.push(' ');
        out.push_str(&format_ident_or_string(k));
        out.push('=');
        if let Some(ty) = ty {
            let _ = write!(out, "({})", format_ident_or_string(ty));
        }
        out.push_str(&format_value(v));
    }

    if node.children.is_empty() {
        out.push('\n');
        return;
    }

    out.push_str(" {\n");
    for child in &node.children {
        format_node(child, indent + 1, out);
    }
    write_indent(indent, out);
    out.push_str("}\n");
}

fn write_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn format_value(value: &Value<'_>) -> String {
    match value {
        Value::String(s) => format_ident_or_string(s.as_str()),
        Value::Bool(true) => "#true".to_owned(),
        Value::Bool(false) => "#false".to_owned(),
        Value::Null => "#null".to_owned(),
        Value::Int(n) => n.to_string(),
        Value::Float { value, raw } => {
            if let Some(raw) = raw {
                format_float_lexical(raw.as_str(), *value)
            } else {
                format_float(*value)
            }
        }
    }
}

/// Pretty-print from the original lexeme when present (strip `_`, normalize `E`).
fn format_float_lexical(lex: &str, value: f64) -> String {
    let cleaned: String = lex.chars().filter(|c| *c != '_').collect();
    if !value.is_finite()
        && (cleaned.contains('e') || cleaned.contains('E') || cleaned.contains('.'))
    {
        // Extreme exponent beyond f64 — emit cleaned scientific form.
        let mut s = cleaned.replace('e', "E");
        if let Some(idx) = s.find('E') {
            let rest = &s[idx + 1..];
            if !rest.is_empty() && !rest.starts_with('+') && !rest.starts_with('-') {
                s = format!("{}E+{}", &s[..idx], rest);
            }
        }
        return s;
    }
    if cleaned.contains('e') || cleaned.contains('E') {
        let mut s = cleaned.replace('e', "E");
        if let Some(idx) = s.find('E') {
            let rest = &s[idx + 1..];
            if !rest.is_empty() && !rest.starts_with('+') && !rest.starts_with('-') {
                s = format!("{}E+{}", &s[..idx], rest);
            }
        }
        // Suite likes a fractional digit on negative small scientific forms like `1.0E-100`.
        // If mantissa has no dot and value is finite, keep as-is (`1E+10`).
        return s;
    }
    if cleaned.contains('.') {
        // Decimal float lexeme: ensure we don't re-expand.
        return cleaned;
    }
    format_float(value)
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "#nan".to_owned();
    }
    if f.is_infinite() {
        return if f.is_sign_negative() {
            "#-inf".to_owned()
        } else {
            "#inf".to_owned()
        };
    }
    // Official suite: use `E` notation; keep a decimal form for ordinary values
    // (`-1.0`, `10.0`) and scientific form when `|f|` is extreme.
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }
    let abs = f.abs();
    // Suite style: scientific when magnitude is large or small; otherwise decimal
    // with at least one fractional digit for whole floats (`10.0`).
    if abs >= 1e10 || (abs > 0.0 && abs < 1e-4) {
        let exp = f.abs().log10().floor() as i32;
        let mant = f / 10f64.powi(exp);
        let mant = (mant * 1e12).round() / 1e12;
        let mant_s = if mant.fract().abs() < 1e-12 {
            // Suite: `1E+10` without forced `.0` in mantissa for integer mantissas.
            format!("{}", mant as i64)
        } else {
            format!("{mant}")
        };
        let sign = if exp >= 0 { "+" } else { "" };
        format!("{mant_s}E{sign}{exp}")
    } else if f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        let s = format!("{f}");
        if !s.contains('.') {
            format!("{s}.0")
        } else {
            s
        }
    }
}

fn format_ident_or_string(s: &str) -> String {
    if is_bare_ident(s) {
        s.to_owned()
    } else {
        format_quoted(s)
    }
}

fn is_bare_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match s {
        "true" | "false" | "null" | "inf" | "-inf" | "nan" => return false,
        _ => {}
    }
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if first.is_ascii_digit() {
        return false;
    }
    if first == '.' && chars.clone().next().is_some_and(|c| c.is_ascii_digit()) {
        return false;
    }
    if first == '+' || first == '-' {
        match s.chars().nth(1) {
            Some(n) if n.is_ascii_digit() => return false,
            Some('.') if s.chars().nth(2).is_some_and(|c| c.is_ascii_digit()) => {
                return false;
            }
            _ => {}
        }
    }
    !s.chars()
        .any(|c| is_non_identifier_char(c) || is_disallowed_literal(c))
}

fn format_quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if c.is_control() || is_disallowed_literal(c) => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
