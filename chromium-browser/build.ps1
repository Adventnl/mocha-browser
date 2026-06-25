<#
  Build the CEF sample browsers (cefclient, cefsimple) and, if generated, the
  rebranded "mocha_chromium", using the CMake that ships in the CEF distribution
  downloaded by .\setup.ps1.

  Run this from a "x64 Native Tools Command Prompt for VS" (so cl.exe is on PATH),
  or just have Visual Studio 2022 + CMake installed.
#>
$ErrorActionPreference = "Stop"

$Dir   = $PSScriptRoot
$Cef   = Join-Path $Dir "cef"
$Build = Join-Path $Cef "build"

if (-not (Test-Path (Join-Path $Cef "CMakeLists.txt"))) { throw "CEF not found. Run .\setup.ps1 first." }

Write-Host "Configuring (Visual Studio 2022, x64) ..."
# If you have a different VS version, change the generator (e.g. "Visual Studio 16 2019").
cmake -S $Cef -B $Build -G "Visual Studio 17 2022" -A x64

Write-Host "Building (Release) ..."
cmake --build $Build --config Release

Write-Host ""
Write-Host "Built. Run a full browser (URL bar + tabs):"
Write-Host "  $Build\tests\cefclient\Release\cefclient.exe"
