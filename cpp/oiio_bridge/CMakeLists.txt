# OIIO Bridge - CMake Build Configuration
#
# Builds a static library that wraps OpenImageIO with a C interface.
# Uses vcpkg for dependency management.

cmake_minimum_required(VERSION 3.20)
project(oiio_bridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find OpenImageIO package (installed via vcpkg)
find_package(OpenImageIO CONFIG REQUIRED)

# Create static library
add_library(oiio_bridge STATIC
    oiio_bridge.cpp
    oiio_bridge.h
)

# Include directories
target_include_directories(oiio_bridge PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Link OpenImageIO
target_link_libraries(oiio_bridge PUBLIC
    OpenImageIO::OpenImageIO
)

# Windows-specific: ensure we use static runtime to match Rust
if(MSVC)
    set_property(TARGET oiio_bridge PROPERTY
        MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()

# Export compile commands for IDE support
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Install rules (optional, for standalone builds)
install(TARGETS oiio_bridge
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
    RUNTIME DESTINATION bin
)

install(FILES oiio_bridge.h
    DESTINATION include
)
