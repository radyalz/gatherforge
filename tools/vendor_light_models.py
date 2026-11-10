#!/usr/bin/env python3
"""Maintainer/installer bootstrap: vendor verified small model files into repo/models/bundled.
Runtime management is native Rust, not this script. Safe to rerun. No API keys.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / 'models/catalog.json'
BUNDLED = ROOT / 'models/bundled'
ALLOWED = {'huggingface.co', 'raw.githubusercontent.com'}

def digest(path, entry):
    h = hashlib.sha256() if entry['hash_type'] == 'sha256' else hashlib.sha1()
    if entry['hash_type'] == 'git_blob_sha1':
        h.update(f'blob {path.stat().st_size}\0'.encode())
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(128 * 1024), b''):
            h.update(chunk)
    return h.hexdigest() == entry['hash']

def vendor(entry):
    from urllib.parse import urlsplit
    src = entry['url']
    if urlsplit(src).scheme != 'https' or urlsplit(src).hostname not in ALLOWED:
        raise ValueError('Unapproved source')
    rel = Path(entry['relative_path'])
    if rel.is_absolute() or '..' in rel.parts:
        raise ValueError('Invalid relative path')
    dest = BUNDLED / rel
    if dest.is_symlink() or dest.parent.is_symlink():
        raise ValueError('Refusing symlink destination')
    if dest.is_file():
        if digest(dest, entry):
            print('Verified bundled:', dest, flush=True)
            return True
        raise ValueError(f'Existing bundled file has wrong checksum: {dest}')
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + '.part')
    if part.is_symlink():
        raise ValueError('Refusing symlink partial')
    limit = entry.get('size_bytes') or (entry['size_mb'] + 10) * 1024 * 1024
    # Re-download into staging; never publish unverified bytes.
    with urllib.request.urlopen(urllib.request.Request(src, headers={'User-Agent':'GatherForge-Vendor/3.9.2'}), timeout=40) as r, part.open('wb') as f:
        total = 0
        while True:
            chunk = r.read(128 * 1024)
            if not chunk: break
            total += len(chunk)
            if total > limit: raise ValueError('File exceeds pinned size ceiling')
            f.write(chunk)
    if not digest(part, entry):
        raise ValueError('Download failed pinned checksum validation; partial kept')
    part.replace(dest)
    print('Installed bundled model:', dest, flush=True)
    return True

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--default', action='store_true', help='Only lightweight default models')
    parser.add_argument('--best-effort', action='store_true', help='Do not fail app install if offline')
    args = parser.parse_args()
    items = json.loads(CATALOG.read_text())['models']
    failed = []
    for entry in items:
        if not entry.get('repo_bundlable') or (args.default and not entry.get('default_install')):
            continue
        try:
            vendor(entry)
        except (OSError, ValueError, urllib.error.URLError) as e:
            print(f'Could not vendor {entry["id"]}: {e}', file=sys.stderr)
            failed.append(entry['id'])
    if failed:
        print('Unavailable now; the installed Rust app will retry automatically:', ', '.join(failed))
    return 0 if args.best_effort or not failed else 1

if __name__ == '__main__':
    raise SystemExit(main())
