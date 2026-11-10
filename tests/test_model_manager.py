"""Offline checks for native Rust model integration and app-contained layout.
Tests do not pretend to compile Rust or download model binaries.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('gf_vendor',ROOT/'tools/vendor_light_models.py')
vm=importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)

class ModelPackageTests(unittest.TestCase):
    def test_default_catalog_and_sources(self):
        items=json.loads((ROOT/'models/catalog.json').read_text())['models']
        self.assertEqual(len([m for m in items if m['default_install']]),4)
        self.assertEqual({m['id'] for m in items if m['default_install']},
                         {'whisper-tiny-q5-multi','ocr-eng-fast','ocr-fas-fast','ocr-ara-fast'})
        for m in items:
            self.assertTrue(m['url'].startswith(('https://huggingface.co/', 'https://raw.githubusercontent.com/')))
            self.assertNotIn('..',Path(m['relative_path']).parts)
        self.assertLess(max(m['size_mb'] for m in items if m['repo_bundlable']),100)
    def test_native_panel_no_python_sidecar(self):
        rust=(ROOT/'src/ai_models.rs').read_text()
        self.assertIn('ureq::AgentBuilder',rust)
        self.assertIn('models/config.local.json',rust)
        self.assertIn('models/bundled',rust)
        self.assertIn('models/installed',rust)
        self.assertIn('auto_download: true',rust)
        self.assertIn('pub fn tick(&mut self',rust)
        self.assertIn('Verified bundled model',rust)
        self.assertIn('sha2::Sha256',rust)
        self.assertIn('theme::RADIUS',rust)
        self.assertNotIn('Command::new("python3")',rust)
        self.assertNotIn('model_manager.py',rust)
        app=(ROOT/'src/app.rs').read_text()
        self.assertIn('self.ai_models.tick(ctx);',app)
        self.assertIn('self.ai_models.ui(ui)',app)
    def test_config_no_raw_token_no_auto_unsave(self):
        cfg=json.loads((ROOT/'models/config.example.json').read_text())
        self.assertTrue(cfg['auto_download'])
        self.assertFalse(cfg['auto_unsave'])
        self.assertFalse(cfg['send_private_media_to_api'])
        self.assertNotIn('api_key',cfg)
        self.assertEqual(cfg['api_key_env'],'GATHERFORGE_AI_API_KEY')
    def test_repo_allows_small_assets_but_ignores_privates(self):
        text=(ROOT/'.gitignore').read_text()
        for expected in ('!/models/bundled/asr/ggml-tiny-q5_1.bin',
                         '!/models/bundled/ocr/tessdata/eng.traineddata',
                         '!/models/bundled/ocr/tessdata/fas.traineddata',
                         '!/models/bundled/ocr/tessdata/ara.traineddata',
                         '/models/config.local.json','/models/installed/','*.gguf'):
            self.assertIn(expected,text)
    def test_vendor_hash_verification(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'file.bin'
            data=b'fixture data'
            path.write_bytes(data)
            model={'hash_type':'sha256','hash':hashlib.sha256(data).hexdigest()}
            self.assertTrue(vm.digest(path,model))
            model['hash']='0'*64
            self.assertFalse(vm.digest(path,model))
            model['hash_type']='git_blob_sha1'
            model['hash']=hashlib.sha1(f'blob {len(data)}\0'.encode()+data).hexdigest()
            self.assertTrue(vm.digest(path,model))
    def test_vendor_safe_download_and_refusal(self):
        with tempfile.TemporaryDirectory() as d:
            fake=b'verified-model-fixture'
            item={'url':'https://raw.githubusercontent.com/test/file','relative_path':'asr/fixture.bin',
                  'hash_type':'sha256','hash':hashlib.sha256(fake).hexdigest(), 'size_mb':1,'id':'fixture'}
            class Resp:
                def __enter__(self): return self
                def __exit__(self,*args): return False
                def __init__(self): self.done=False
                def read(self,n):
                    if self.done: return b''
                    self.done=True; return fake
            with patch.object(vm,'BUNDLED',Path(d)), patch.object(vm.urllib.request,'urlopen',return_value=Resp()):
                self.assertTrue(vm.vendor(item))
                self.assertEqual((Path(d)/'asr/fixture.bin').read_bytes(),fake)
            item['hash']='0'*64
            with patch.object(vm,'BUNDLED',Path(d)):
                with self.assertRaises(ValueError):vm.vendor(item)
    def test_documentation_and_no_inference_promise(self):
        readme=(ROOT/'README.md').read_text()
        self.assertIn('speech',readme.lower())
        self.assertIn('reel intelligence',readme.lower())
        self.assertIn('/mnt/shared/Projects/MusicOrganizer',readme)
        policy=(ROOT/'docs/AI_AGENT_RULE.md').read_text()
        self.assertIn('GATHERFORGE_APP_CONTAINED_MODELS_V2',policy)

if __name__=='__main__':unittest.main()
