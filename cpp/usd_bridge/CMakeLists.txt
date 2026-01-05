# USD Bridge - CMake Build Configuration
#
# Builds a static library that wraps Pixar's USD C++ API with a C interface.
# Uses vcpkg for dependency management.

cmake_minimum_required(VERSION 3.20)
project(usd_bridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find USD package (installed via vcpkg)
find_package(pxr REQUIRED)

# Create static library
add_library(usd_bridge STATIC
    usd_bridge.cpp
    usd_bridge.h
)

# Include directories
target_include_directories(usd_bridge PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Link USD libraries
# The pxr package provides these targets
target_link_libraries(usd_bridge PUBLIC
    usd
    usdGeom
    sdf
    tf
    gf
    vt
)

# Export compile commands for IDE support
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Install rules (optional, for standalone builds)
install(TARGETS usd_bridge
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
    RUNTIME DESTINATION bin
)

install(FILES usd_bridge.h
    DESTINATION include
)
