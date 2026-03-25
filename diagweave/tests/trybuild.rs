#[test]
fn macro_compile_failures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/visibility/*.rs");
    t.compile_fail("tests/ui/display/*.rs");
    t.compile_fail("tests/ui/derive/*.rs");
    t.compile_fail("tests/ui/set-algebra/*.rs");
    t.pass("tests/ui-pass/*.rs");
}
