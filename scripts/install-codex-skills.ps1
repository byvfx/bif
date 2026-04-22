[CmdletBinding()]
param(
    [string]$SourceDir = "agents/generated/codex",
    [string]$TargetDir = "$HOME/.codex/skills",
    [switch]$WhatIf
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$sourceRoot = Join-Path $repoRoot $SourceDir
$targetRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($TargetDir)

if (-not (Test-Path -LiteralPath $sourceRoot)) {
    throw "Source directory not found: $sourceRoot"
}

if (-not (Test-Path -LiteralPath $targetRoot)) {
    if ($WhatIf) {
        Write-Host "Would create $targetRoot"
    } else {
        [void][System.IO.Directory]::CreateDirectory($targetRoot)
    }
}

$skillDirs = Get-ChildItem -LiteralPath $sourceRoot -Directory

foreach ($dir in $skillDirs) {
    $sourceSkill = $dir.FullName
    $targetSkill = Join-Path $targetRoot $dir.Name

    if ($WhatIf) {
        Write-Host "Would sync $sourceSkill -> $targetSkill"
        continue
    }

    if (Test-Path -LiteralPath $targetSkill) {
        Remove-Item -LiteralPath $targetSkill -Recurse -Force
    }

    Copy-Item -LiteralPath $sourceSkill -Destination $targetSkill -Recurse
    Write-Host "Installed $($dir.Name) -> $targetSkill"
}
