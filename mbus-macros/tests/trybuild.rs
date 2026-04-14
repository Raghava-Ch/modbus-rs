#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_holding_registers.rs");
    t.compile_fail("tests/ui/fail_coils_duplicate.rs");
    t.compile_fail("tests/ui/fail_discrete_inputs_duplicate.rs");
    t.compile_fail("tests/ui/fail_modbus_app_unsorted_maps.rs");
    t.compile_fail("tests/ui/fail_notify_via_batch_without_on_batch_write.rs");
    t.compile_fail("tests/ui/fail_duplicate_on_write.rs");
    t.compile_fail("tests/ui/fail_on_write_unmapped_address.rs");
    t.compile_fail("tests/ui/fail_on_write_unmapped_coil_address.rs");
    t.compile_fail("tests/ui/fail_on_write_bad_signature.rs");
}
