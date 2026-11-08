#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist
GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o dist/GatherForge-Linux-x64 .
GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o dist/GatherForge-Windows-x64.exe .
echo "Built dist/GatherForge-Linux-x64 and dist/GatherForge-Windows-x64.exe"
