//! Compile-fail coverage for `#[derive(Decode)]` / `#[derive(Encode)]`
//! attribute and shape errors (`docs/kdl/design.md` §12).

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
