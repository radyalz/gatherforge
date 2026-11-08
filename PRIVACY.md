# Privacy checklist for public releases

This repository contains **source code only**, not the contents of the machine on which Git was run.
Git does not upload local terminal history or the filesystem working directory automatically.
However, Git commit metadata and any content that was deliberately staged **are public** after a push.

## Before your first public push

1. In GitHub **Settings > Emails**, enable **Keep my email addresses private** and copy the **exact noreply email** GitHub provides. You may also enable **Block command line pushes that expose my email**.
2. Configure this repository locally (not globally):

   ```bash
   git config --local user.name "GatherForge Maintainer"
   git config --local user.email "YOUR_GITHUB_NOREPLY_EMAIL"
   ```

   Both entries are saved inside your local `.git/config`, **not** in commits.
3. Stage **only source files**. Avoid blindly adding folders containing downloads, reports, cookies, debug logs, browser profiles or local configuration.
4. Run `bash scripts/check-public.sh` to inspect staged files. Review `git diff --cached --stat` and `git diff --cached` as well.
5. Inspect the author and committer of the commit you are about to publish:

   ```bash
   git log -1 --format=fuller
   ```

## If an initial commit has already been published

If the repository has exactly one published commit, no other work is based on it, and you wish to replace its content and commit identity:

```bash
# After changing the tracked files and configuring a noreply Git identity:
git add README.md .gitignore main.go downloader.go downloader_test.go go.mod scripts/ .github/ PRIVACY.md
git diff --cached --stat
bash scripts/check-public.sh
git commit --amend --reset-author -m "Initial public release of GatherForge"
git log -1 --format=fuller
git push --force-with-lease origin main
```

**Do not force push** if collaborators, other branches, tags, pull requests or recent remote commits depend on the old history. Make sure your copy is up to date with `git fetch origin` and `git status -sb` first.
If you have more than one published commit, do **not** assume amending the latest one will remove material from older history. Review the entire history or use purpose-built rewriting tools.

A rewritten Git commit gets a new hash, but the original commit may remain temporarily accessible by hash, in caches, forks or clones. Force pushing is not a guarantee of erasure. If private credentials were exposed, revoke/rotate them; for sensitive material follow GitHub's official sensitive-data removal guidance.

## What this repo should *never* contain

- Actual music/media downloads; private tracks; playlists or listening histories.
- Cookies, credentials, browser profiles, API keys, session tokens and `.env` files.
- Local paths revealing machine usernames, disks or private directories.
- Reports, staged files, logs, download archives, Excel spreadsheets or diagnostic bundles.
- Real personal URLs when generic placeholders suffice.

Application runtime paths may be visible **locally** in the browser UI and logs; they are not automatically committed to this repository. Use per-project `.gitignore` and audit what you stage before every push.

**Limits:** `.gitignore` does not remove files already tracked or clean old commits. The checker finds common accidental leaks, not every possible secret. GitHub owners, commit dates and public activity cannot be hidden by these source changes. Build artifacts compiled without `-trimpath` may contain build paths; provided build scripts use `-trimpath`.
