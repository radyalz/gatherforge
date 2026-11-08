#!/usr/bin/env bash
# Deploy verified Rust source into an existing GatherForge repo and publish a normal Git commit.
set -euo pipefail
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then echo "Usage: bash scripts/install_into_repo.sh /path/to/repo" >&2; exit 2; fi
cd "$TARGET"
[[ "$(git rev-parse --show-toplevel)" == "$PWD" ]] || { echo 'Not at Git root' >&2; exit 1; }
[[ "$(git branch --show-current)" == main ]] || { echo 'Expected main branch' >&2; exit 1; }
[[ "$(git remote get-url origin)" == *radyalz/gatherforge* ]] || { echo 'Wrong remote; refusing' >&2; exit 1; }
git diff --cached --quiet || { echo 'Staged changes detected; stop before migration' >&2; exit 1; }
# Earlier prototype left untracked Rust source and appended Git exclusions. Back those up.
for path in $(git diff --name-only); do
  [[ "$path" == .gitignore ]] || { echo "Uncommitted tracked file: $path; stop and review" >&2; exit 1; }
done
[[ "$SOURCE" != "$PWD" ]] || { echo 'Source must be outside the destination repo' >&2; exit 1; }
command -v cargo >/dev/null || { echo 'Cargo missing; run: source "$HOME/.cargo/env"' >&2; exit 1; }
command -v python3 >/dev/null || { echo 'Python 3 is needed for safe source copy checks' >&2; exit 1; }
git fetch origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || { echo 'Local branch differs from GitHub; stop' >&2; exit 1; }
[[ -z "$(git tag -l v3.2.0)" ]] || { echo 'v3.2.0 already tagged; stop' >&2; exit 1; }
# Compile the extracted source BEFORE touching the user's Git checkout.
(cd "$SOURCE" && cargo fmt --all && CARGO_TARGET_DIR="$TARGET/target" cargo test && CARGO_TARGET_DIR="$TARGET/target" cargo build --release)
backup="$HOME/GatherForge-migration-backup-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$backup"
for path in Cargo.toml Cargo.lock src .gitignore; do
  if [[ -e "$path" ]]; then cp -a -- "$path" "$backup/"; fi
done
printf 'Backed up previous local Rust prototype to: %s\n' "$backup"
# Replace only known implementation files. Do not delete music or private working folders.
for file in main.go downloader.go downloader_test.go desktop_cli.go go.mod go.sum GatherForge; do
  if git ls-files --error-unmatch "$file" >/dev/null 2>&1; then git rm -- "$file"; fi
done
if git ls-files -- desktop | grep -q .; then git rm -r -- desktop; fi
for file in scripts/build.sh scripts/build.ps1 scripts/build-core.sh scripts/build-desktop.py scripts/install-linux-user.sh .github/workflows/desktop-build.yml; do
  if git ls-files --error-unmatch "$file" >/dev/null 2>&1; then git rm -- "$file"; fi
done
mkdir -p src/components docs assets scripts .github/workflows
for path in Cargo.toml .gitignore README.md PRIVACY.md; do cp -- "$SOURCE/$path" "$path"; done
for path in src/main.rs src/app.rs src/downloader.rs src/library.rs src/model.rs src/theme.rs src/util.rs \
  src/components/mod.rs src/components/cards.rs src/components/chrome.rs src/components/controls.rs src/components/tracks.rs; do
  mkdir -p "$(dirname "$path")"; cp -- "$SOURCE/$path" "$path";
done
for path in docs/DESIGN_SYSTEM.md docs/COMPONENTS.md docs/SAFETY_AND_RELEASE.md docs/REFERENCES.md \
  .claude/skills/gatherforge-ui/SKILL.md assets/gatherforge.svg .github/workflows/build.yml scripts/package_release.py scripts/check-public.sh scripts/install_into_repo.sh; do
  mkdir -p "$(dirname "$path")"; cp -- "$SOURCE/$path" "$path"
done
chmod +x scripts/*.sh
if [[ -f "$SOURCE/Cargo.lock" ]]; then cp -- "$SOURCE/Cargo.lock" Cargo.lock; fi
# Revalidate the destination after copy.
cargo fmt --all
cargo test
cargo build --release
if [[ "$(uname -s)" == Linux ]]; then
  cp -- target/release/gatherforge ./GatherForge-Rust-Linux
  chmod +x ./GatherForge-Rust-Linux
fi
# Explicitly stage known source paths only, never a broad `git add .`.
git add -- Cargo.toml .gitignore README.md PRIVACY.md src/main.rs src/app.rs src/downloader.rs src/library.rs \
  src/model.rs src/theme.rs src/util.rs src/components/mod.rs src/components/cards.rs src/components/chrome.rs \
  src/components/controls.rs src/components/tracks.rs docs/DESIGN_SYSTEM.md docs/COMPONENTS.md \
  docs/SAFETY_AND_RELEASE.md docs/REFERENCES.md .claude/skills/gatherforge-ui/SKILL.md assets/gatherforge.svg \
  .github/workflows/build.yml scripts/package_release.py scripts/check-public.sh scripts/install_into_repo.sh
if [[ -f Cargo.lock ]]; then git add -- Cargo.lock; fi
bash scripts/check-public.sh
git diff --cached --check
[[ "$(git config --local --get user.email || true)" == *users.noreply.github.com ]] || {
  echo 'Set your real verified GitHub no-reply address locally before committing.' >&2
  echo 'Source copied and built; nothing pushed.' >&2; exit 1;
}
git config --local user.name 'GatherForge Maintainer'
git commit -m 'Release GatherForge 3.2: Collection Studio native Rust design'
git tag -a v3.2.0 -m 'GatherForge v3.2 Collection Studio'
git push origin main --follow-tags
printf '\nMigration complete. Launch: %s/GatherForge-Rust-Linux\n' "$PWD"
