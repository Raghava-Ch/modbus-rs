cmake_minimum_required(VERSION 3.10)
project(mbus_ffi_c_server_demo C)

set(CMAKE_C_STANDARD 11)
include(CTest)
find_package(Threads REQUIRED)

option(MBUS_FFI_LINK_STATIC "Link mbus_ffi as static library" OFF)

# Point to the generated header
include_directories(../../include)

# Path to the Rust library (dev profile)
set(RUST_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/debug")

if(MBUS_FFI_LINK_STATIC)
    set(_MBUS_ORIG_FIND_SUFFIXES ${CMAKE_FIND_LIBRARY_SUFFIXES})
    set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")
endif()

find_library(MBUS_FFI_LIB
    NAMES mbus_ffi
    PATHS ${RUST_LIB_DIR}
    REQUIRED
)

if(MBUS_FFI_LINK_STATIC)
    set(CMAKE_FIND_LIBRARY_SUFFIXES ${_MBUS_ORIG_FIND_SUFFIXES})
endif()

add_executable(c_server_demo main.c)
target_link_libraries(c_server_demo PRIVATE ${MBUS_FFI_LIB} Threads::Threads)

if(BUILD_TESTING)
    add_test(NAME c_server_demo_test COMMAND c_server_demo)
    set_tests_properties(c_server_demo_test PROPERTIES TIMEOUT 10)
endif()
