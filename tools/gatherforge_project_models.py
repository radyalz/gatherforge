#!/usr/bin/env python3
"""GatherForge: migrate pinned models into the project beside GatherForge-Rust-Linux.

This installer does not modify Rust code or rebuild the executable. It makes
one canonical app home (the project directory), updates the desktop launcher,
and migrates only hash-verified assets. No deletions of legacy media/models.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.request

MARK = 'GATHERFORGE_PROJECT_LOCAL_MODELS_V3'
IGNORE = f'''\n# {MARK}: project-local, repo-friendly model files
/models/config.local.json
/models/config.local.json.tmp
/models/installed/
/models/cache/
/models/**/*.part
/models/**/*.part.*
/models/bundled/vision/
# The three verified small defaults are intentionally eligible for Git.
!/models/bundled/
!/models/bundled/asr/
!/models/bundled/ocr/
!/models/bundled/ocr/tessdata/
!/models/bundled/asr/ggml-tiny-q5_1.bin
!/models/bundled/ocr/tessdata/eng.traineddata
!/models/bundled/ocr/tessdata/fas.traineddata
'''
README = f'''\n\n<!-- {MARK} -->
## Canonical local model storage (v3.9.3)
The **project directory containing `GatherForge-Rust-Linux`** is the single
GatherForge app home. The desktop launcher points to that executable instead of
an XDG copy. Therefore `models/`, `models/config.local.json`, and model weights
live directly in this repository (for example
`/mnt/shared/Projects/MusicOrganizer/models/`). Settings displays that path.
Three small, verified model assets can be committed at
`models/bundled/asr/ggml-tiny-q5_1.bin`,
`models/bundled/ocr/tessdata/eng.traineddata`, and
`models/bundled/ocr/tessdata/fas.traineddata`. The optional vision GGUF files,
config.local.json, partials, and downloaded cache remain Git-ignored.
Existing valid models from the old `~/.local/share/gatherforge/models` location
are copied, **not moved**. Missing default assets can be downloaded and hash
verified on installation. An optional model never downloads automatically.
The GUI manages these weights only; transcription, OCR, vision inference, AI
categorization, and AI-gated unsaving are **not functional** yet.
To maintain the paths after rebuilding, run
`python3 tools/gatherforge_project_models.py --apply` from the repository root.
<!-- END {MARK} -->
'''
RULES = f'''\n\n## {MARK}: model directory is part of the project
- The canonical GatherForge runtime executable is `<repo>/GatherForge-Rust-Linux`.
  The menu launcher and `~/.local/bin/GatherForge` must resolve to it.
- The only managed model root is `<repo>/models/`, adjacent to the executable.
  Do not create another active model store under `~/.local/share/gatherforge/`.
- Model config and installed weights use `<repo>/models/config.local.json` and
  `<repo>/models/bundled` or `<repo>/models/installed`; UI must show real paths.
- Bundle only pinned, integrity-verified, license-reviewed small defaults in Git.
  Never commit local config, API secrets, partials, media, or optional large GGUF.
- Never claim inference or categorization works until it is implemented and tested.
- Preserve the project's 2px corner radius rule across every UI component.
'''

def atomic_write(path: Path, content: bytes, mode: int | None = None):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink(): raise RuntimeError(f'Refusing symlink target: {path}')
    fd, tmp = tempfile.mkstemp(prefix=f'.{path.name}.gf393-', dir=path.parent)
    try:
        with os.fdopen(fd,'wb') as file:
            file.write(content)
            file.flush();os.fsync(file.fileno())
        if mode is not None: os.chmod(tmp, mode)
        elif path.exists(): os.chmod(tmp, stat.S_IMODE(path.stat().st_mode))
        os.replace(tmp,path)
    finally:
        if os.path.lexists(tmp): os.unlink(tmp)

def digest(path:Path,item:dict)->bool:
    if path.is_symlink() or not path.is_file():return False
    if 'size_bytes' in item and path.stat().st_size != item['size_bytes']:return False
    if item['hash_type']=='sha256':h=hashlib.sha256()
    elif item['hash_type']=='git_blob_sha1':
        h=hashlib.sha1();h.update(f'blob {path.stat().st_size}\0'.encode())
    else:return False
    with path.open('rb') as f:
        for block in iter(lambda:f.read(131072),b''):h.update(block)
    return h.hexdigest().lower() == item['hash'].lower()

def copy_verified(origin:Path,target:Path,item:dict)->bool:
    if digest(target,item):return True
    if target.exists() or target.is_symlink():
        raise RuntimeError(f'Existing model file has invalid checksum; refusing overwrite: {target}')
    if not digest(origin,item):return False
    if target.parent.is_symlink():raise RuntimeError(f'Symlinked model directory: {target.parent}')
    target.parent.mkdir(parents=True,exist_ok=True)
    fd,tmp=tempfile.mkstemp(prefix=f'.{target.name}.gf393-',dir=target.parent)
    try:
        with origin.open('rb') as src, os.fdopen(fd,'wb') as dest:
            shutil.copyfileobj(src,dest,1024*1024)
            dest.flush();os.fsync(dest.fileno())
        tmp_path=Path(tmp)
        if not digest(tmp_path,item):raise RuntimeError(f'Model copy verification failed: {origin}')
        os.replace(tmp,target)
    finally:
        if os.path.lexists(tmp):os.unlink(tmp)
    return True

def download_verified(item:dict,target:Path):
    if target.exists() or target.is_symlink():raise RuntimeError(f'Cannot overwrite existing model: {target}')
    url=item.get('url','')
    if not (url.startswith('https://huggingface.co/') or url.startswith('https://raw.githubusercontent.com/')):
        raise RuntimeError('Unexpected download host')
    target.parent.mkdir(parents=True,exist_ok=True)
    fd,tmp=tempfile.mkstemp(prefix=f'.{target.name}.gf393-',dir=target.parent)
    os.close(fd)
    try:
        request=urllib.request.Request(url,headers={'User-Agent':'GatherForge/3.9.3'})
        limit=(item.get('size_bytes') or (item['size_mb']+8)*1024*1024)+1024
        with urllib.request.urlopen(request,timeout=40) as src,open(tmp,'wb') as dest:
            total=0
            while True:
                block=src.read(1024*1024)
                if not block:break
                total+=len(block)
                if total>limit:raise RuntimeError(f'Download exceeded model size ceiling: {item["id"]}')
                dest.write(block)
        if not digest(Path(tmp),item):raise RuntimeError(f'Download checksum mismatch for {item["id"]}')
        os.replace(tmp,target)
    finally:
        if os.path.lexists(tmp):os.unlink(tmp)

def append_once(path:Path,text:str,marker:str):
    current=path.read_text(encoding='utf-8') if path.is_file() else ''
    if marker in current:return False
    atomic_write(path,(current.rstrip()+text).encode('utf-8'))
    return True

def main():
    ap=argparse.ArgumentParser(description='Unify model files with the GatherForge project')
    ap.add_argument('repo',nargs='?',default='.')
    ap.add_argument('--apply',action='store_true',help='Apply changes (otherwise preview only)')
    ap.add_argument('--no-download',action='store_true',help='Offline mode: migrate existing verified weights only')
    args=ap.parse_args()
    repo=Path(args.repo).expanduser().resolve()
    home=Path.home()
    app=Path(os.getenv('XDG_DATA_HOME',str(home/'.local/share')))/'gatherforge'
    binary=repo/'GatherForge-Rust-Linux'
    if not (repo/'.git').exists() or not (repo/'Cargo.toml').is_file() or not (repo/'src/ai_models.rs').is_file():
        raise SystemExit(f'Not the v3.9.2+ GatherForge repo: {repo}')
    if not binary.is_file() or binary.is_symlink() or binary.open('rb').read(4)!=b'\x7fELF':
        raise SystemExit(f'Compiled project executable missing/not ELF: {binary}. Nothing changed.')
    if 'env::current_exe()' not in (repo/'src/ai_models.rs').read_text():
        raise SystemExit('Unexpected app-home implementation; nothing changed.')
    print('Canonical model home:',repo/'models')
    print('Desktop executable:',binary)
    pkg=Path(__file__).resolve().parent.parent
    catalog_path=repo/'models/catalog.json'
    if not catalog_path.exists():catalog_path=pkg/'models/catalog.json'
    if not catalog_path.is_file():raise SystemExit('Model catalog missing')
    catalog=json.loads(catalog_path.read_text())
    models=catalog['models']
    for item in models:
        rel=Path(item['relative_path'])
        if rel.is_absolute() or '..' in rel.parts:raise SystemExit('Unsafe model path in catalog')
        dst=repo/'models'/('bundled' if item['repo_bundlable'] else 'installed')/rel
        candidates=[repo/'models/installed'/rel,repo/'models/bundled'/rel,app/'models/bundled'/rel,app/'models/installed'/rel]
        found=next((p for p in candidates if digest(p,item)),None)
        print(' ',item['id'], 'READY' if digest(dst,item) else f'migrate {found}' if found else 'missing (optional)' if not item['default_install'] else 'missing (will download when online)')
    if not args.apply:
        print('Preview only. Re-run with --apply to unify models and the launch path.');return
    backup_base=repo/'.git/gatherforge-backups'
    backup_base.mkdir(parents=True,exist_ok=True)
    stamp=time.strftime('pre-v393-%Y%m%d-%H%M%S')
    backup=backup_base/f'{stamp}-{os.getpid()}'
    backup.mkdir(mode=0o700)
    touched=[]
    backups={}
    link=home/'.local/bin/GatherForge'
    desktop=home/'.local/share/applications/gatherforge.desktop'
    watch=[repo/'models/catalog.json',repo/'models/config.example.json',repo/'models/config.local.json',repo/'README.md',repo/'AGENTS.md',repo/'.gitignore',repo/'docs/MODEL_STORAGE.md',repo/'tools/gatherforge_project_models.py',desktop,link]
    try:
        for p in watch:
            if p.is_symlink():backups[p]=('link',os.readlink(p))
            elif p.is_file():
                name=hashlib.sha256(str(p).encode()).hexdigest()[:16]+'-'+p.name
                dest=backup/name;shutil.copy2(p,dest)
                backups[p]=('file',dest)
            elif p.exists():raise RuntimeError(f'Refusing to modify non-file: {p}')
            else:backups[p]=('absent',None)
        (backup/'manifest.json').write_text(json.dumps({str(k):[v[0],str(v[1]) if v[1] else None] for k,v in backups.items()},indent=2))
        print('Safety backup:',backup,flush=True)
        for rel in ('models/catalog.json','models/config.example.json'):
            dst=repo/rel
            if not dst.is_file():atomic_write(dst,(pkg/rel).read_bytes());touched.append(dst)
        conf=repo/'models/config.local.json'
        if not conf.exists():
            old=app/'models/config.local.json'
            if old.is_file() and not old.is_symlink():
                data=json.loads(old.read_text())
                atomic_write(conf,json.dumps(data,indent=2,ensure_ascii=False).encode());touched.append(conf)
        for item in models:
            rel=Path(item['relative_path'])
            dest=repo/'models'/('bundled' if item['repo_bundlable'] else 'installed')/rel
            if digest(dest,item):continue
            candidates=[repo/'models/installed'/rel,repo/'models/bundled'/rel,app/'models/bundled'/rel,app/'models/installed'/rel]
            valid=next((p for p in candidates if p!=dest and digest(p,item)),None)
            if valid:
                copy_verified(valid,dest,item);touched.append(dest)
                print('Migrated verified:',item['id'],'->',dest,flush=True)
            elif item['default_install'] and not args.no_download:
                try:
                    download_verified(item,dest);touched.append(dest)
                    print('Downloaded + verified:',item['id'],'->',dest,flush=True)
                except Exception as ex:
                    print(f'Warning: {item["id"]} unavailable ({ex}). No placeholder created.',file=sys.stderr)
        docfile=repo/'docs/MODEL_STORAGE.md'
        if docfile.is_symlink():raise RuntimeError('Symlinked model documentation')
        atomic_write(docfile,(pkg/'docs/MODEL_STORAGE.md').read_bytes());touched.append(docfile)
        helper=repo/'tools/gatherforge_project_models.py'
        if helper.is_symlink():raise RuntimeError('Symlinked helper')
        atomic_write(helper,Path(__file__).read_bytes(),0o755);touched.append(helper)
        for path,text,marker in [(repo/'README.md',README,MARK),(repo/'AGENTS.md',RULES,MARK),(repo/'.gitignore',IGNORE,MARK)]:
            if path.is_symlink():raise RuntimeError(f'Symlinked documentation: {path}')
            if append_once(path,text,marker):touched.append(path)
        link.parent.mkdir(parents=True,exist_ok=True)
        # A real existing CLI binary is user data, so do not overwrite it.
        if link.exists() and not link.is_symlink():raise RuntimeError(f'CLI launcher is not a symlink; refusing to replace: {link}')
        staged=link.parent/f'.GatherForge-v393-link-{os.getpid()}'
        if staged.exists() or staged.is_symlink():raise RuntimeError('Temporary launcher path occupied')
        staged.symlink_to(binary)
        os.replace(staged,link);touched.append(link)
        desktop_text=f'''[Desktop Entry]\nType=Application\nName=GatherForge\nComment=Native Music & Reel Workspace\nExec={link}\nIcon=gatherforge\nTerminal=false\nCategories=AudioVideo;Audio;\n'''
        atomic_write(desktop,desktop_text.encode());touched.append(desktop)
        print('DONE. Run GatherForge from the application menu or:',link)
        print('Settings will now use:',repo/'models')
        print('The old XDG model files/executable were not deleted.')
        print('Git status for small bundled weights:')
        subprocess.run(['git','status','--short','--untracked-files=all','--','models/bundled'],cwd=repo,check=False)
    except BaseException:
        print('Migration failed/interrupted. Restoring launcher and docs...',file=sys.stderr)
        for p in reversed(touched):
            try:
                kind,src=backups.get(p,('absent',None))
                if kind=='file':atomic_write(p,Path(src).read_bytes(),stat.S_IMODE(Path(src).stat().st_mode))
                elif kind=='link':
                    if p.exists() or p.is_symlink():p.unlink()
                    p.symlink_to(src)
                else:
                    if p.is_file() or p.is_symlink():p.unlink()
            except Exception as ex: print('Rollback issue:',p,ex,file=sys.stderr)
        raise

if __name__=='__main__':main()