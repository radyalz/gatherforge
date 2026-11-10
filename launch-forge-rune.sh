#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
export PATH="$ROOT/tools/bin:$PATH"
exec "$ROOT/GatherForge-Rune-Linux" "$@"
