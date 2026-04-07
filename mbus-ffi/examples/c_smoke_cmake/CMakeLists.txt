cmake_minimum_required(VERSION 3.10)
project(mbus_ffi_c_smoke C)

set(CMAKE_C_STANDARD 11)
include(CTest)
find_package(Threads REQUIRED)

# Point to the generated header
include_directories(../../include)

# Path to the Rust library (dev profile)
set(RUST_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/debug")

# Note: on macOS this is libmbus_ffi.dylib, on Linux libmbus_ffi.so, Windows mbus_ffi.dll.
# CMake handles this via find_library if we give it the name without prefix/suffix.
find_library(MBUS_FFI_LIB
    NAMES mbus_ffi
    PATHS ${RUST_LIB_DIR}
    REQUIRED
)

add_executable(c_smoke_test main.c)
target_link_libraries(c_smoke_test PRIVATE ${MBUS_FFI_LIB} Threads::Threads)

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_libraries(c_smoke_test PRIVATE util)
endif()

if(BUILD_TESTING)
    add_test(NAME c_smoke_serial_pty COMMAND c_smoke_test --serial-pty)
    set_tests_properties(c_smoke_serial_pty PROPERTIES TIMEOUT 10)
endif()
