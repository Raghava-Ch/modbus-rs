#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_holding_registers.rs");
    t.compile_fail("tests/ui/fail_coils_duplicate.rs");
    t.compile_fail("tests/ui/fail_modbus_app_unsorted_maps.rs");
}
