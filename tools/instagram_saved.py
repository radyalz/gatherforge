#!/usr/bin/env python3
"""Browser-assisted GatherForge Saved collection import.

The user logs in to Instagram directly in Chromium and selects a Saved collection.
GatherForge does not ask for passwords or store exported cookies. No private API.
The browser's persistent profile is dedicated to GatherForge and stays local.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import http.cookiejar
import json
import os
import queue
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.parse import urlsplit

HOSTS = {"instagram.com", "www.instagram.com", "m.instagram.com"}
REEL = re.compile(r"^/(reel|reels|p)/([A-Za-z0-9_-]{4,60})/?$")
VIDEO_EXTS = {".mp4", ".mkv", ".mov", ".webm"}
MAX_ITEMS = 1000
STOP = False


def stop_signal(*_):
    global STOP
    STOP = True


def log(message: str) -> None:
    print(message, flush=True)


def reel_url(value: str) -> tuple[str, str] | None:
    try:
        u = urlsplit(value)
    except ValueError:
        return None
    if u.scheme != "https" or (u.hostname or "").lower() not in HOSTS:
        return None
    match = REEL.fullmatch(u.path)
    return (f"https://www.instagram.com/{'reel' if match.group(1) == 'reels' else match.group(1)}/{match.group(2)}/", match.group(2)) if match else None


def valid_saved_page(value: str) -> bool:
    try:
        u = urlsplit(value)
    except ValueError:
        return False
    return u.scheme == "https" and (u.hostname or "").lower() in HOSTS and "saved" in u.path.lower().split("/")


def slug(label: str, page_url: str) -> str:
    """A human label plus stable URL suffix avoids collisions and unsafe paths."""
    cleaned = re.sub(r"[^\w-]+", "-", label.strip(), flags=re.UNICODE).strip("-._")[:65] or "Saved"
    digest = hashlib.sha256(page_url.encode("utf8")).hexdigest()[:8]
    return f"{cleaned}-{digest}"


def safe_dir(path: Path, root: Path) -> Path:
    root = root.resolve()
    if root.is_symlink() or root == Path.home().resolve() or len(root.parts) < 3:
        raise ValueError("Refusing unsafe media root")
    if not path.resolve().is_relative_to(root):
        raise ValueError("Media folder escapes configured root")
    candidate = root
    for part in path.relative_to(root).parts:
        candidate = candidate / part
        if candidate.is_symlink():
            raise ValueError(f"Refusing symlink: {candidate}")
    path.mkdir(parents=True, exist_ok=True)
    return path


def write_json(path: Path, data: dict) -> None:
    if path.is_symlink() or path.parent.is_symlink():
        raise ValueError("Refusing symlink manifest")
    with tempfile.NamedTemporaryFile("w", dir=path.parent, prefix=".manifest-", suffix=".tmp", encoding="utf8", delete=False) as stream:
        tmp = Path(stream.name)
        json.dump(data, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    try:
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def read_json(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 2_000_000:
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf8"))
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


def video_file(folder: Path) -> Path | None:
    matches = [p for p in folder.iterdir() if p.is_file() and not p.is_symlink() and p.suffix.lower() in VIDEO_EXTS and p.stat().st_size > 0]
    return sorted(matches)[0] if len(matches) == 1 else None


def verified(folder: Path, canonical: str) -> bool:
    record = read_json(folder / "manifest.json")
    media = video_file(folder) if folder.is_dir() and not folder.is_symlink() else None
    if not media or not record.get("downloaded") or not record.get("source_url"):
        return False
    url = reel_url(str(record.get("source_url")))
    if not url or url[1] != reel_url(canonical)[1]:
        return False
    if media.name != record.get("video_file") or media.stat().st_size != record.get("size_bytes"):
        return False
    expected_hash = record.get("sha256")
    if not isinstance(expected_hash, str) or len(expected_hash) != 64:
        return False
    return hash_file(media) == expected_hash


def hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def manifest_for(url: str, collection: str, saved_url: str, media: Path, metadata: dict, before: dict | None = None) -> dict:
    before = before or {}
    canonical, short = reel_url(url)
    upload = metadata.get("upload_date") or ""
    if not isinstance(upload, str):
        upload = ""
    creator = metadata.get("uploader_id") or metadata.get("channel_id") or metadata.get("uploader") or before.get("creator") or ""
    description = metadata.get("description") or metadata.get("title") or before.get("description") or ""
    return {
        "schema_version": 2,
        "id": short,
        "shortcode": short,
        "source_url": canonical,
        "url": canonical,
        "creator": str(creator)[:200],
        "caption": str(description)[:20000],
        "description": str(description)[:20000],
        "date": upload,
        "upload_date": upload,
        "duration": metadata.get("duration"),
        "views": metadata.get("view_count"),
        "likes": metadata.get("like_count"),
        "comments": metadata.get("comment_count"),
        "collection": collection,
        "saved_collection_url": saved_url,
        "video_file": media.name,
        "size_bytes": media.stat().st_size,
        "sha256": hash_file(media),
        "downloaded": True,
        "downloaded_at": before.get("downloaded_at") or dt.datetime.now(dt.timezone.utc).isoformat(),
        "instagram_unsaved": bool(before.get("instagram_unsaved", False)),
        "instagram_unsaved_at": before.get("instagram_unsaved_at"),
    }


def insta_cookiefile(context, filepath: Path) -> None:
    """Temporary Netscape cookie jar, limited to Instagram; deleted after yt-dlp."""
    jar = http.cookiejar.MozillaCookieJar(str(filepath))
    for cookie in context.cookies(["https://www.instagram.com/"]):
        domain = str(cookie.get("domain", "")).lstrip(".").lower()
        if not (domain == "instagram.com" or domain.endswith(".instagram.com")):
            continue
        jar.set_cookie(http.cookiejar.Cookie(
            version=0, name=cookie["name"], value=cookie["value"], port=None,
            port_specified=False, domain=cookie["domain"], domain_specified=True,
            domain_initial_dot=cookie["domain"].startswith("."), path=cookie.get("path", "/"),
            path_specified=True, secure=bool(cookie.get("secure", True)),
            expires=int(cookie.get("expires", 0)) or None, discard=not bool(cookie.get("expires", 0)),
            comment=None, comment_url=None, rest={"HttpOnly": cookie.get("httpOnly", False)}, rfc2109=False
        ))
    fd = os.open(filepath, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.close(fd)
    jar.save(ignore_discard=True, ignore_expires=True)
    filepath.chmod(0o600)


def download_one(root: Path, label: str, saved_url: str, canonical: str, context) -> tuple[Path, bool]:
    """Download only when needed. Commit a per-Reel manifest before returning verified."""
    parsed = reel_url(canonical)
    if not parsed:
        raise ValueError("Invalid Reel URL")
    canonical, code = parsed
    folder = safe_dir(root / "Collections" / slug(label, saved_url) / code, root)
    record = read_json(folder / "manifest.json")
    if verified(folder, canonical):
        log(f"SKIP already verified {code}")
        return folder, True
    tool = shutil.which("yt-dlp")
    if not tool:
        raise RuntimeError("yt-dlp is not installed")
    # Cookies are not kept in the Reel folder or included in a manifest.
    with tempfile.TemporaryDirectory(prefix="gatherforge-ig-cookies-") as cookie_tmp:
        cookies = Path(cookie_tmp) / "cookies.txt"
        insta_cookiefile(context, cookies)
        args = [tool, "--ignore-config", "--no-playlist", "--continue", "--no-overwrites",
                "--no-post-overwrites", "--no-warnings", "--quiet", "--print-json",
                "--merge-output-format", "mp4", "-f", "bv*+ba/b", "-P", str(folder),
                "-o", "video.%(ext)s", "--cookies", str(cookies), "--", canonical]
        process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        started = time.monotonic()
        try:
            while True:
                try:
                    out_text, err_text = process.communicate(timeout=0.5)
                    break
                except subprocess.TimeoutExpired:
                    if STOP or time.monotonic() - started > 900:
                        process.terminate()
                        try:
                            process.communicate(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.communicate()
                        raise RuntimeError("Download stopped or timed out; partial data retained for resumption")
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()
    if process.returncode != 0:
        raise RuntimeError(f"yt-dlp failed ({process.returncode}): {err_text.strip()[-250:]}")
    media = video_file(folder)
    if not media:
        raise RuntimeError("No single nonempty video file found; keeping saved item")
    json_lines = [line for line in out_text.splitlines() if line.lstrip().startswith("{")]
    metadata = {}
    for line in reversed(json_lines):
        try:
            metadata = json.loads(line)
            break
        except ValueError:
            continue
    if not isinstance(metadata, dict):
        metadata = {}
    data = manifest_for(canonical, label, saved_url, media, metadata, record)
    write_json(folder / "manifest.json", data)
    if not verified(folder, canonical):
        raise RuntimeError("Local verification failed; keeping saved item")
    log(f"VERIFIED {code} · @{data['creator']} · {media.stat().st_size:,} bytes")
    return folder, False


def find_links(page, limit: int = MAX_ITEMS) -> list[str]:
    """Only browser-visible links; no GraphQL or private Instagram endpoints."""
    found: dict[str, None] = {}
    stable = 0
    for step in range(200):
        if STOP:
            break
        hrefs = page.locator('main a[href*="/reel/"],main a[href*="/reels/"],main a[href*="/p/"]').evaluate_all(
            "els => els.map(e => e.href).filter(Boolean)")
        size = len(found)
        for href in hrefs:
            parsed = reel_url(href)
            if parsed:
                found[parsed[0]] = None
            if len(found) >= limit:
                break
        if len(found) >= limit:
            break
        stable = stable + 1 if len(found) == size else 0
        if stable >= 4:
            break
        page.evaluate("""() => {
            const list = [...document.querySelectorAll('main,section,div')]
              .filter(el => el.scrollHeight > el.clientHeight + 180 && el.clientHeight > 250)
              .sort((a,b) => b.scrollHeight-a.scrollHeight);
            for (const el of list.slice(0,3)) el.scrollTop = el.scrollHeight;
            window.scrollTo(0, document.body.scrollHeight);
        }""")
        page.wait_for_timeout(950)
        if step and step % 10 == 0:
            log(f"DISCOVERED {len(found)} visible links · scrolling")
    return list(found)


def unsave_verified(context, folder: Path, canonical: str) -> bool:
    """Fail closed: click only an unambiguously SAVED control, confirm the result."""
    if not verified(folder, canonical):
        return False
    data = read_json(folder / "manifest.json")
    if data.get("instagram_unsaved"):
        return True
    # GATHERFORGE_REEL_PIPELINE_310: no Instagram mutation until approved locally.
    from reel_analysis import approved_for_unsave
    if not approved_for_unsave(folder, folder.parents[2], canonical):
        log(f"UNSAVE BLOCKED {data.get('id')} · review, approval, or organized copy missing")
        return False
    page = context.new_page()
    try:
        page.goto(canonical, wait_until="domcontentloaded", timeout=35000)
        page.wait_for_timeout(950)
        selectors = [
            'button[aria-label="Remove from saved"]',
            'button[aria-label="Unsave"]',
            'svg[aria-label="Remove"]',
            'svg[aria-label="Saved"]',
        ]
        buttons = []
        for selector in selectors:
            element = page.locator(selector)
            if element.count() == 1:
                buttons.append((selector, element))
        if len(buttons) != 1:
            log(f"UNSAVE MANUALLY {data.get('id')} · Saved control ambiguous or absent")
            return False
        selector, element = buttons[0]
        target = element.locator('xpath=ancestor::*[self::button or @role="button"][1]') if selector.startswith("svg") else element
        if target.count() != 1:
            log(f"UNSAVE MANUALLY {data.get('id')} · No unique Save button")
            return False
        target.click(timeout=5000)
        page.wait_for_timeout(1000)
        # Some layouts show an explicit 'Unsave' confirmation dialog.
        confirm = page.get_by_role("button", name=re.compile(r"^(Unsave|Remove from saved)$", re.I))
        if confirm.count() == 1:
            confirm.click(timeout=4000)
            page.wait_for_timeout(750)
        now_unmarked = (page.locator('svg[aria-label="Save"],button[aria-label="Save"]').count() == 1
                        and page.locator('svg[aria-label="Remove"],svg[aria-label="Saved"],button[aria-label="Remove from saved"]').count() == 0)
        if not now_unmarked:
            log(f"UNSAVE UNCONFIRMED {data.get('id')} · Keeping manifest status unchanged")
            return False
        data["instagram_unsaved"] = True
        data["instagram_unsaved_at"] = dt.datetime.now(dt.timezone.utc).isoformat()
        write_json(folder / "manifest.json", data)
        log(f"UNSAVED verified {data.get('id')}")
        return True
    except Exception as exc:
        log(f"UNSAVE MANUALLY {data.get('id')} · {type(exc).__name__}: {str(exc)[:130]}")
        return False
    finally:
        page.close()


OVERLAY = r"""(() => {
if (window.__gfInjected) return; window.__gfInjected = true;
function mount() {
 if (!/instagram\.com$/.test(location.hostname.replace(/^www\./,''))) return;
 let old = document.getElementById('gatherforge-saved-panel');
 if (old || !document.body) return;
 const box = document.createElement('div'); box.id='gatherforge-saved-panel';
 box.style='all:initial;position:fixed;right:18px;bottom:18px;z-index:2147483647;font:13px system-ui;max-width:290px;box-shadow:0 10px 28px #0005;background:#152235f2;color:white;border:1px solid #ffffff40;padding:12px;border-radius:2px;';
 const title=document.createElement('strong');title.textContent='GatherForge · Saved collections';box.appendChild(title);
 const note=document.createElement('p');note.style='font:12px system-ui;line-height:1.5;color:#d7e7eb';note.textContent='Sign in, open Saved, then select one collection. Enter its name here and press Download.';box.appendChild(note);
 const name=document.createElement('input');name.placeholder='Collection name (e.g. Recipes)';name.maxLength=80;
 name.style='box-sizing:border-box;width:100%;padding:9px;color:#121c2a;background:#fff;border:0;border-radius:2px;margin:0 0 6px';box.appendChild(name);
 const status=document.createElement('div');status.style='margin:5px 0;color:#a9d9e0;font:11px system-ui';box.appendChild(status);
 function make(label,action){const b=document.createElement('button');b.textContent=label;b.style='width:100%;padding:9px;margin-top:5px;border:1px solid #ffffff55;border-radius:2px;color:white;background:#267a82;cursor:pointer;font:12px system-ui';b.onclick=()=>{
 if (!location.pathname.split('/').includes('saved')) { status.textContent='Open your Saved collection first.';return; }
 if (!name.value.trim()) {status.textContent='Enter a collection name.';return;}
 if(action==='reconcile'&&!confirm('Unsave ONLY Reels that you manually approved and organized in GatherForge? This updates Instagram Saved.')) return;
 status.textContent='Request sent to GatherForge. Keep browser open.';
 window.gatherforge_action({action,url:location.href,label:name.value.trim()});};box.appendChild(b);}
 make('Download selected collection','download');make('Unsave approved + organized Reels','reconcile');
 document.body.appendChild(box);
}
setInterval(mount,1400);document.addEventListener('DOMContentLoaded', mount);mount();
})();"""


def collection_job(context, page, action: dict, root: Path, auto_unsave: bool, max_items: int) -> None:
    collection_url = str(action.get("url", ""))
    name = str(action.get("label", "")).strip()[:80]
    mode = str(action.get("action", ""))
    if not valid_saved_page(collection_url) or not name or mode not in {"download", "reconcile"}:
        log("ERROR Choose one Saved collection first")
        return
    if page.url != collection_url:
        log("ERROR Browser page changed; reselect the collection")
        return
    collection_url = "https://www.instagram.com" + urlsplit(collection_url).path.rstrip("/") + "/"
    log(f"COLLECTION {name} · {mode} · discovering browser-visible links")
    links = find_links(page, max_items)
    if not links:
        log("ERROR No post links found. Instagram may have changed its web layout or content is not loaded.")
        return
    folder = root / "Collections" / slug(name, collection_url)
    log(f"FOUND {len(links)} candidate links · {folder}")
    verified_count = 0
    skipped_count = 0
    unsaved_count = 0
    error_count = 0
    for index, url in enumerate(links, 1):
        if STOP:
            break
        canonical, shortcode = reel_url(url)
        target = folder / shortcode
        try:
            if mode == "download":
                destination, previously = download_one(root, name, collection_url, canonical, context)
                verified_count += 1
                if previously:
                    skipped_count += 1
                # Automatic unsaving is never permitted: require review + separate browser action.
            elif verified(target, canonical):
                if unsave_verified(context, target, canonical):
                    unsaved_count += 1
            else:
                skipped_count += 1
        except Exception as exc:
            error_count += 1
            log(f"ERROR {shortcode}: {type(exc).__name__} {str(exc)[:220]}")
        if index % 10 == 0:
            log(f"PROGRESS {index}/{len(links)} · verified={verified_count} · unsaved={unsaved_count} · errors={error_count}")
    log(f"DONE {name} · verified={verified_count} · already/ignored={skipped_count} · unsaved={unsaved_count} · errors={error_count}")


def run_browser(root: Path, auto_unsave: bool, max_items: int) -> None:
    # The Playwright Python library and a system Chromium binary are installed
    # by install.sh. A separate browser profile isolates GatherForge credentials.
    try:
        from playwright.sync_api import sync_playwright
    except ImportError as exc:
        raise RuntimeError("Playwright unavailable. Re-run the v3.6 installer.") from exc
    profile = Path.home() / ".config" / "gatherforge" / "instagram-browser"
    profile.mkdir(parents=True, exist_ok=True, mode=0o700)
    profile.chmod(0o700)
    events: queue.Queue = queue.Queue()
    browser_bin = os.environ.get("GATHERFORGE_CHROMIUM_PATH") or next((shutil.which(n) for n in ("chromium", "chromium-browser", "google-chrome-stable", "google-chrome", "brave-browser") if shutil.which(n)), None)
    if not browser_bin:
        raise RuntimeError("Chromium not found. Install Chromium or set GATHERFORGE_CHROMIUM_PATH")
    with sync_playwright() as play:
        context = play.chromium.launch_persistent_context(str(profile), executable_path=browser_bin, headless=False,
                                                          accept_downloads=False, viewport={"width": 1260, "height": 860})
        try:
            context.expose_binding("gatherforge_action", lambda source, action: events.put((source["page"], action)))
            context.add_init_script(OVERLAY)
            page = context.pages[0] if context.pages else context.new_page()
            page.goto("https://www.instagram.com/", wait_until="domcontentloaded", timeout=60000)
            page.evaluate(OVERLAY)
            log("BROWSER READY · Sign in yourself. Open Saved, select a collection, enter its name, then click a GatherForge button in the browser.")
            log("Passwords never pass through GatherForge. Unsave-after-download is " + ("ON" if auto_unsave else "OFF"))
            while not STOP:
                try:
                    page, action = events.get_nowait()
                except queue.Empty:
                    # Process browser events, and exit when the browser closes.
                    if not context.pages:
                        break
                    try:
                        context.pages[0].wait_for_timeout(400)
                    except Exception:
                        break
                    continue
                collection_job(context, page, action, root, auto_unsave, max_items)
                log("READY · Select another collection or close browser to finish.")
        finally:
            context.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--auto-unsave", action="store_true", help="Deprecated and blocked. Use explicit reviewed reconciliation")
    parser.add_argument("--max-items", type=int, default=500)
    args = parser.parse_args()
    if not 1 <= args.max_items <= MAX_ITEMS:
        parser.error(f"--max-items must be 1..{MAX_ITEMS}")
    root = Path(args.root).expanduser().resolve()
    safe_dir(root / "Collections", root)
    signal.signal(signal.SIGTERM, stop_signal)
    signal.signal(signal.SIGINT, stop_signal)
    if args.auto_unsave:
        parser.error('Automatic unsaving disabled. Review and approve each Reel first.')
    run_browser(root, False, args.max_items)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        log(f"ERROR {type(exc).__name__}: {str(exc)[:240]}")
        raise SystemExit(2)
