# BIF USD Environment Setup
#
# Run this script before using USD features:
#   . .\setup_usd_env.ps1
#
# Or source it in your PowerShell profile for permanent setup.

$VcpkgRoot = "D:\__projects\_programming\vcpkg"
$UsdBinPath = "$VcpkgRoot\installed\x64-windows\bin"
$OidnRoot = "D:\__projects\_programming\oidn-2.4.1.x64.windows"

# Add vcpkg bin and OIDN bin to PATH for DLLs
$env:PATH = "$UsdBinPath;$OidnRoot\bin;$env:PATH"

# Set OIDN_DIR for oidn crate build.rs
$env:OIDN_DIR = $OidnRoot

# Set VCPKG_ROOT for build.rs
$env:VCPKG_ROOT = $VcpkgRoot

# Set USD plugin path (required for USD to find its plugins)
# Scan both bin/usd and lib/usd — usdMtlx (MaterialX) may land in either location
$UsdLibPath = "$VcpkgRoot\installed\x64-windows\lib"
$pluginDirs = @()
$pluginDirs += Get-ChildItem "$UsdBinPath\usd" -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { $_.FullName + "\resources" }
$pluginDirs += Get-ChildItem "$UsdLibPath\usd" -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { $_.FullName + "\resources" }
$pluginDirs = $pluginDirs | Select-Object -Unique
$env:PXR_PLUGINPATH_NAME = $pluginDirs -join ";"

Write-Host "USD + OIDN environment configured:" -ForegroundColor Green
Write-Host "  VCPKG_ROOT = $env:VCPKG_ROOT"
Write-Host "  OIDN_DIR   = $env:OIDN_DIR"
Write-Host "  PATH includes USD + OIDN DLLs"
Write-Host "  PXR_PLUGINPATH_NAME set with $($pluginDirs.Count) plugin directories"
