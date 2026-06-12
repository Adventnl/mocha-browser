# Build and package Mocha Browser as a runnable Windows folder:
#
#   dist\MochaBrowser\
#     Mocha.exe     (the desktop GUI binary, renamed from mocha_desktop.exe)
#     README.txt
#     LICENSE
#     profile\      (fallback profile dir; the app defaults to %APPDATA%\MochaBrowser)
#     logs\         (placeholder; Mocha does not write log files yet)
#
# Usage (from anywhere):
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
#
# Requirements: the pinned stable Rust toolchain plus a C compiler (MinGW-w64
# gcc on the windows-gnu toolchain) for the bundled SQLite and ring builds.

$ErrorActionPreference = "Stop"

# Always operate from the repository root (this script lives in scripts\).
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    # Resolve cargo: PATH first, then the standard rustup install location.
    $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
    if ($null -ne $cargoCommand) {
        $cargo = $cargoCommand.Source
    } else {
        $cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    }
    if (-not (Test-Path $cargo)) {
        throw "cargo not found on PATH or in $env:USERPROFILE\.cargo\bin"
    }

    & $cargo build --release -p mocha_desktop --features gui
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    Remove-Item -Recurse -Force dist\MochaBrowser -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force dist\MochaBrowser | Out-Null
    New-Item -ItemType Directory -Force dist\MochaBrowser\profile | Out-Null
    New-Item -ItemType Directory -Force dist\MochaBrowser\logs | Out-Null

    Copy-Item target\release\mocha_desktop.exe dist\MochaBrowser\Mocha.exe
    Copy-Item LICENSE dist\MochaBrowser\LICENSE
    Copy-Item README.md dist\MochaBrowser\README.txt

    Write-Output "Built dist\MochaBrowser\Mocha.exe"
    Write-Output "Run it with:"
    Write-Output ".\dist\MochaBrowser\Mocha.exe"
} finally {
    Pop-Location
}
