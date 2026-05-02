#!/usr/bin/env pwsh
# Release build script

$ErrorActionPreference = "Stop"

# ── Load MSVC environment (required by nvcc / CUDA build) ────────────────────
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vsWhere) {
    $vsPath = & $vsWhere -latest -property installationPath 2>$null
    if ($vsPath) {
        $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
        if (Test-Path $vcvars) {
            Write-Host "[ENV] Loading MSVC environment from $vcvars" -ForegroundColor DarkGray
            $envLines = cmd /c "`"$vcvars`" > nul 2>&1 && set" 2>$null
            foreach ($line in $envLines) {
                if ($line -match "^([^=]+)=(.*)$") {
                    [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
                }
            }
            Write-Host "[ENV] MSVC ready." -ForegroundColor DarkGray
        } else {
            Write-Warning "vcvars64.bat not found at $vcvars"
        }
    }
} else {
    Write-Warning "vswhere.exe not found — CUDA build may fail if cl.exe is not in PATH"
}

# ── Cargo PATH ────────────────────────────────────────────────────────────────
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# ── Build ─────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "[RELEASE] Building voice-to-text-ime (optimized)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[FAIL] Release build failed." -ForegroundColor Red
    exit 1
}

Write-Host "[OK] Release build succeeded." -ForegroundColor Green
Write-Host "[BIN] Output: target\release\voice-to-text-ime.exe" -ForegroundColor Yellow

$run = Read-Host "Run now? (y/N)"
if ($run -eq 'y' -or $run -eq 'Y') {
    Write-Host "[RUN] Starting application..." -ForegroundColor Cyan
    .\target\release\voice-to-text-ime.exe
}
