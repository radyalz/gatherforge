#!/usr/bin/env python3
"""GatherForge model catalog manager (stdlib; no weights in the repository).

Only explicit `install ID --yes` sends HTTP requests. Downloads are pinned to
catalog file hashes, bounded by a configurable disk budget and atomically moved.
This manages MODEL FILES, not executable inference runtimes or Reel classification.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import sys
import urllib.request
import urllib.error

CATALOG_PATH = Path(__file__).resolve().parent.parent / "models" / "catalog.json"
# Installed CLI lives in ~/.local/share/gatherforge/ next to models/catalog.json.
if not CATALOG_PATH.exists():
    CATALOG_PATH = Path(__file__).resolve().parent / "models" / "catalog.json"
CONFIG_BASE = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))).expanduser()
CONFIG_PATH = CONFIG_BASE / "gatherforge" / "ai.json"
DEFAULT_MODEL_DIR = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "gatherforge/models"
ALLOWED_HOSTS = {"huggingface.co", "raw.githubusercontent.com"}


def defaults():
    return {
        "schema_version": 1,
        "model_dir": str(DEFAULT_MODEL_DIR),
        "storage_limit_mb": 600,
        "auto_download": False,
        "speech_backend": "managed_local",
        "ocr_backend": "managed_local",
        "vision_backend": "disabled",
        "classification_backend": "rules",
        "speech_custom_path": "", "ocr_custom_path": "", "vision_custom_path": "", "classification_custom_path": "",
        "api_endpoint": "", "api_model": "", "api_key_env": "GATHERFORGE_AI_API_KEY",
        "send_private_media_to_api": False,
        "auto_unsave": False,
        "require_approval_before_unsave": True,
    }


def load_config():
    cfg = defaults()
    if CONFIG_PATH.exists():
        read = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
        if not isinstance(read, dict):
            raise ValueError("Invalid AI config: expected object")
        cfg.update({k: v for k, v in read.items() if k in cfg})
    cfg["storage_limit_mb"] = int(cfg["storage_limit_mb"])
    if not 1 <= cfg["storage_limit_mb"] <= 102400:
        raise ValueError("storage_limit_mb must be 1..102400")
    if cfg["auto_download"] is not False:
        raise ValueError("auto_download must remain false for this release")
    return cfg


def safe_save(obj):
    CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)
    if CONFIG_PATH.is_symlink():
        raise ValueError("Refusing to overwrite symlink config")
    temp = CONFIG_PATH.with_name(CONFIG_PATH.name + ".new")
    if temp.is_symlink():
        raise ValueError("Refusing symlink temporary config")
    with temp.open("w", encoding="utf-8") as stream:
        os.chmod(temp, 0o600)
        json.dump(obj, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    temp.replace(CONFIG_PATH)


def catalog():
    data = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1 or not isinstance(data.get("models"), list):
        raise ValueError("Invalid model catalog")
    result = {}
    for item in data["models"]:
        mid = item["id"]
        rel = PurePosixPath(item["relative_path"])
        if (not mid or not all(c.isalnum() or c in "-_" for c in mid)
                or mid in result or rel.is_absolute() or ".." in rel.parts
                or len(rel.parts) < 2):
            raise ValueError(f"Invalid model path or ID: {mid!r}")
        from urllib.parse import urlparse
        url = urlparse(item["url"])
        if url.scheme != "https" or url.hostname not in ALLOWED_HOSTS:
            raise ValueError("Disallowed model download hostname")
        if item["hash_type"] not in ("sha256", "git_blob_sha1"):
            raise ValueError("Unknown hash type")
        if not isinstance(item["hash"], str) or len(item["hash"]) != (64 if item["hash_type"] == "sha256" else 40):
            raise ValueError("Invalid catalog digest")
        result[mid] = item
    return result


def model_dir(cfg):
    path = Path(os.path.expandvars(str(cfg["model_dir"]))).expanduser()
    if not path.is_absolute():
        raise ValueError("Model directory must be absolute")
    return path


def target_path(entry, cfg):
    root = model_dir(cfg)
    path = root.joinpath(*PurePosixPath(entry["relative_path"]).parts)
    # The destination must be inside the selected model directory.
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("Model destination escapes selected model directory")
    # Do not follow symlinks from storage root to target.
    check = root
    if check.is_symlink():
        raise ValueError("Symlinked model root is not supported")
    for part in PurePosixPath(entry["relative_path"]).parts:
        check = check / part
        if check.is_symlink():
            raise ValueError("Symlinked model destination is not supported")
    return path


def hasher(entry, path):
    typ = entry["hash_type"]
    sha = hashlib.sha256() if typ == "sha256" else hashlib.sha1()
    size = path.stat().st_size
    if typ == "git_blob_sha1":
        sha.update(f"blob {size}\0".encode())
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest().lower()


def verify(entry, cfg):
    dest = target_path(entry, cfg)
    if not dest.is_file():
        return False
    if entry.get("size_bytes") and dest.stat().st_size != entry["size_bytes"]:
        return False
    return hasher(entry, dest) == entry["hash"]


def total_size(root):
    if not root.exists():
        return 0
    total = 0
    for path in root.rglob("*"):
        if path.is_file() and not path.is_symlink():
            total += path.stat().st_size
    return total


def install(entry, cfg):
    dest = target_path(entry, cfg)
    if verify(entry, cfg):
        print(f"Verified, already installed: {entry['id']}")
        return
    if dest.exists():
        raise RuntimeError(f"Corrupt/existing file detected. Inspect manually, do not overwrite: {dest}")
    limit = int(cfg["storage_limit_mb"]) * 1024 * 1024
    root = model_dir(cfg)
    expected = int(entry.get("size_bytes", entry["size_mb"] * 1024 * 1024))
    if total_size(root) + expected > limit:
        raise RuntimeError(f"Model exceeds {cfg['storage_limit_mb']} MB storage budget")
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + ".part")
    if part.is_symlink():
        raise RuntimeError("Symlinked partial download refused")
    existing = part.stat().st_size if part.exists() else 0
    request = urllib.request.Request(entry["url"], headers={"User-Agent": "GatherForge-ModelManager/1"})
    if existing:
        request.add_header("Range", f"bytes={existing}-")
    print(f"Downloading {entry['id']} -> {dest} ({entry['size_mb']} MB approx)", flush=True)
    with urllib.request.urlopen(request, timeout=45) as response:
        # A server that ignores Range returns 200; restart safely rather than append invalid bytes.
        status = response.status
        if existing and status != 206:
            existing = 0
        if existing and status == 206:
            content_range = response.headers.get("Content-Range", "")
            if not content_range.startswith(f"bytes {existing}-"):
                raise RuntimeError("Invalid resume range returned by server")
        mode = "ab" if existing else "wb"
        downloaded = existing
        with part.open(mode) as stream:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                downloaded += len(chunk)
                if total_size(root) + len(chunk) > limit:
                    raise RuntimeError("Model download exceeded configured storage limit")
                stream.write(chunk)
    # Verify the partial file BEFORE promoting it. Never trust a URL or name alone.
    if (entry.get("size_bytes") and part.stat().st_size != entry["size_bytes"]) or hasher(entry, part) != entry["hash"]:
        raise RuntimeError(f"Downloaded bytes failed catalog integrity verification: {part}")
    # Atomic promote to installed model file.
    part.replace(dest)
    print(f"Installed and hash-verified: {entry['id']}")


def remove(entry, cfg):
    dest = target_path(entry, cfg)
    part = dest.with_name(dest.name + ".part")
    for path in (dest, part):
        if path.is_symlink():
            raise RuntimeError(f"Refusing to remove a symlink: {path}")
        if path.is_file():
            path.unlink()
            print(f"Removed managed model data: {path}")


def status(cfg, entries):
    root = model_dir(cfg)
    result = []
    for item in entries.values():
        path = target_path(item, cfg)
        result.append({
            "id": item["id"], "title": item["title"], "role": item["role"],
            "path": str(path), "installed": path.is_file(),
            "size_bytes": path.stat().st_size if path.is_file() else 0,
            "partial_bytes": path.with_name(path.name + ".part").stat().st_size if path.with_name(path.name + ".part").is_file() else 0,
            "estimate_mb": item["size_mb"], "integrity": "unchecked; use verify",
        })
    return {"config_path": str(CONFIG_PATH), "storage": str(root), "used_bytes": total_size(root), "models": result}


def cli(argv=None):
    parser = argparse.ArgumentParser(description="GatherForge managed local model files")
    subs = parser.add_subparsers(dest="command", required=True)
    subs.add_parser("init")
    subs.add_parser("config")
    show = subs.add_parser("status")
    show.add_argument("--json", action="store_true")
    for op in ("install", "verify", "remove"):
        p = subs.add_parser(op)
        p.add_argument("id")
        if op in ("install", "remove"):
            p.add_argument("--yes", action="store_true")
    conf = subs.add_parser("set")
    conf.add_argument("key")
    conf.add_argument("value")
    subs.add_parser("doctor")
    args = parser.parse_args(argv)
    cfg = load_config()
    items = catalog()
    if args.command == "init":
        if not CONFIG_PATH.exists():
            safe_save(cfg)
            print("Created", CONFIG_PATH)
        else:
            print("Existing config preserved:", CONFIG_PATH)
        model_dir(cfg).mkdir(parents=True, exist_ok=True)
    elif args.command == "config":
        print(json.dumps(cfg, indent=2))
    elif args.command == "status":
        s = status(cfg, items)
        if args.json:
            print(json.dumps(s, indent=2))
        else:
            print("Models:", s["storage"], f"({s['used_bytes'] // 1024 // 1024} MB used)")
            for x in s["models"]:
                print("  ", "INSTALLED" if x["installed"] else "missing", x["id"], "(verify explicitly)")
    elif args.command in ("install", "verify", "remove"):
        if args.id not in items:
            parser.error(f"Unknown model ID: {args.id}")
        entry = items[args.id]
        if args.command == "verify":
            if not verify(entry, cfg):
                raise RuntimeError(f"Verification FAILED or model missing: {args.id}")
            print("Verified:", args.id)
        elif args.command == "install":
            if not args.yes:
                parser.error("Installing requires --yes; no automatic downloads")
            install(entry, cfg)
        else:
            if not args.yes:
                parser.error("Removing a model requires --yes")
            remove(entry, cfg)
    elif args.command == "set":
        allowed = {"model_dir", "storage_limit_mb", "speech_backend", "ocr_backend", "vision_backend", "classification_backend", "speech_custom_path", "ocr_custom_path", "vision_custom_path", "classification_custom_path", "api_endpoint", "api_model", "api_key_env"}
        if args.key not in allowed:
            parser.error("Unknown or locked configuration key")
        v = args.value
        if args.key == "storage_limit_mb":
            v = int(v)
            if not 1 <= v <= 102400:
                parser.error("storage_limit_mb out of range")
        if args.key.endswith("_backend") and v not in ("managed_local", "custom_local", "api", "disabled", "rules"):
            parser.error("Invalid backend")
        if args.key == "model_dir" and not Path(os.path.expandvars(v)).expanduser().is_absolute():
            parser.error("Model directory must be absolute")
        if args.key == "api_key_env" and (not v.isidentifier() or len(v) > 120):
            parser.error("Provide the *name* of an env var, never a secret")
        cfg[args.key] = v
        safe_save(cfg)
        print("Saved", args.key)
    elif args.command == "doctor":
        print("Python:", sys.version.split()[0])
        for tool in ("ffmpeg", "whisper-cli", "tesseract", "llama-cli", "llama-server"):
            print(f"{tool}: {shutil.which(tool) or 'NOT FOUND (optional except for its stage)'}")
        print("Mode: model management only; Reel inference wiring not implemented.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(cli())
    except (RuntimeError, ValueError, OSError, urllib.error.URLError) as err:
        print(f"GatherForge model manager: {err}", file=sys.stderr)
        sys.exit(1)
