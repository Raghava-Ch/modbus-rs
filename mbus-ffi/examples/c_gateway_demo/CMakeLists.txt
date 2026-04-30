cmake_minimum_required(VERSION 3.10)
project(mbus_ffi_c_gateway_demo C)

set(CMAKE_C_STANDARD 11)

# Generated header lives under target/mbus-ffi/include
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../../../target/mbus-ffi/include)

# Path to the Rust library (dev profile)
set(RUST_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/debug")

find_package(Threads REQUIRED)

find_library(MBUS_FFI_LIB
    NAMES mbus_ffi
    PATHS ${RUST_LIB_DIR}
    REQUIRED
    NO_DEFAULT_PATH
)

add_executable(c_gateway_demo main.c)
target_link_libraries(c_gateway_demo PRIVATE ${MBUS_FFI_LIB} Threads::Threads)

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_libraries(c_gateway_demo PRIVATE m)
endif()
