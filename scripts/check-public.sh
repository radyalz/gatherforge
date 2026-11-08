#!/usr/bin/env bash
# Guard against accidentally committing personal music, paths or real account URLs.
# This audits the Git index, so run AFTER git add and BEFORE git commit.
set -euo pipefail
cd "$(dirname "$0")/.."
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Not a Git repository. Run this after git init and git add." >&2
  exit 2
fi
failed=0

# Check staged filenames for common runtime artifacts (even if force-added).
if git ls-files --cached | grep -E -i '(^|/)(xxx|_Incoming|\.env|cookies([_-].*)?\.txt|archive\.txt|downloaded\.txt|state\.json)(/|$)|\.(mp3|flac|m4a|mp4|wav|opus|xlsx|sqlite3?|db|pem|p12|pfx|key|log|exe|dll|so|dylib)$'; then
  echo "ERROR: one or more staged files look like personal data or local artifacts." >&2
  failed=1
fi

# Common absolute home-directory / mount-point leaks. Skip this script itself
# because it documents the patterns it checks.
if git grep --cached -n -I -E \
  -e '/mnt/shared/' \
  -e '/home/[^/[:space:]<>]+' \
  -e '/Users/[^/[:space:]<>]+' \
  -e '[A-Za-z]:\\Users\\[^\\[:space:]<>]+' \
  -- . ':!scripts/check-public.sh' ':!PRIVACY.md'; then
  echo "ERROR: staged source contains a probable absolute local path." >&2
  failed=1
fi

# SoundCloud likes are often private-account identifiers. Generic docs/UI
# placeholders are allowed; real profile handles should be local config only.
while IFS= read -r line; do
  if [[ "$line" =~ soundcloud\.com/(USERNAME|USER|EXAMPLE)/likes ]]; then
    continue
  fi
  echo "$line"
  failed=1
done < <(git grep --cached -n -I -E 'soundcloud[.]com/[^[:space:]"<]+/likes' -- . ':!scripts/check-public.sh' || true)
if [[ "$failed" -ne 0 ]]; then
  echo "Public-source check FAILED. Fix the staged data before committing." >&2
  exit 1
fi
echo "Public-source check passed (staged files only; review Git author metadata separately)."
