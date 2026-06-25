<#
  Download the official CEF (Chromium Embedded Framework) binary distribution
  for Windows into .\cef, so the sample browsers (cefclient / cefsimple) and our
  "Mocha Chromium" rebrand can be built against it.

  This is the real, full Chromium engine (the same one Chrome / Edge ship). We
  download the prebuilt framework the CEF project publishes — we are NOT
  compiling Chromium from source.

  Usage:   .\setup.ps1
           $env:CEF_CHANNEL="beta"; .\setup.ps1
#>
$ErrorActionPreference = "Stop"

$IndexUrl = "https://cef-builds.spotifycdn.com/index.json"
$BaseUrl  = "https://cef-builds.spotifycdn.com"
$Channel  = if ($env:CEF_CHANNEL) { $env:CEF_CHANNEL } else { "stable" }
$Dest     = Join-Path $PSScriptRoot "cef"

# --- Map this machine to a CEF platform key ----------------------------------
$arch = (Get-CimInstance Win32_Processor).Architecture  # 9 = x64, 12 = arm64
switch ($arch) {
  9  { $Platform = "windows64" }
  12 { $Platform = "windowsarm64" }
  default { throw "Unsupported Windows architecture code: $arch" }
}

Write-Host "Platform: $Platform   channel: $Channel"
Write-Host "Fetching CEF build index ..."

$index = Invoke-RestMethod -Uri $IndexUrl
$versions = $index.$Platform.versions
$build = $versions | Where-Object { $_.channel -eq $Channel } | Select-Object -First 1
if (-not $build) { throw "No '$Channel' build found for $Platform" }
$file = $build.files | Where-Object { $_.type -eq "standard" } | Select-Object -First 1
if (-not $file) { throw "No 'standard' archive in the selected build" }

$cefVersion = $build.cef_version
Write-Host "Selected: $($file.name)  (CEF $cefVersion)"

# '+' must be percent-encoded in the download URL.
$encoded = $file.name -replace '\+', '%2B'
$archive = Join-Path $env:TEMP $file.name

if (-not (Test-Path $archive)) {
  Write-Host "Downloading (~1 GB, this takes a while) ..."
  Invoke-WebRequest -Uri "$BaseUrl/$encoded" -OutFile $archive
}

Write-Host "Extracting into $Dest ..."
if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
New-Item -ItemType Directory -Path $Dest | Out-Null

# tar ships with Windows 10+; it extracts .tar.bz2. Strip the top-level dir.
tar -xjf $archive -C $Dest --strip-components=1

Write-Host ""
Write-Host "Done. CEF $cefVersion is in $Dest"
Write-Host "Next:  open a 'x64 Native Tools' prompt and run  .\build.ps1"
