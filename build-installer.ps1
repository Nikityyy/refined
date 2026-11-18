param([string]$Iscc)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    if (-not $Iscc) {
        $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($command) { $Iscc = $command.Source }
        else {
            $Iscc = @(
                "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
                "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
                "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
                "$env:ProgramFiles\Inno Setup 7\ISCC.exe"
            ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        }
    }
    if (-not $Iscc) { throw 'Install Inno Setup 6.7.4 or newer, or pass -Iscc with the path to ISCC.exe.' }
    $manifest = Get-Content Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"').Groups[1].Value
    if (-not $version) { throw 'Could not read the version from Cargo.toml.' }
    Write-Host "Building Refined release binary (v$version)..." -ForegroundColor Cyan
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    Write-Host "Compiling Inno Setup installer..." -ForegroundColor Cyan
    & $Iscc "/DAppVersion=$version" installer\refined.iss
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
    $output = Join-Path $PSScriptRoot "target\installer\Refined-Setup-$version.exe"
    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($output))" | Set-Content -Encoding ascii "$output.sha256"
    Write-Host "Installer created successfully:" -ForegroundColor Green
    Write-Output $output
} finally {
    Pop-Location
}
