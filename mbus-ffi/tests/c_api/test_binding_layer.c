#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <modbus_rs_client.h>

static int tests_passed = 0;
static int tests_failed = 0;

#define ASSERT(condition, msg) do { \
    if (!(condition)) { \
        printf("    FAIL: %s\n", msg); \
        tests_failed++; \
    } else { \
        printf("    PASS: %s\n", msg); \
        tests_passed++; \
    } \
} while (0)

#define ASSERT_EQ(actual, expected, msg) do { \
    if ((actual) != (expected)) { \
        printf("    FAIL: %s (expected %d, got %d)\n", msg, expected, actual); \
        tests_failed++; \
    } else { \
        printf("    PASS: %s\n", msg); \
        tests_passed++; \
    } \
} while (0)

#define ASSERT_NOT_NULL(ptr, msg) do { \
    if ((ptr) == NULL) { \
        printf("    FAIL: %s (pointer is NULL)\n", msg); \
        tests_failed++; \
    } else { \
        printf("    PASS: %s\n", msg); \
        tests_passed++; \
    } \
} while (0)

#define ASSERT_NULL(ptr, msg) do { \
    if  ((ptr) != NULL) { \
        printf("    FAIL: %s (pointer is not NULL)\n", msg); \
        tests_failed++; \
    } else { \
        printf("    PASS: %s\n", msg); \
        tests_passed++; \
    } \
} while (0)

#define ASSERT_ERROR(code, msg) do { \
    if ((code) == MbusOk) { \
        printf("    FAIL: %s (expected error, got OK)\n", msg); \
        tests_failed++; \
    } else { \
        printf("    PASS: %s\n", msg); \
        tests_passed++; \
    } \
} while (0)

/* Status string test */
void test_status_str(void) {
    printf("test_status_str\n");
    const char *str = mbus_status_str(MbusOk);
    ASSERT_NOT_NULL(str, "mbus_status_str(MbusOk) returns non-null");
    ASSERT(strlen(str) > 0, "mbus_status_str(MbusOk) returns non-empty");
}

/* Coils accessor tests */
void test_coils_accessors_null(void) {
    printf("test_coils_accessors_null\n");
    bool out_val = false;
    uint16_t addr = mbus_coils_from_address(NULL);
    ASSERT_EQ(addr, 0, "mbus_coils_from_address(NULL) returns 0");

    uint16_t qty = mbus_coils_quantity(NULL);
    ASSERT_EQ(qty, 0, "mbus_coils_quantity(NULL) returns 0");

    enum MbusStatusCode rc = mbus_coils_value(NULL, 0, &out_val);
    ASSERT_ERROR(rc, "mbus_coils_value(NULL) returns error");

    rc = mbus_coils_value_at_index(NULL, 0, &out_val);
    ASSERT_ERROR(rc, "mbus_coils_value_at_index(NULL) returns error");

    const uint8_t *ptr = mbus_coils_values_ptr(NULL);
    ASSERT_NULL(ptr, "mbus_coils_values_ptr(NULL) returns NULL");
}

/* TCP client lifecycle */
void test_tcp_client_lifecycle(void) {
    printf("test_tcp_client_lifecycle\n");
    MbusClientId id = mbus_tcp_client_new(NULL, NULL, NULL);
    ASSERT_EQ(id, MBUS_INVALID_CLIENT_ID, "mbus_tcp_client_new(NULL) returns INVALID_ID");

    enum MbusStatusCode rc = mbus_tcp_connect(MBUS_INVALID_CLIENT_ID);
    ASSERT_ERROR(rc, "mbus_tcp_connect(INVALID_ID) returns error");

    rc = mbus_tcp_disconnect(MBUS_INVALID_CLIENT_ID);
    ASSERT_ERROR(rc, "mbus_tcp_disconnect(INVALID_ID) returns error");

    uint8_t connected = mbus_tcp_is_connected(MBUS_INVALID_CLIENT_ID);
    ASSERT_EQ(connected, 0, "mbus_tcp_is_connected(INVALID_ID) returns 0");

    mbus_tcp_poll(MBUS_INVALID_CLIENT_ID);
    ASSERT(1, "mbus_tcp_poll(INVALID_ID) does not crash");

    mbus_tcp_client_free(MBUS_INVALID_CLIENT_ID);
    ASSERT(1, "mbus_tcp_client_free(INVALID_ID) does not crash");
}

