cmake_minimum_required(VERSION 3.10)
project(mbus_ffi_c_server_demo_yaml C)

set(CMAKE_C_STANDARD 11)
include(CTest)
find_package(Threads REQUIRED)

# Canonical YAML config for generated handlers:
#   mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml

include_directories(../../include)

set(RUST_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/debug")

# Always link the Rust engine as a static archive.
set(_saved_suffixes ${CMAKE_FIND_LIBRARY_SUFFIXES})
set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")
find_library(MBUS_FFI_LIB
    NAMES mbus_ffi
    PATHS ${RUST_LIB_DIR}
    REQUIRED
)
set(CMAKE_FIND_LIBRARY_SUFFIXES ${_saved_suffixes})

add_executable(c_server_demo_yaml main.c)
target_link_libraries(c_server_demo_yaml PRIVATE ${MBUS_FFI_LIB} Threads::Threads)

if(BUILD_TESTING)
    add_test(NAME c_server_demo_yaml_test COMMAND c_server_demo_yaml --self-test)
    set_tests_properties(c_server_demo_yaml_test PROPERTIES TIMEOUT 10)
endif()
