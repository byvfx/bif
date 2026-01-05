# BIF USD Environment Setup
#
# Run this script before using USD features:
#   . .\setup_usd_env.ps1
#
# Or source it in your PowerShell profile for permanent setup.

$VcpkgRoot = "D:\__projects\_programming\vcpkg"
$UsdBinPath = "$VcpkgRoot\installed\x64-windows\bin"

# Add vcpkg bin to PATH for USD DLLs
$env:PATH = "$UsdBinPath;$env:PATH"

# Set VCPKG_ROOT for build.rs
$env:VCPKG_ROOT = $VcpkgRoot

# Set USD plugin path (required for USD to find its plugins)
$pluginDirs = Get-ChildItem "$UsdBinPath\usd" -Directory | 
    ForEach-Object { $_.FullName + "\resources" }
$env:PXR_PLUGINPATH_NAME = $pluginDirs -join ";"

Write-Host "USD environment configured:" -ForegroundColor Green
Write-Host "  VCPKG_ROOT = $env:VCPKG_ROOT"
Write-Host "  PATH includes USD DLLs"
Write-Host "  PXR_PLUGINPATH_NAME set with $($pluginDirs.Count) plugin directories"
