#!/usr/bin/env pwsh
# Debug build script

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
$env:RUST_LOG = "debug"

# ── Build ─────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "[DEBUG] Building voice-to-text-ime..." -ForegroundColor Cyan
cargo build
if ($LASTEXITCODE -ne 0) {
    Write-Host "[FAIL] Debug build failed." -ForegroundColor Red
    exit 1
}

Write-Host "[OK] Debug build succeeded." -ForegroundColor Green
Write-Host "[RUN] Starting application..." -ForegroundColor Cyan
.\target\debug\voice-to-text-ime.exe
