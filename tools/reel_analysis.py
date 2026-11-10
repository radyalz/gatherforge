#!/usr/bin/env python3
"""GatherForge local Reel scan, transcription/OCR/optional vision, approval and foldering.
No network, no credentials, no removal of original media, and no Instagram mutations.
AI category decisions are supplied separately by native Rust Laya (not by this script).
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import csv, hashlib, io, json, math, os, re, shutil, subprocess, sys, tempfile, unicodedata
from pathlib import Path
from urllib.parse import urlsplit

VIDEO_EXTS={'.mp4','.mov','.webm','.mkv'}
CATEGORIES=('Food and recipes','Fitness and health','Travel and places','Technology and gadgets',
 'Home and DIY','Education and tutorials','Entertainment and humor','Shopping and products','Other')


def now(): return datetime.now(timezone.utc).isoformat()

def safe_id(value):
    value=str(value)
    if not re.fullmatch(r'[A-Za-z0-9_-]{3,90}',value): raise ValueError('Unsafe Reel ID')
    return value

def reel_url(text):
    u=urlsplit(str(text))
    if u.scheme!='https' or u.hostname not in {'instagram.com','www.instagram.com','m.instagram.com'} or u.username or u.password:
        return None
    m=re.fullmatch(r'/(reel|reels|p)/([A-Za-z0-9_-]{3,90})/?',u.path)
    if not m: return None
    return f"https://www.instagram.com/{'reel' if m.group(1)=='reels' else m.group(1)}/{m.group(2)}/", m.group(2)

def within(root, path):
    root=root.resolve(strict=True)
    if root.is_symlink() or path.is_symlink() or not path.resolve().is_relative_to(root):
        raise ValueError('Path escapes Reel storage or is a symlink')
    return path

def read_json(p, max_bytes=2_000_000):
    if p.is_symlink() or not p.is_file() or p.stat().st_size>max_bytes: return {}
    try:
        v=json.loads(p.read_text(encoding='utf8'))
        return v if isinstance(v,dict) else {}
    except (OSError,ValueError):return {}

def write_json(p, data):
    if p.is_symlink() or p.parent.is_symlink(): raise ValueError('Refusing symlink destination')
    p.parent.mkdir(parents=True,exist_ok=True)
    fd,tmp=tempfile.mkstemp(prefix='.gf-reel-',suffix='.tmp',dir=p.parent)
    try:
        with os.fdopen(fd,'w',encoding='utf8') as f:
            json.dump(data,f,ensure_ascii=False,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
        os.replace(tmp,p)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)

def sha256(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        for chunk in iter(lambda:f.read(2*1024*1024),b''):h.update(chunk)
    return h.hexdigest()

def checked_media(root, rel):
    raw=Path(rel)
    if raw.is_absolute() or '..' in raw.parts or not raw.parts:raise ValueError('Unsafe media relative path')
    file=within(root,root/raw)
    if not file.is_file() or file.suffix.lower() not in VIDEO_EXTS or file.stat().st_size==0:
        raise ValueError('Nonempty local video missing')
    return file

def locate_info(media):
    stem=media.stem
    for p in (media.with_name(stem+'.info.json'),media.with_name('video.info.json')):
        if p.is_file() and not p.is_symlink():return read_json(p)
    return {}

def ensure_manifest(root,media):
    """Normalize legacy direct downloads using hard links to avoid copying large videos."""
    meta=locate_info(media)
    folder=media.parent
    manifest=read_json(folder/'manifest.json')
    if not manifest:
        url=meta.get('webpage_url') or meta.get('original_url') or meta.get('url')
        parsed=reel_url(url)
        if not parsed:raise ValueError('No canonical Instagram source URL in download metadata')
        url,code=parsed
        # Legacy layout /uploader/id.mp4 is not one Reel per folder; make a hard-link view.
        if media.name!='video'+media.suffix or folder.parts[-2:]!=('Direct',code):
            if not (root/'Collections').resolve().is_relative_to(root.resolve()): raise ValueError('Unsafe root')
            normalized=root/'Collections'/'Direct'/code
            if any(p.is_symlink() for p in [root/'Collections',root/'Collections'/'Direct',normalized]):
                raise ValueError('Refusing symlink destination')
            normalized.mkdir(parents=True,exist_ok=True)
            link=normalized/('video'+media.suffix.lower())
            if not link.exists():os.link(media,link)
            elif link.is_symlink() or not link.samefile(media):raise ValueError('Direct Reel destination conflict')
            media=link;folder=normalized
        manifest={
            'schema_version':3,'id':code,'shortcode':code,'source_url':url,'url':url,
            'creator':str(meta.get('uploader_id') or meta.get('uploader') or ''),
            'caption':str(meta.get('description') or meta.get('title') or '')[:20000],
            'description':str(meta.get('description') or meta.get('title') or '')[:20000],
            'upload_date':meta.get('upload_date') or '', 'duration':meta.get('duration'),
            'views':meta.get('view_count'), 'likes':meta.get('like_count'),
            'collection':'Direct', 'video_file':media.name,'size_bytes':media.stat().st_size,
            'sha256':sha256(media),'downloaded':True,'downloaded_at':now(),
            'instagram_unsaved':False,
        }
        write_json(folder/'manifest.json',manifest)
    else:
        parsed=reel_url(manifest.get('source_url') or manifest.get('url'))
        if not parsed:raise ValueError('Manifest has no valid original Instagram URL')
        if manifest.get('video_file')!=media.name:raise ValueError('Manifest/video filename mismatch')
        if manifest.get('size_bytes')!=media.stat().st_size or manifest.get('sha256')!=sha256(media):
            raise ValueError('Existing manifest verification failed')
        if manifest.get('downloaded') is not True:raise ValueError('Reel not marked downloaded')
    return media,manifest,folder

def probe(media):
    if not shutil.which('ffprobe'):raise RuntimeError('ffprobe required for video validation')
    p=subprocess.run(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(media)],capture_output=True,text=True,timeout=25)
    if p.returncode:raise RuntimeError('ffprobe could not decode media')
    v=json.loads(p.stdout)
    streams=v.get('streams') or []
    if not any(s.get('codec_type')=='video' for s in streams):raise RuntimeError('No video stream')
    try:
        duration=float(v.get('format',{}).get('duration') or 0)
        if not math.isfinite(duration) or duration<0:duration=0
    except (ValueError,TypeError):duration=0
    return {'duration':duration,'audio':any(s.get('codec_type')=='audio' for s in streams)}

def engine(name,root):
    local=root.parent/'tools'/'bin'/name if root.name=='models' else root/'tools'/'bin'/name
    return str(local) if local.is_file() and os.access(local,os.X_OK) else shutil.which(name)

def extract_frame(media,path,second):
    try:
        p=subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-ss',str(second),'-i',str(media),'-frames:v','1','-vf','scale=1600:1600:force_original_aspect_ratio=decrease:force_divisible_by=2', '-y',str(path)],capture_output=True,timeout=25)
        return p.returncode==0 and path.is_file() and path.stat().st_size>0
    except (OSError,subprocess.TimeoutExpired):return False

def frame_positions(duration, interval=2.0, max_frames=64):
    """Sample changing Reel subtitles throughout the video, with a strict CPU cap.

    Five evenly spaced frames miss almost every caption in an ordinary Reel.
    The last sample is kept inside the file rather than at the undecodable EOF.
    """
    if not isinstance(duration,(int,float)) or not math.isfinite(duration) or duration<=0:
        return [0.0]
    end=max(0.0,duration-min(0.2,max(0.02,duration*0.05)))
    if end == 0: return [0.0]
    count=min(max_frames,max(2,math.ceil(end/interval)+1))
    return [round(end*i/(count-1),3) for i in range(count)]

def transcribe(media, models, work, audio_present):
    """Transcribe successive 60-second windows, reporting incomplete results explicitly."""
    if not audio_present:
        return {'state':'skipped','reason':'no audio stream','text':''}
    # Settings are local metadata; API mode is deliberately not implemented here.
    config_path=models/'config.local.json'
    try:
        if models.is_symlink():
            raise ValueError('model root is a symlink')
        if config_path.is_symlink():
            raise ValueError('speech settings file is a symlink')
        if config_path.exists():
            if not config_path.is_file() or config_path.stat().st_size > 131072:
                raise ValueError('speech settings file is invalid or too large')
            config=json.loads(config_path.read_text(encoding='utf8'))
            if not isinstance(config,dict): raise ValueError('speech settings must be an object')
        else:
            config={}
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as e:
        return {'state':'unavailable','reason':f'invalid local speech settings: {e}','text':''}
    backend=config.get('speech_backend','managed_local')
    language=config.get('speech_language','auto')
    # Language is a Whisper code; the UI exposes the common supported languages.
    if language not in ('auto','en','fa','ar','de','es','fr','tr','ru','it','pt','hi','ja','ko','zh'):
        return {'state':'unavailable','reason':'invalid speech language in Settings','text':''}
    if backend == 'disabled':
        return {'state':'skipped','reason':'speech transcription is disabled in Settings','text':'', 'backend':backend}
    if backend == 'api':
        return {'state':'unavailable','reason':'external speech API is not implemented; no data was uploaded','text':'', 'backend':backend}
    if backend not in ('managed_local','custom_local'):
        return {'state':'unavailable','reason':'unknown speech backend in Settings','text':''}
    if backend == 'managed_local':
        variant=config.get('speech_model','tiny')
        names={'tiny':'ggml-tiny-q5_1.bin','base':'ggml-base-q5_1.bin',
               'small':'ggml-small-q5_1.bin'}
        if variant not in names:
            return {'state':'unavailable','reason':'unknown managed speech model in Settings','text':'', 'backend':backend}
        name=names[variant]
        bundled=models/'bundled/asr'/name
        installed=models/'installed/asr'/name
        # Default weights may be shipped in bundled/ or downloaded into installed/.
        weights=bundled if bundled.is_file() and not bundled.is_symlink() else installed
    else:
        value=config.get('speech_custom_path','')
        if not isinstance(value,str) or not value.strip() or '\x00' in value:
            return {'state':'unavailable','reason':'custom speech model path is empty or invalid','text':'', 'backend':backend}
        try:
            base=models.resolve(strict=True)
            path=Path(value.strip()).expanduser()
            if '..' in path.parts:
                raise ValueError('parent traversal is not allowed')
            path=path if path.is_absolute() else base/path
            rel=path.relative_to(base)
            if not rel.parts: raise ValueError('a model file is required')
            weights=base
            for part in rel.parts:
                weights=weights/part
                if weights.is_symlink(): raise ValueError('model symlinks are not allowed')
            if weights.resolve(strict=True) != weights or not weights.is_file() or weights.stat().st_size == 0:
                raise ValueError('model must be a nonempty regular file')
            if weights.suffix.lower() not in ('.bin','.gguf'):
                raise ValueError('expected a local Whisper .bin or .gguf model')
        except (ValueError, OSError, RuntimeError) as e:
            return {'state':'unavailable','reason':f'custom speech model is invalid: {e}','text':'', 'backend':backend}
    model_name=weights.name
    executable=engine('whisper-cli',models)
    if not executable or not weights.is_file():
        return {'state':'unavailable','reason':'whisper-cli executable/model missing; use Setup AI engines','text':'', 'backend':backend, 'model':model_name}
    if not shutil.which('ffmpeg'):
        return {'state':'unavailable','reason':'ffmpeg executable missing','text':'', 'backend':backend, 'model':model_name}
    import math, time, wave
    deadline=time.monotonic()+1200
    audio_duration=None
    try:
        if shutil.which('ffprobe'):
            p=subprocess.run(['ffprobe','-v','error','-select_streams','a:0','-show_entries',
                'stream=duration','-of','default=noprint_wrappers=1:nokey=1',str(media)],
                capture_output=True,text=True,timeout=8)
            value=float(p.stdout.strip()) if p.returncode == 0 else 0.0
            if math.isfinite(value) and value > 0: audio_duration=value
    except (ValueError, OSError, subprocess.TimeoutExpired):
        pass  # Stream duration can be unavailable; decoded samples remain authoritative.
    chunk_seconds=60
    max_chunks=120  # Explicit two-hour resource ceiling; no silent truncation.
    max_text=30000  # Keep the analysis sidecar bounded; report if reached.
    parts=[]
    completed=0
    processed_seconds=0.0
    wav=work/'speech-chunk.wav'
    out=work/'speech-chunk'
    txt=out.with_suffix('.txt')

    def incomplete(reason):
        return {'state':'partial' if completed else 'error', 'engine':'whisper.cpp', 'model':model_name, 'backend':backend,
                'reason':reason, 'text':'\n'.join(parts), 'chunks_completed':completed,
                'seconds_processed':processed_seconds, 'audio_duration_seconds':audio_duration}

    for index in range(max_chunks):
        if time.monotonic() >= deadline:
            return incomplete(f'transcription exceeded the 1200-second total time limit at {processed_seconds:.1f}s')
        # Delete only temporary files owned by this function; never reuse stale output.
        wav.unlink(missing_ok=True)
        txt.unlink(missing_ok=True)
        try:
            p=subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin',
                '-ss',str(index*chunk_seconds),'-i',str(media),'-t',str(chunk_seconds),'-vn','-ar','16000',
                '-ac','1','-c:a','pcm_s16le','-y',str(wav)],capture_output=True,timeout=min(60, max(0.001, deadline-time.monotonic())))
        except subprocess.TimeoutExpired:
            return incomplete(f'audio extraction timed out at {index*chunk_seconds}s')
        except OSError as e:
            return incomplete(f'audio extraction could not start: {e}')
        if p.returncode or not wav.is_file():
            return incomplete(f'audio extraction failed at {index*chunk_seconds}s')
        try:
            with wave.open(str(wav),'rb') as audio:
                frames=audio.getnframes()
                rate=audio.getframerate()
        except (wave.Error, EOFError, OSError):
            return incomplete(f'audio extraction produced an invalid WAV at {index*chunk_seconds}s')
        if rate != 16000:
            return incomplete('audio extraction produced an unexpected sample rate')
        if frames == 0:
            if completed == 0:
                return {'state':'skipped','reason':'audio stream contained no decodable samples','text':''}
            return {'state':'ok','engine':'whisper.cpp', 'model':model_name, 'backend':backend,'text':'\n'.join(parts),
                    'chunks_completed':completed, 'seconds_processed':processed_seconds,
                    'audio_duration_seconds':audio_duration}
        if time.monotonic() >= deadline:
            return incomplete(f'transcription exceeded the 1200-second total time limit at {processed_seconds:.1f}s')
        try:
            p=subprocess.run([executable,'-m',str(weights),'-f',str(wav),'-l',language,
                '-otxt','-of',str(out),'-nt','-t','4','-ng'],capture_output=True,text=True,timeout=min(250, max(0.001, deadline-time.monotonic())))
        except subprocess.TimeoutExpired:
            return incomplete(f'transcription timed out for chunk {index+1} at {index*chunk_seconds}s')
        except OSError as e:
            return incomplete(f'transcription could not start: {e}')
        if p.returncode or not txt.is_file():
            return incomplete(f'whisper-cli failed for chunk {index+1} at {index*chunk_seconds}s')
        try:
            text=txt.read_text(encoding='utf8',errors='replace').strip()
        except OSError as e:
            return incomplete(f'could not read transcript for chunk {index+1}: {e}')
        if sum(len(x) for x in parts)+len(text)+len(parts)>max_text:
            return incomplete(f'transcript exceeded {max_text} characters before chunk {index+1} could be recorded')
        if text:
            parts.append(text)
        completed+=1
        processed_seconds+=frames/rate
        if frames < chunk_seconds*rate:
            return {'state':'ok','engine':'whisper.cpp', 'model':model_name, 'backend':backend,'text':'\n'.join(parts),
                    'chunks_completed':completed, 'seconds_processed':processed_seconds,
                    'audio_duration_seconds':audio_duration}
    return incomplete(f'transcription stopped after {max_chunks*chunk_seconds}s resource limit')

def _ocr_tsv_lines(output, min_confidence=60.0):
    """Keep recognisable high-confidence words and reconstruct Tesseract lines."""
    lines={}
    for row in csv.DictReader(io.StringIO(output),delimiter='\t',quoting=csv.QUOTE_NONE):
        try:
            confidence=float(row.get('conf','-1'))
            if int(row.get('level','0')) != 5 or not math.isfinite(confidence) or confidence < min_confidence:continue
        except (TypeError,ValueError):continue
        word=' '.join((row.get('text') or '').split())
        if not word or not any(c.isalnum() for c in word):continue
        key=tuple(row.get(k,'') for k in ('page_num','block_num','par_num','line_num'))
        lines.setdefault(key,[]).append(word[:100])
    return [' '.join(words)[:1000] for words in lines.values() if words]

def _useful_ocr_line(line):
    """Drop isolated UI glyphs/digits that overwhelmed Reel subtitles.

    Keep multiword captions in Latin or Arabic scripts and long single words.
    Short single-word captions may still pass when repeated at two timestamps.
    """
    words=re.findall(r'[^\W\d_]+',line,flags=re.UNICODE)
    letters=sum(len(w) for w in words)
    return letters>=5 and (len(words)>=2 or len(words[0])>=6)


def _ocr_key(line):
    return ''.join(c for c in unicodedata.normalize('NFKC',line).casefold() if c.isalnum())


def read_ocr(frames,models,positions=None):
    config_path=models/'config.local.json'
    try:
        if config_path.is_symlink():raise ValueError('OCR settings file is a symlink')
        if config_path.exists():
            if not config_path.is_file() or config_path.stat().st_size>65536:raise ValueError('OCR settings file is invalid or too large')
            config=json.loads(config_path.read_text(encoding='utf8'))
            if not isinstance(config,dict):raise ValueError('OCR settings must be an object')
        else:config={}
    except (OSError,UnicodeError,ValueError) as e:
        return {'state':'unavailable','reason':f'Cannot safely read OCR settings: {e}','text':''}
    backend=config.get('ocr_backend','managed_local')
    if backend=='disabled':return {'state':'skipped','reason':'OCR is disabled in Settings','text':'','backend':backend}
    if backend=='api':return {'state':'unavailable','reason':'OCR API is not implemented; no data was uploaded','text':'','backend':backend}
    if backend=='managed_local':
        # Native model downloads can land in installed/, even when English and
        # Persian are bundled/. Look in both instead of ignoring installed models.
        model_dirs=[models/'bundled/ocr/tessdata',models/'installed/ocr/tessdata']
    elif backend=='custom_local':
        raw=config.get('ocr_custom_path','')
        if not isinstance(raw,str) or not raw.strip():return {'state':'unavailable','reason':'Custom OCR directory is empty or invalid','text':'','backend':backend}
        path=Path(raw)
        if '..' in path.parts:return {'state':'unavailable','reason':'Custom OCR path may not contain parent traversal','text':'','backend':backend}
        try:
            root=models.resolve(strict=True)
            candidate=path if path.is_absolute() else root/path
            relative=candidate.relative_to(root)
            cursor=root
            for component in relative.parts:
                cursor=cursor/component
                if cursor.is_symlink():raise ValueError('symlink in custom OCR path')
            tess=candidate.resolve(strict=True)
            if not tess.is_relative_to(root) or not tess.is_dir():raise ValueError('custom OCR directory must be inside models')
        except (OSError,ValueError) as e:
            return {'state':'unavailable','reason':f'Custom OCR directory invalid: {e}','text':'','backend':backend}
        model_dirs=[tess]
    else:return {'state':'unavailable','reason':'Unknown OCR backend in Settings','text':''}
    exe=shutil.which('tesseract')
    if not exe:return {'state':'unavailable','reason':'Tesseract executable missing','text':''}
    groups=[]
    # English and Arabic-script text are OCRed separately. Tesseract's joint
    # eng+fas pass tended to turn captions into spurious fragments.
    available={}
    for lang in ('eng','ara','fas'):
        for directory in model_dirs:
            trained=directory/(lang+'.traineddata')
            if not directory.is_symlink() and trained.is_file() and not trained.is_symlink():
                available[lang]=directory
                break
    for script in [('eng',),('ara','fas')]:
        by_directory={}
        for lang in script:
            if lang in available:
                by_directory.setdefault(available[lang],[]).append(lang)
        groups.extend((directory, languages) for directory,languages in by_directory.items())
    langs=[lang for lang in ('eng','ara','fas') if lang in available]
    if not langs:return {'state':'unavailable','reason':'English, Arabic and Persian OCR language data missing','text':''}
    if not frames:return {'state':'error','reason':'No usable video frames for OCR','text':''}
    found=[]; seen=set(); deferred={}; completed=0;failed=0;frames_failed=0;segments=[]
    # Caller supplies one time per successfully extracted image; legacy callers
    # can omit positions without changing the OCR success/error semantics.
    if positions is None:positions=[float(i) for i in range(len(frames))]
    for index,frame in enumerate(frames[:64]):
        per_frame=[]; frame_ok=False; deferred_on_frame=set()
        for tess,languages in groups:
            env={**os.environ,'TESSDATA_PREFIX':str(tess)}
            try:
                p=subprocess.run([exe,str(frame),'stdout','-l','+'.join(languages),'--psm','11',
                    '-c','tessedit_create_tsv=1'],capture_output=True,text=True,errors='replace',timeout=35,env=env)
            except (OSError,subprocess.TimeoutExpired):failed+=1;continue
            if p.returncode!=0:failed+=1;continue
            header=p.stdout.partition('\n')[0].split('\t')
            if not {'level','conf','text','page_num','block_num','par_num','line_num'}.issubset(header):
                failed+=1;continue
            frame_ok=True
            for line in _ocr_tsv_lines(p.stdout,min_confidence=65):
                key=_ocr_key(line)
                if not key:continue
                if _useful_ocr_line(line):
                    if key not in seen:
                        seen.add(key);found.append(line)
                    per_frame.append(line)
                elif key not in deferred_on_frame:
                    # Count recurrence across frames, not across language passes
                    # that happen to misread the same logo in one image.
                    deferred_on_frame.add(key)
                    count,original=deferred.get(key,(0,line))
                    deferred[key]=(count+1,original)
        if frame_ok:
            completed+=1
            if per_frame:
                segments.append({'time_seconds':round(float(positions[index]),2),
                                 'text':'\n'.join(dict.fromkeys(per_frame))[:1000]})
        else:frames_failed+=1
    for key,(count,line) in deferred.items():
        if count>=2 and len(_ocr_key(line))>=4 and key not in seen:
            seen.add(key);found.append(line)
    if not completed:return {'state':'error','reason':'OCR failed or timed out on all frames','text':'','languages':langs,
                             'frames_processed':0,'frames_failed':frames_failed}
    result={'state':'partial' if failed else ('ok' if found else 'no_text'),
            'engine':'tesseract','text':'\n'.join(found)[:12000], 'languages':langs,
            'frames_processed':completed,'frames_failed':frames_failed,
            'ocr_passes_failed':failed,'ocr_passes_per_frame':len(groups),
            'min_word_confidence':65,'backend':backend,'segments':segments}
    if failed:result['reason']=f'OCR failed or timed out on {failed} language pass(es) across {frames_failed} frame(s)'
    elif not result['text']:result['reason']='No text passed the OCR confidence threshold or subtitle-noise filter'
    return result

def vision(frames,models,enabled):
    if not enabled:return {'state':'skipped','reason':'text evidence sufficient','text':''}
    exe=engine('llama-mtmd-cli',models)
    base=models/'installed/vision'
    model=base/'SmolVLM2-256M-Video-Instruct-Q4_K_M.gguf'
    proj=base/'mmproj-SmolVLM2-256M-Video-Instruct-Q8_0.gguf'
    if not exe or not model.is_file() or not proj.is_file():
        return {'state':'unavailable','reason':'llama-mtmd-cli or vision weights/projector missing','text':''}
    if not frames:return {'state':'error','reason':'no usable video frames','text':''}
    try:
        p=subprocess.run([exe,'-m',str(model),'--mmproj',str(proj),'--no-mmproj-offload','--image',str(frames[0]),
           '-p','Describe the main subject and action seen in this Instagram Reel frame in one sentence. Do not guess unseen facts.',
           '-n','90','-c','1024','-ngl','0'],capture_output=True,text=True,timeout=180)
    except subprocess.TimeoutExpired:return {'state':'error','reason':'vision analysis timeout','text':''}
    if p.returncode:return {'state':'error','reason':f'vision engine exited {p.returncode}','text':''}
    return {'state':'ok','engine':'SmolVLM2/llama.cpp','text':p.stdout.strip()[-3000:]}

def build_evidence(caption, asr, ocr, visual):
    """Keep room for every source while retaining complete stage text in analysis.json."""
    sources=(('Caption',caption,700),('Speech',asr.get('text',''),2800),
             ('Screen text',ocr.get('text',''),900),('Visual',visual.get('text',''),400))
    excerpts=[]
    for label,value,limit in sources:
        if not isinstance(value,str) or not value.strip():continue
        text=value.strip()
        if len(text)>limit:
            # Both ends can contain useful context, especially for long speech.
            marker='\n[excerpt]\n'
            room=limit-len(marker)
            left=room*2//3
            text=text[:left]+marker+text[-(room-left):]
        excerpts.append(f'{label}: {text}')
    return '\n'.join(excerpts)

def analyze(root,rel,models,force_vision=False):
    media=checked_media(root,rel)
    media,manifest,folder=ensure_manifest(root,media)
    parsed=reel_url(manifest.get('source_url'))
    if not parsed:raise ValueError('Original URL required before analysis')
    scanned=probe(media)
    caption=str(manifest.get('caption') or manifest.get('description') or '')[:5000]
    with tempfile.TemporaryDirectory(prefix='gatherforge-reel-') as tmp:
        work=Path(tmp)
        asr=transcribe(media,models,work,scanned['audio'])
        frames=[]; extracted_at=[]
        duration=scanned['duration']
        positions=frame_positions(duration)
        for i,at in enumerate(positions):
            f=work/f'frame-{i}.png'
            if extract_frame(media,f,at):
                frames.append(f)
                extracted_at.append(at)
        ocr=read_ocr(frames,models,extracted_at)
        missing_frames=len(positions)-len(frames)
        ocr['frames_requested']=len(positions)
        ocr['frames_extraction_failed']=missing_frames
        if missing_frames and ocr.get('state') in ('ok','no_text','partial'):
            ocr['state']='partial'
            detail=f'{missing_frames} sampled frame(s) could not be extracted'
            ocr['reason']='; '.join(filter(None,[ocr.get('reason',''),detail]))
        text_evidence=' '.join([caption,asr.get('text',''),ocr.get('text','')]).strip()
        need_vision=force_vision or len(text_evidence)<95
        visual=vision(frames,models,need_vision)
    evidence=build_evidence(caption,asr,ocr,visual)
    data={
       'schema_version':1, 'id':parsed[1],'source_url':parsed[0], 'media_sha256':manifest['sha256'],
       'video_file':media.name,'scanned_at':now(),'status':'needs_review','category_suggested':'',
       'confidence':0, 'approved_category':'', 'approved_at':None, 'foldered_relative':'',
       'asr':asr,'ocr':ocr,'vision':visual,'duration':scanned['duration'],
       'evidence':evidence,'review_warnings':[
           f'{name}: {part.get("reason")}' for name,part in [('speech',asr),('ocr',ocr),('vision',visual)] if part.get('state') in ('unavailable','error','partial')
       ],
    }
    write_json(folder/'analysis.json',data)
    return {'id':parsed[1], 'video_relative':str(media.relative_to(root)),
        'analysis_relative':str((folder/'analysis.json').relative_to(root)),
        'text':evidence,'warnings':data['review_warnings']}

def record_category(root,rel,category,confidence):
    media=checked_media(root,rel)
    folder=media.parent;analysis=read_json(folder/'analysis.json')
    manifest=read_json(folder/'manifest.json')
    if not analysis or not manifest:raise ValueError('Analyze this Reel first')
    if analysis.get('media_sha256')!=manifest.get('sha256'):raise ValueError('Analysis/media mismatch')
    if category not in CATEGORIES:raise ValueError('Unsupported category')
    if not 0 <= float(confidence) <= 1:raise ValueError('Invalid confidence')
    analysis.update(category_suggested=category,confidence=float(confidence),status='needs_review',approved_category='',approved_at=None,foldered_relative='')
    write_json(folder/'analysis.json',analysis)
    return {'id':analysis['id'],'category':category,'confidence':float(confidence)}

def approve(root,rel,category):
    media=checked_media(root,rel)
    manifest=read_json(media.parent/'manifest.json');analysis=read_json(media.parent/'analysis.json')
    if not manifest or not analysis:raise ValueError('Missing video manifest or scan; analyze first')
    if category not in CATEGORIES:raise ValueError('Select a valid category')
    canonical=reel_url(manifest.get('source_url'))
    if not canonical or canonical[1]!=analysis.get('id'):raise ValueError('Source URL identity mismatch')
    digest=sha256(media)
    if (manifest.get('sha256')!=digest or analysis.get('media_sha256')!=digest or
        manifest.get('video_file')!=media.name or manifest.get('size_bytes')!=media.stat().st_size):
        raise ValueError('Video changed since scanning; refusing approval')
    if analysis.get('status')!='needs_review':raise ValueError('Analysis is not pending review')
    category_slug=re.sub(r'[^a-z0-9]+','-',category.lower()).strip('-')
    dest=root/'Organized'/category_slug/canonical[1]
    if any(p.is_symlink() for p in [root/'Organized',root/'Organized'/category_slug,dest]):
        raise ValueError('Symlink category destination')
    dest.mkdir(parents=True,exist_ok=True)
    link=dest/media.name
    if not link.exists():os.link(media,link)
    elif link.is_symlink() or sha256(link)!=digest:raise ValueError('Organized video name collision')
    if sha256(link)!=digest:raise ValueError('Organized copy verification failed')
    index={'schema_version':1,'source_url':canonical[0],'id':canonical[1],
       'category':category,'source_manifest':str((media.parent/'manifest.json').relative_to(root)),
       'video_file':media.name,'sha256':digest,'approved_at':now()}
    indexpath=dest/'manifest.json'
    if indexpath.is_file():
        existing=read_json(indexpath)
        if existing.get('sha256')!=digest or existing.get('source_url')!=canonical[0]:
            raise ValueError('Existing organized manifest conflict')
    write_json(indexpath,index)
    analysis.update(status='approved',approved_category=category,approved_at=index['approved_at'],foldered_relative=str(dest.relative_to(root)))
    write_json(media.parent/'analysis.json',analysis)
    manifest['organization']={'status':'approved','category':category,'folder_relative':str(dest.relative_to(root)),
       'media_sha256':digest,'approved_at':index['approved_at']}
    write_json(media.parent/'manifest.json',manifest)
    return {'id':canonical[1],'status':'approved','category':category,'folder':str(dest.relative_to(root))}

def approved_for_unsave(folder,root,canonical):
    """Only an explicit human approval + verified indexed hardlink unlocks the browser action."""
    try:
        folder=within(root,folder)
        v=read_json(folder/'manifest.json');a=read_json(folder/'analysis.json')
        url=reel_url(v.get('source_url'));wanted=reel_url(canonical)
        if not url or not wanted or url[1]!=wanted[1]:return False
        if a.get('status')!='approved' or not a.get('approved_at'):return False
        approval=v.get('organization') or {}
        if approval.get('status')!='approved' or approval.get('media_sha256')!=a.get('media_sha256'):return False
        category=a.get('approved_category')
        if category not in CATEGORIES or approval.get('category')!=category:return False
        rel=a.get('foldered_relative')
        if not isinstance(rel,str) or not rel.startswith('Organized/'):return False
        video=checked_media(root,str((folder/str(v.get('video_file'))).relative_to(root)))
        dest=within(root,root/rel)
        link=dest/video.name
        if link.is_symlink() or not link.is_file():return False
        if (a.get('media_sha256')!=v.get('sha256') or sha256(video)!=v.get('sha256')
            or sha256(link)!=v.get('sha256')):return False
        return True
    except (OSError,ValueError,TypeError,KeyError):return False

def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=['normalize','analyze','record-category','approve','status','doctor'])
    p.add_argument('--root',required=True);p.add_argument('--video',default='')
    p.add_argument('--models',default='');p.add_argument('--category',default='');p.add_argument('--confidence',type=float,default=0)
    p.add_argument('--force-vision',action='store_true')
    a=p.parse_args(argv)
    root=Path(a.root).expanduser().resolve()
    if not root.is_dir() or root.is_symlink():p.error('Reel root missing or unsafe')
    models=Path(a.models).expanduser().resolve() if a.models else root/'models'
    try:
        if a.action=='normalize':
            media=checked_media(root,a.video);media,manifest,folder=ensure_manifest(root,media)
            result={'id':manifest.get('id'),'video_relative':str(media.relative_to(root)),
                    'manifest_relative':str((folder/'manifest.json').relative_to(root))}
        elif a.action=='analyze':result=analyze(root,a.video,models,a.force_vision)
        elif a.action=='record-category':result=record_category(root,a.video,a.category,a.confidence)
        elif a.action=='approve':result=approve(root,a.video,a.category)
        elif a.action=='status':
            media=checked_media(root,a.video);result=read_json(media.parent/'analysis.json')
        else:
            result={'ffmpeg':bool(shutil.which('ffmpeg')),'ffprobe':bool(shutil.which('ffprobe')),
              'tesseract':bool(shutil.which('tesseract')),'whisper_cli':bool(engine('whisper-cli',models)),
              'vision_cli':bool(engine('llama-mtmd-cli',models))}
        print(json.dumps(result,ensure_ascii=False))
    except (OSError,ValueError,RuntimeError,subprocess.TimeoutExpired) as exc:
        print(f'ERROR: {exc}',file=sys.stderr);return 2
    return 0
if __name__=='__main__':raise SystemExit(main())
