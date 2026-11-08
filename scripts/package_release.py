#!/usr/bin/env python3
"""Package a native Rust binary with a checksum-verified official yt-dlp sidecar.

Executed on each target OS in GitHub Actions, not via cross-compilation.
"""
from pathlib import Path
import hashlib
import os
import platform
import shutil
import stat
import sys
import urllib.request
import zipfile

ROOT=Path(__file__).resolve().parents[1]
DIST=ROOT/'dist'
DIST.mkdir(exist_ok=True)
SYSTEM=platform.system().lower()
MACHINE=platform.machine().lower()
TOOL={
    'windows':'yt-dlp.exe',
    'linux':'yt-dlp_linux_aarch64' if MACHINE in ('aarch64','arm64') else 'yt-dlp_linux',
    'darwin':'yt-dlp_macos',
}[SYSTEM]
ARCH='arm64' if MACHINE in ('aarch64','arm64') else 'x64'
FLAVOR={'windows':'Windows','linux':'Linux','darwin':'macOS'}[SYSTEM]
NAME=f'GatherForge-v3.1-{FLAVOR}-{ARCH}'
STAGE=DIST/NAME
STAGE.mkdir(exist_ok=True)
BASE='https://github.com/yt-dlp/yt-dlp/releases/latest/download/'

def fetch(file:str,limit:int)->bytes:
    with urllib.request.urlopen(urllib.request.Request(BASE+file,headers={'User-Agent':'GatherForge-build/3.1'}),timeout=120) as r:
        data=r.read(limit+1)
        if len(data)>limit:raise ValueError(f'Over-size official file: {file}')
        return data

checksums=fetch('SHA2-256SUMS',2_000_000).decode()
expected=None
for line in checksums.splitlines():
    parts=line.strip().split()
    if len(parts)>=2 and parts[1].lstrip('*')==TOOL:
        expected=parts[0].lower();break
if not expected or len(expected)!=64:raise RuntimeError(f'Official checksum missing for {TOOL}')
payload=fetch(TOOL,160*1024*1024)
actual=hashlib.sha256(payload).hexdigest()
if actual!=expected:raise RuntimeError(f'Checksum mismatch for {TOOL}')

exe=ROOT/'target'/'release'/('gatherforge.exe' if SYSTEM=='windows' else 'gatherforge')
if not exe.exists():raise FileNotFoundError(exe)
if SYSTEM=='darwin':
    app=STAGE/'GatherForge.app'/'Contents'
    bin_dir=app/'MacOS'
    bin_dir.mkdir(parents=True,exist_ok=True)
    (app/'Info.plist').write_text('''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>GatherForge</string>
<key>CFBundleIdentifier</key><string>app.gatherforge.desktop</string>
<key>CFBundleName</key><string>GatherForge</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>3.1.0</string>
</dict></plist>''')
    binary=bin_dir/'GatherForge'
else:
    bin_dir=STAGE
    binary=bin_dir/('GatherForge.exe' if SYSTEM=='windows' else 'GatherForge')
shutil.copy2(exe,binary)
(bin_dir/TOOL).write_bytes(payload)
for p in [binary,bin_dir/TOOL]:p.chmod(p.stat().st_mode|stat.S_IXUSR|stat.S_IXGRP|stat.S_IXOTH)
shutil.copy2(ROOT/'README.md',STAGE/'README.md')
if SYSTEM=='linux':shutil.copy2(ROOT/'assets'/'gatherforge.svg',STAGE/'gatherforge.svg')
archive=DIST/(NAME+'.zip')
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
    for p in STAGE.rglob('*'):
        if p.is_file():z.write(p,arcname=str(Path(NAME)/p.relative_to(STAGE)))
print(f'Created {archive} ({archive.stat().st_size:,} bytes)')
