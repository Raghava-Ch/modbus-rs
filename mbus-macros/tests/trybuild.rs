#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_holding_registers.rs");
    t.pass("tests/ui/pass_input_registers.rs");
    t.pass("tests/ui/pass_input_registers_allow_gaps.rs");
    t.pass("tests/ui/pass_modbus_app_fifo_file_record_traits.rs");
    t.compile_fail("tests/ui/fail_coils_duplicate.rs");
    t.compile_fail("tests/ui/fail_discrete_inputs_duplicate.rs");
    t.compile_fail("tests/ui/fail_input_registers_duplicate.rs");
    t.compile_fail("tests/ui/fail_input_registers_non_contiguous.rs");
    t.pass("tests/ui/pass_modbus_app_fifo_routing.rs");
    t.pass("tests/ui/pass_modbus_app_file_record_routing.rs");
    t.compile_fail("tests/ui/fail_modbus_app_fifo_group_unsupported.rs");
    t.compile_fail("tests/ui/fail_modbus_app_file_record_group_unsupported.rs");
    t.compile_fail("tests/ui/fail_modbus_app_unsorted_maps.rs");
    t.compile_fail("tests/ui/fail_notify_via_batch_without_on_batch_write.rs");
    t.compile_fail("tests/ui/fail_duplicate_on_write.rs");
    t.compile_fail("tests/ui/fail_on_write_unmapped_address.rs");
    t.compile_fail("tests/ui/fail_on_write_unmapped_coil_address.rs");
    t.compile_fail("tests/ui/fail_on_write_bad_signature.rs");
}
