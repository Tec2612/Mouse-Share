#Requires -Version 5.1
<#
  One-shot bootstrap for building Mouse Share on Windows: detects every
  missing prerequisite (Rust, the MSVC C++ build tools Rust needs to link,
  Node.js) and installs each one automatically via winget, then builds
  the daemon and the UI. Safe to re-run — every step first checks whether
  it's already satisfied and skips itself if so.

  Usage (from any directory):
    powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
  or just double-click scripts\setup-windows.bat.
#>

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg) { Write-Host "    OK: $msg" -ForegroundColor Green }
function Write-Skip($msg) { Write-Host "    already present: $msg" -ForegroundColor DarkGray }

function Test-CommandExists($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

# Installers (winget, VS Build Tools, rustup, npm packages) write PATH to
# the registry, but this already-running PowerShell process doesn't pick
# that up automatically — re-reading both PATH scopes after each install
# is what lets the rest of this same script see newly installed tools
# without asking the user to close and reopen their terminal.
function Update-SessionPath {
    $machine = [System.Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [System.Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = "$machine;$user"
}

function Test-Winget {
    if (-not (Test-CommandExists "winget")) {
        Write-Host @"

winget (the Windows Package Manager) was not found. It ships with
Windows 10 (2004+) and Windows 11 by default; if it's missing here your
Windows may be out of date. Install "App Installer" from the Microsoft
Store, then re-run this script:
  https://apps.microsoft.com/detail/9nblggh4nns1
"@ -ForegroundColor Yellow
        exit 1
    }
}

Write-Step "Checking for Rust (cargo/rustc)"
if (Test-CommandExists "cargo") {
    Write-Skip "cargo -> $((Get-Command cargo).Source)"
} else {
    Write-Host "    Rust not found; downloading and installing via rustup (non-interactive, default profile)..."
    $rustupInit = Join-Path $env:TEMP "rustup-init.exe"
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit
    # -y = accept defaults non-interactively; --default-host targets the
    # MSVC toolchain, which is what Windows builds (including this repo's
    # windows-rs based crates) expect.
    & $rustupInit -y --default-host x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "rustup-init failed with exit code $LASTEXITCODE" }
    Update-SessionPath
    if (-not (Test-CommandExists "cargo")) {
        # rustup installs to %USERPROFILE%\.cargo\bin but the registry
        # PATH update can occasionally lag; add it directly as a fallback
        # so the rest of this script still works in the same session.
        $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
        $env:Path = "$cargoBin;$env:Path"
    }
    Write-Ok "Rust installed"
}

Write-Step "Checking for the MSVC C++ build tools (required to link Rust binaries on Windows)"
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasVCTools = $false
if (Test-Path $vswhere) {
    $vcInstall = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $hasVCTools = [bool]$vcInstall
}
if ($hasVCTools) {
    Write-Skip "MSVC C++ build tools"
} else {
    Test-Winget
    Write-Host "    Not found; installing Visual Studio 2022 Build Tools with the C++ workload via winget."
    Write-Host "    This is a multi-hundred-MB download and can take several minutes." -ForegroundColor Yellow
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-source-agreements --accept-package-agreements --silent --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    if ($LASTEXITCODE -ne 0) { throw "winget failed to install the VS Build Tools (exit code $LASTEXITCODE)" }
    Update-SessionPath
    Write-Ok "MSVC C++ build tools installed"
}

Write-Step "Checking for Node.js/npm"
if (Test-CommandExists "npm") {
    Write-Skip "npm -> $((Get-Command npm).Source)"
} else {
    Test-Winget
    Write-Host "    Not found; installing Node.js LTS via winget."
    winget install --id OpenJS.NodeJS.LTS -e --accept-source-agreements --accept-package-agreements --silent
    if ($LASTEXITCODE -ne 0) { throw "winget failed to install Node.js (exit code $LASTEXITCODE)" }
    Update-SessionPath
    Write-Ok "Node.js installed"
}

Write-Step "Building the Rust core + daemon (cargo build --release --workspace)"
Push-Location $RepoRoot
try {
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}
Write-Ok "Rust build complete"

Write-Step "Building the desktop UI (npm install + tauri build)"
Push-Location (Join-Path $RepoRoot "app")
try {
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
} finally {
    Pop-Location
}

Write-Step "Done"
$bundleDir = Join-Path $RepoRoot "app\src-tauri\target\release\bundle"
Write-Host "Installer(s) should be under:" -ForegroundColor Green
Write-Host "  $bundleDir"
if (Test-Path $bundleDir) {
    Get-ChildItem -Path $bundleDir -Recurse -Include *.exe,*.msi | ForEach-Object { Write-Host "  - $($_.FullName)" }
}
