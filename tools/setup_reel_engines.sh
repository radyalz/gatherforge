#!/usr/bin/env bash
# GatherForge native AI runtime bootstrap. Builds lightweight CLI engines outside Git.
# Does not install weights, modify media, use sudo, or change Rust source.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---speech}"
case "$MODE" in --speech|--vision|--all|--check) ;; *) echo 'Use --speech | --vision | --all | --check' >&2; exit 2;; esac
BIN="$ROOT/tools/bin"
SRC="$ROOT/tools/runtime-sources"
mkdir -p "$BIN" "$SRC"
check_tool() { local n="$1"; if [[ -x "$BIN/$n" ]]; then echo "$n: $BIN/$n"; elif command -v "$n" >/dev/null; then command -v "$n"; else echo "$n: missing"; fi; }
if [[ "$MODE" == --check ]]; then
 check_tool whisper-cli;check_tool llama-mtmd-cli
 command -v ffmpeg || true;command -v tesseract || true
 exit 0
fi
for t in git cmake c++;do command -v "$t" >/dev/null || { echo "Missing build dependency: $t (install Fedora git, cmake and gcc-c++)" >&2;exit 2;};done
JOBS="${GATHERFORGE_BUILD_JOBS:-2}"
if [[ "$MODE" == --speech || "$MODE" == --all ]];then
 if [[ -x "$BIN/whisper-cli" ]];then echo 'whisper-cli already present';else
  cd "$SRC"
  if [[ ! -d whisper.cpp/.git ]];then git clone --depth 1 --branch v1.9.4 https://github.com/ggml-org/whisper.cpp.git whisper.cpp;fi
  cmake -S whisper.cpp -B whisper.cpp/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON -DGGML_CUDA=OFF
  cmake --build whisper.cpp/build --target whisper-cli --parallel "$JOBS"
  test -x whisper.cpp/build/bin/whisper-cli
  cp whisper.cpp/build/bin/whisper-cli "$BIN/whisper-cli";chmod 755 "$BIN/whisper-cli"
 fi
fi
if [[ "$MODE" == --vision || "$MODE" == --all ]];then
 if [[ -x "$BIN/llama-mtmd-cli" ]];then echo 'llama-mtmd-cli already present';else
  cd "$SRC"
  if [[ ! -d llama.cpp/.git ]];then
   # Pinned release tag. Model compatibility can change; installation is optional.
   git clone --depth 1 --branch v0.9.0 https://github.com/ggml-org/llama.cpp.git llama.cpp
  fi
  cmake -S llama.cpp -B llama.cpp/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_CURL=OFF -DGGML_CUDA=OFF
  cmake --build llama.cpp/build --target llama-mtmd-cli --parallel "$JOBS"
  test -x llama.cpp/build/bin/llama-mtmd-cli
  cp llama.cpp/build/bin/llama-mtmd-cli "$BIN/llama-mtmd-cli";chmod 755 "$BIN/llama-mtmd-cli"
 fi
fi
check_tool whisper-cli;check_tool llama-mtmd-cli
