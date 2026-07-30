//! Feature-gated miette presentation (core stays Glaze format_error).
#![cfg(feature = "diagnostics")]

use miette::Diagnostic;
use tensor_kdl::{format_error, from_str, report_error, report_error_named};

#[test]
fn report_error_has_source_label() {
    let input = "good 1\nbad {\n";
    let err = from_str(input).unwrap_err();
    let glaze = format_error(&err.ctx, input);
    assert!(glaze.contains('^') || glaze.contains(':'));

    let report = report_error(&err.ctx, input);
    let rendered = format!("{report:?}");
    assert!(
        !rendered.is_empty(),
        "miette report should render non-empty"
    );
    let display = format!("{report}");
    assert!(
        display.contains("syntax") || display.contains("expected") || display.contains('{'),
        "display={display:?}"
    );
}

#[test]
fn report_error_named_has_primary_and_line_labels() {
    // P-G9c: primary span at consumed + related full-line span.
    let input = "ok\nbad {\n";
    let err = from_str(input).unwrap_err();
    let report = report_error_named(&err.ctx, input, "config.kdl");
    let diag: &dyn Diagnostic = report.as_ref();
    let labels: Vec<_> = diag.labels().map(|it| it.collect()).unwrap_or_default();
    assert!(
        !labels.is_empty(),
        "expected at least primary label, got {labels:?}"
    );
    // Help includes line:column (Glaze source_info).
    let help = diag.help().map(|h| h.to_string());
    assert!(
        help.as_deref().is_some_and(|h| h.contains(':')),
        "help={help:?}"
    );
}
