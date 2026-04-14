# BIF Qt Environment Setup
#
# Run this script before building/running bif_qt_spike or bif_qt:
#   . .\setup_qt_env.ps1
#
# Or source it alongside setup_usd_env.ps1 in your PowerShell profile.
#
# Qt 6.8.3 LTS — installed via official Qt Online Installer.
# Binding: cxx-qt 0.7.x (see wiki/architecture/adr/006-qt-via-cxx-qt.md).

$QtRoot = "C:\Qt\6.8.3\msvc2022_64"
$QtTools = "C:\Qt\Tools"

if (-not (Test-Path "$QtRoot\bin\qmake.exe")) {
    Write-Warning "Qt not found at $QtRoot. Install Qt 6.8 LTS + MSVC 2022 64-bit component."
    return
}

# Core Qt env vars consumed by cxx-qt-build + qt-build-utils
$env:Qt6_DIR = "$QtRoot\lib\cmake\Qt6"
$env:QT_DIR = $QtRoot
$env:CMAKE_PREFIX_PATH = "$QtRoot;$env:CMAKE_PREFIX_PATH"

# Qt DLLs (Widgets/Gui/Core) + deploy tools on PATH
$env:PATH = "$QtRoot\bin;$QtTools\CMake_64\bin;$QtTools\Ninja;$env:PATH"

# Qt plugin path so QPA / imageformats / styles resolve at runtime
$env:QT_PLUGIN_PATH = "$QtRoot\plugins"
$env:QML2_IMPORT_PATH = "$QtRoot\qml"

Write-Host "Qt 6.8.3 LTS env loaded:" -ForegroundColor Green
Write-Host "  Qt6_DIR           = $env:Qt6_DIR"
Write-Host "  CMAKE_PREFIX_PATH = $QtRoot"
Write-Host "  qmake             = $(& "$QtRoot\bin\qmake.exe" -query QT_VERSION)"
