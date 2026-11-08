#!/usr/bin/env bash
set -euo pipefail
# Only inspect staged additions. Do not scan the private working folder.
items=$(git diff --cached --name-only --diff-filter=ACMR)
if [[ -z "$items" ]]; then echo 'No staged files to check'; exit 0; fi
if printf '%s\n' "$items" | grep -Ei '(^|/)(xxx|_Incoming|\.env|cookies|target|\.git)(/|$)|\.(mp3|m4a|flac|wav|ogg|opus|xlsx|part)$'; then
  echo 'ERROR: private media, state or build artifacts are staged' >&2
  exit 1
fi
if git diff --cached --unified=0 -- '*.rs' '*.md' '*.sh' '*.yml' '*.toml' '*.py' ':(exclude)scripts/check-public.sh' \
  | grep '^+' \
  | grep -E '(/home/[^/[:space:]]+/|/Users/[^/[:space:]]+/|/mnt/(shared|data)/|[A-Za-z]:\\Users\\)'; then
  echo 'ERROR: machine-specific path detected in staged content' >&2
  exit 1
fi
echo 'Public-source check passed'