/* Modbus coils operations */
void test_coils_operations(void) {
    printf("test_coils_operations\n");
    enum MbusStatusCode rc = mbus_tcp_read_coils(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_tcp_read_coils(INVALID_ID) returns error");

    rc = mbus_tcp_read_single_coil(MBUS_INVALID_CLIENT_ID, 0, 0, 0);
    ASSERT_ERROR(rc, "mbus_tcp_read_single_coil(INVALID_ID) returns error");

    rc = mbus_tcp_write_single_coil(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_tcp_write_single_coil(INVALID_ID) returns error");

    uint8_t values[] = {0xFF};
    rc = mbus_tcp_write_multiple_coils(MBUS_INVALID_CLIENT_ID, 0, 0, 0, values, 8);
    ASSERT_ERROR(rc, "mbus_tcp_write_multiple_coils(INVALID_ID) returns error");

    rc = mbus_serial_read_coils(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_serial_read_coils(INVALID_ID) returns error");

    rc = mbus_serial_read_single_coil(MBUS_INVALID_CLIENT_ID, 0, 0, 0);
    ASSERT_ERROR(rc, "mbus_serial_read_single_coil(INVALID_ID) returns error");

    rc = mbus_serial_write_single_coil(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_serial_write_single_coil(INVALID_ID) returns error");

    rc = mbus_serial_write_multiple_coils(MBUS_INVALID_CLIENT_ID, 0, 0, 0, values, 8);
    ASSERT_ERROR(rc, "mbus_serial_write_multiple_coils(INVALID_ID) returns error");
}

void test_discrete_inputs_operations(void) {
    printf("test_discrete_inputs_operations\n");
    enum MbusStatusCode rc = mbus_tcp_read_discrete_inputs(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_tcp_read_discrete_inputs(INVALID_ID) returns error");

    rc = mbus_tcp_read_single_discrete_input(MBUS_INVALID_CLIENT_ID, 0, 0, 0);
    ASSERT_ERROR(rc, "mbus_tcp_read_single_discrete_input(INVALID_ID) returns error");

    rc = mbus_serial_read_discrete_inputs(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1);
    ASSERT_ERROR(rc, "mbus_serial_read_discrete_inputs(INVALID_ID) returns error");
}

/* Modbus registers operations */
void test_registers_operations(void) {
    printf("test_registers_operations\n");
    uint16_t values[] = {100};
    enum MbusStatusCode rc = mbus_tcp_write_multiple_registers(MBUS_INVALID_CLIENT_ID, 0, 0, 0, values, 1);
    ASSERT_ERROR(rc, "mbus_tcp_write_multiple_registers(INVALID_ID) returns error");

    uint16_t write_vals[] = {100};
    rc = mbus_tcp_read_write_multiple_registers(MBUS_INVALID_CLIENT_ID, 0, 0, 0, 1, 0, write_vals, 1);
    ASSERT_ERROR(rc, "mbus_tcp_read_write_multiple_registers(INVALID_ID) returns error");

    rc = mbus_serial_write_multiple_registers(MBUS_INVALID_CLIENT_ID, 0, 0, 0, values, 1);
    ASSERT_ERROR(rc, "mbus_serial_write_multiple_registers(INVALID_ID) returns error");
}

/* Modbus FIFO queue operations */
void test_fifo_operations(void) {
    printf("test_fifo_operations\n");
    enum MbusStatusCode rc = mbus_tcp_read_fifo_queue(MBUS_INVALID_CLIENT_ID, 0, 0, 0);
    ASSERT_ERROR(rc, "mbus_tcp_read_fifo_queue(INVALID_ID) returns error");

    rc = mbus_serial_read_fifo_queue(MBUS_INVALID_CLIENT_ID, 0, 0, 0);
    ASSERT_ERROR(rc, "mbus_serial_read_fifo_queue(INVALID_ID) returns error");
}

/* Modbus file record operations */
void test_file_record_operations(void) {
    printf("test_file_record_operations\n");
    struct MbusSubRequest requests[] = {{0, 0, 1}};
    enum MbusStatusCode rc = mbus_tcp_read_file_record(MBUS_INVALID_CLIENT_ID, 0, 0, requests, 1);
    ASSERT_ERROR(rc, "mbus_tcp_read_file_record(INVALID_ID) returns error");

    rc = mbus_tcp_write_file_record(MBUS_INVALID_CLIENT_ID, 0, 0, requests, 1);
    ASSERT_ERROR(rc, "mbus_tcp_write_file_record(INVALID_ID) returns error");

    rc = mbus_serial_read_file_record(MBUS_INVALID_CLIENT_ID, 0, 0, requests, 1);
    ASSERT_ERROR(rc, "mbus_serial_read_file_record(INVALID_ID) returns error");
}

/* Modbus diagnostics operations */
void test_diagnostics_operations(void) {
    printf("test_diagnostics_operations\n");
    uint16_t data[] = {0};
    enum MbusStatusCode rc = mbus_tcp_diagnostics(MBUS_INVALID_CLIENT_ID, 0, 0, 0, data, 1);
    ASSERT_ERROR(rc, "mbus_tcp_diagnostics(INVALID_ID) returns error");

    rc = mbus_serial_diagnostics(MBUS_INVALID_CLIENT_ID, 0, 0, 0, data, 1);
    ASSERT_ERROR(rc, "mbus_serial_diagnostics(INVALID_ID) returns error");
}

/* Modbus device identification operations */
void test_device_id_operations(void) {
    printf("test_device_id_operations\n");
    enum MbusStatusCode rc = mbus_tcp_read_device_identification(MBUS_INVALID_CLIENT_ID, 0, 0, 1, 0);
    ASSERT_ERROR(rc, "mbus_tcp_read_device_identification(INVALID_ID) returns error");

    rc = mbus_serial_read_device_identification(MBUS_INVALID_CLIENT_ID, 0, 0, 1, 0);
    ASSERT_ERROR(rc, "mbus_serial_read_device_identification(INVALID_ID) returns error");
}

/* Serial client lifecycle */
void test_serial_client_lifecycle(void) {
    printf("test_serial_client_lifecycle\n");
    MbusClientId id = mbus_serial_client_new(NULL, NULL, NULL);
    ASSERT_EQ(id, MBUS_INVALID_CLIENT_ID, "mbus_serial_client_new(NULL) returns INVALID_ID");

    enum MbusStatusCode rc = mbus_serial_connect(MBUS_INVALID_CLIENT_ID);
    ASSERT_ERROR(rc, "mbus_serial_connect(INVALID_ID) returns error");

    rc = mbus_serial_disconnect(MBUS_INVALID_CLIENT_ID);
    ASSERT_ERROR(rc, "mbus_serial_disconnect(INVALID_ID) returns error");

    uint8_t connected = mbus_serial_is_connected(MBUS_INVALID_CLIENT_ID);
    ASSERT_EQ(connected, 0, "mbus_serial_is_connected(INVALID_ID) returns 0");

    mbus_serial_poll(MBUS_INVALID_CLIENT_ID);
    ASSERT(1, "mbus_serial_poll(INVALID_ID) does not crash");

    mbus_serial_client_free(MBUS_INVALID_CLIENT_ID);
    ASSERT(1, "mbus_serial_client_free(INVALID_ID) does not crash");
}

/* Discrete inputs and registers accessors */
void test_data_accessors(void) {
    printf("test_data_accessors\n");
    bool out_val = false;
    uint16_t addr = mbus_discrete_inputs_from_address(NULL);
    ASSERT_EQ(addr, 0, "mbus_discrete_inputs_from_address(NULL) returns 0");

    enum MbusStatusCode rc = mbus_discrete_inputs_value(NULL, 0, &out_val);
    ASSERT_ERROR(rc, "mbus_discrete_inputs_value(NULL) returns error");

    uint16_t reg_val = 0;
    addr = mbus_registers_from_address(NULL);
    ASSERT_EQ(addr, 0, "mbus_registers_from_address(NULL) returns 0");

    rc = mbus_registers_value(NULL, 0, &reg_val);
    ASSERT_ERROR(rc, "mbus_registers_value(NULL) returns error");

    addr = mbus_fifo_queue_ptr_address(NULL);
    ASSERT_EQ(addr, 0, "mbus_fifo_queue_ptr_address(NULL) returns 0");

    rc = mbus_fifo_queue_value(NULL, 0, &reg_val);
    ASSERT_ERROR(rc, "mbus_fifo_queue_value(NULL) returns error");
}

int main(void) {
    printf("===== C Binding Layer Integration Tests =====\n\n");

    test_status_str();
    printf("\n");

    test_coils_accessors_null();
    printf("\n");

    test_tcp_client_lifecycle();
    printf("\n");

    test_coils_operations();
    printf("\n");

    test_discrete_inputs_operations();
    printf("\n");

    test_registers_operations();
    printf("\n");

    test_fifo_operations();
    printf("\n");

    test_file_record_operations();
    printf("\n");

    test_diagnostics_operations();
    printf("\n");

    test_device_id_operations();
    printf("\n");

    test_serial_client_lifecycle();
    printf("\n");

    test_data_accessors();
    printf("\n");

    printf("===== Test Summary =====\n");
    printf("Passed: %d\n", tests_passed);
    printf("Failed: %d\n", tests_failed);
    printf("Total:  %d\n", tests_passed + tests_failed);

    return tests_failed == 0 ? 0 : 1;
}