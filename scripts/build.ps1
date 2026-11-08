$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
New-Item -ItemType Directory -Force -Path dist | Out-Null
$env:CGO_ENABLED = '0'
$env:GOOS = 'windows'; $env:GOARCH = 'amd64'
go build -trimpath -ldflags '-s -w' -o dist/GatherForge-Windows-x64.exe .
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:GOOS = 'linux'; $env:GOARCH = 'amd64'
go build -trimpath -ldflags '-s -w' -o dist/GatherForge-Linux-x64 .
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host 'Builds are in dist/'
