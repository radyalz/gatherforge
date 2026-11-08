# Safe release and migration rules

## File pipeline

1. Scan has read-only intent except creating GatherForge-owned state.
2. Review artist assignments and potential duplicates. A title match does not prove identical audio.
3. Stage moves originals to `xxx/_staging` without deleting them.
4. Publish creates copies in artist directories and verifies complete bytes.
5. Finalize is separate, explicitly confirmed and verified before any staged copy removal.
6. Restore never overwrites existing originals.
7. Never delete files recursively outside a precisely owned workspace, nor remove the entire `xxx` directory as a cleanup shortcut.

## Git / machine privacy

- Do not commit `target/`, `xxx/`, `_Incoming/`, audio content, .env, cookies or downloaded executables.
- Use `git ls-files` before removing legacy source. Preserve unrelated files and `.git/`.
- `git status --short` and `git diff --cached --check` are mandatory before commit.
- Configure local Git author identity with a verified GitHub no-reply email.
- `main` is the shipping branch; no forced push required for the migration.
- GitHub build jobs must run source tests, not just compile a binary.

## Layout reference policy

Design reference links are cited in `docs/REFERENCES.md`. They are used for abstract style principles only; do not download their assets into this repository.
