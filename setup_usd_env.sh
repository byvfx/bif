#!/bin/bash
# BIF USD Environment Setup (Linux/macOS)
#
# Source this script before using USD features:
#   source ./setup_usd_env.sh
#
# Or add to your .bashrc/.zshrc for permanent setup.

VCPKG_ROOT="${VCPKG_ROOT:-/opt/vcpkg}"

# Detect triplet
if [[ "$(uname)" == "Darwin" ]]; then
    TRIPLET="x64-osx"
else
    TRIPLET="x64-linux"
fi

USD_LIB_PATH="$VCPKG_ROOT/installed/$TRIPLET/lib"
USD_BIN_PATH="$VCPKG_ROOT/installed/$TRIPLET/bin"
USD_TOOLS_PATH="$VCPKG_ROOT/installed/$TRIPLET/tools/usd"

# Add vcpkg lib to library path
export LD_LIBRARY_PATH="$USD_LIB_PATH:${LD_LIBRARY_PATH:-}"
if [[ "$(uname)" == "Darwin" ]]; then
    export DYLD_LIBRARY_PATH="$USD_LIB_PATH:${DYLD_LIBRARY_PATH:-}"
fi

# Add vcpkg bin to PATH for tools
export PATH="$USD_BIN_PATH:$USD_TOOLS_PATH:$PATH"

# Set VCPKG_ROOT for build.rs
export VCPKG_ROOT

# Set USD plugin path (required for USD to find its plugins)
# Scan both bin/usd and lib/usd — usdMtlx (MaterialX) may land in either location
PXR_PLUGINS=""
for base in "$USD_BIN_PATH/usd" "$USD_LIB_PATH/usd"; do
    if [ -d "$base" ]; then
        for dir in "$base"/*/; do
            if [ -d "${dir}resources" ]; then
                PXR_PLUGINS="${PXR_PLUGINS:+$PXR_PLUGINS:}${dir}resources"
            fi
        done
    fi
done
export PXR_PLUGINPATH_NAME="$PXR_PLUGINS"

# OIDN (optional)
if [ -n "$OIDN_DIR" ] && [ -d "$OIDN_DIR/lib" ]; then
    export LD_LIBRARY_PATH="$OIDN_DIR/lib:$LD_LIBRARY_PATH"
    export PATH="$OIDN_DIR/bin:$PATH"
    echo "  OIDN_DIR   = $OIDN_DIR"
fi

echo "USD environment configured:"
echo "  VCPKG_ROOT = $VCPKG_ROOT"
echo "  TRIPLET    = $TRIPLET"
echo "  LD_LIBRARY_PATH includes USD libs"
PLUGIN_COUNT=$(echo "$PXR_PLUGINS" | tr ':' '\n' | grep -c . || true)
echo "  PXR_PLUGINPATH_NAME set with $PLUGIN_COUNT plugin directories"
