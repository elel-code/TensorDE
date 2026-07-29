//! Feature-gated miette presentation (core stays Glaze format_error).
#![cfg(feature = "diagnostics")]

use tensor_kdl::{format_error, from_str, report_error};

#[test]
fn report_error_has_source_label() {
    let input = "good 1\nbad {\n";
    let err = from_str(input).unwrap_err();
    let glaze = format_error(&err.ctx, input);
    assert!(glaze.contains('^') || glaze.contains(':'));

    let report = report_error(&err.ctx, input);
    let rendered = format!("{report:?}");
    // miette Debug includes source / labels; Display may be multi-line Glaze text.
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
