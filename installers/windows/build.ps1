# Builds the Windows release binaries and compiles the Inno Setup
# installer. Run from anywhere; paths are resolved relative to this
# script. Requires Inno Setup 6 (ISCC.exe) on PATH — already present on
# GitHub's windows-latest runner image; install locally from
# https://jrsoftware.org/isdl.php if building outside CI.

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")

Push-Location $RepoRoot
try {
    Write-Host "==> cargo build --release --workspace"
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    if (Test-Path "$RepoRoot\app") {
        Write-Host "==> Building the Tauri UI (if app/ has been scaffolded with npm dependencies installed)"
        Push-Location "$RepoRoot\app"
        try {
            if (Test-Path "node_modules") {
                npm run tauri build
            } else {
                Write-Warning "app/node_modules not found; skipping UI build (run 'npm install' in app/ first). Packaging the daemon-only installer."
            }
        } finally {
            Pop-Location
        }
    }
} finally {
    Pop-Location
}

$Iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if (-not $Iscc) {
    $CandidatePaths = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    )
    $Iscc = $CandidatePaths | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Iscc) {
    throw "ISCC.exe (Inno Setup 6) not found on PATH or in the default install locations."
}

New-Item -ItemType Directory -Force -Path "$PSScriptRoot\output" | Out-Null

Write-Host "==> Compiling installer with $Iscc"
& $Iscc "$PSScriptRoot\installer.iss"
if ($LASTEXITCODE -ne 0) { throw "ISCC.exe failed" }

Write-Host "==> Done: $PSScriptRoot\output"
