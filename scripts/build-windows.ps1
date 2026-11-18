$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    dotnet build src/AndroidEmu/AndroidEmu.csproj -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Windows application build failed.' }
    dotnet build tests/Verification/Verification.csproj -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Verification build failed.' }
    & ./tests/Verification/bin/Release/net48/Verification.exe
    if ($LASTEXITCODE -ne 0) { throw 'Verification failed.' }
    $dist = New-Item -ItemType Directory -Force dist
    $exe = Join-Path $dist.FullName 'android-emu.exe'
    Copy-Item -LiteralPath src/AndroidEmu/bin/Release/net48/android-emu.exe -Destination $exe -Force
    $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $dist.FullName 'SHA256SUMS.txt') -Encoding ascii -Value "$hash *android-emu.exe"
    Write-Host "Release: $exe"
}
finally {
    Pop-Location
}
