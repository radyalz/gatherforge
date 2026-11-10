import sys, tempfile, unittest, wave, os, shutil, subprocess, json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).parents[1] / 'tools'))
import reel_analysis as r

class TranscriptionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root=Path(self.temp.name)
        self.models=root/'models'; self.work=root/'work'; self.work.mkdir()
        weights=self.models/'bundled/asr/ggml-tiny-q5_1.bin'
        weights.parent.mkdir(parents=True); weights.write_bytes(b'fixture')
        self.media=root/'clip.mp4';self.media.write_bytes(b'fixture')

    def run_chunks(self, lengths, fail=None):
        offsets=[]
        self.models_seen=[]
        def execute(args, **kwargs):
            if args[0]=='ffmpeg':
                offset=int(args[args.index('-ss')+1]);offsets.append(offset)
                idx=offset//60; frames=lengths[idx] if idx<len(lengths) else 0
                with wave.open(str(self.work/'speech-chunk.wav'),'wb') as w:
                    w.setnchannels(1);w.setsampwidth(2);w.setframerate(16000)
                    w.writeframes(bytes(2)*frames)
                return SimpleNamespace(returncode=0)
            idx=len(offsets)-1
            self.models_seen.append(args[args.index('-m')+1])
            if fail==idx:return SimpleNamespace(returncode=1)
            (self.work/'speech-chunk.txt').write_text(f'chunk {idx+1}')
            return SimpleNamespace(returncode=0)
        with patch.object(r,'engine',return_value='whisper-cli'), patch.object(r.shutil,'which',side_effect=lambda name: '/usr/bin/ffmpeg' if name == 'ffmpeg' else None), patch.object(r.subprocess,'run',side_effect=execute):
            result=r.transcribe(self.media,self.models,self.work,True)
        return result,offsets

    def test_long_audio_is_transcribed_through_last_partial_chunk(self):
        result,offsets=self.run_chunks([960000,960000,16000])
        self.assertEqual(offsets,[0,60,120]);self.assertEqual(result['state'],'ok')
        self.assertEqual(result['text'],'chunk 1\nchunk 2\nchunk 3')
        self.assertEqual(result['chunks_completed'],3)

    def test_failure_reports_partial_and_preserves_completed_text(self):
        result,offsets=self.run_chunks([960000,16000],fail=1)
        self.assertEqual(offsets,[0,60]);self.assertEqual(result['state'],'partial')
        self.assertEqual(result['text'],'chunk 1');self.assertEqual(result['seconds_processed'],60)

    def test_exact_boundary_checks_for_eof(self):
        result,offsets=self.run_chunks([960000])
        self.assertEqual(offsets,[0,60]);self.assertEqual(result['state'],'ok')
        self.assertEqual(result['text'],'chunk 1')

    @unittest.skipUnless(shutil.which('ffmpeg'), 'ffmpeg required')
    def test_real_ffmpeg_covers_start_middle_and_end(self):
        audio=self.work/'input.wav'
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i',
            r"aevalsrc=if(lt(t\,60)\,0.1\,if(lt(t\,120)\,0.2\,0.3)):s=16000:d=125",
            '-c:a','pcm_s16le','-y',str(audio)],check=True,timeout=30)
        fake=self.work/'whisper-cli'
        fake.write_text("#!/usr/bin/env python3\nimport sys,wave,struct\na=sys.argv; f=wave.open(a[a.index('-f')+1],'rb'); x=f.readframes(min(f.getnframes(),16000)); v=struct.unpack('<%dh'%(len(x)//2),x); level=round(sum(abs(i) for i in v)/len(v)/32767,1); open(a[a.index('-of')+1]+'.txt','w').write(str(level))\n")
        fake.chmod(0o755)
        with patch.object(r,'engine',return_value=str(fake)):
            result=r.transcribe(audio,self.models,self.work,True)
        self.assertEqual(result['state'],'ok')
        self.assertEqual(result['text'].splitlines(),['0.1','0.2','0.3'])
        self.assertAlmostEqual(result['seconds_processed'],125,places=2)

    def test_english_language_is_passed_to_whisper(self):
        self.write_settings(speech_language='en')
        self.models_seen=[]
        launched=[]
        def execute(args, **kwargs):
            if args[0]=='ffmpeg':
                with wave.open(str(self.work/'speech-chunk.wav'),'wb') as w:
                    w.setnchannels(1);w.setsampwidth(2);w.setframerate(16000)
                    w.writeframes(bytes(2)*16000)
            else:
                launched.append(args)
                (self.work/'speech-chunk.txt').write_text('We are growing')
            return SimpleNamespace(returncode=0)
        with patch.object(r,'engine',return_value='whisper-cli'), \
             patch.object(r.shutil,'which',side_effect=lambda name: '/usr/bin/ffmpeg' if name == 'ffmpeg' else None), \
             patch.object(r.subprocess,'run',side_effect=execute):
            result=r.transcribe(self.media,self.models,self.work,True)
        self.assertEqual(result['state'],'ok')
        self.assertEqual(launched[0][launched[0].index('-l')+1],'en')

    def test_managed_base_and_small_can_be_selected(self):
        for model in ('base','small'):
            name=f'ggml-{model}-q5_1.bin'
            weight=self.models/'installed/asr'/name
            weight.parent.mkdir(parents=True,exist_ok=True)
            weight.write_bytes(b'model')
            self.write_settings(speech_model=model)
            result,_=self.run_chunks([16000])
            self.assertEqual(result['state'],'ok')
            self.assertEqual(result['model'],name)
            self.assertEqual(self.models_seen,[str(weight)])

    def test_managed_tiny_can_be_in_installed_folder(self):
        (self.models/'bundled/asr/ggml-tiny-q5_1.bin').unlink()
        weight=self.models/'installed/asr/ggml-tiny-q5_1.bin'
        weight.parent.mkdir(parents=True,exist_ok=True)
        weight.write_bytes(b'model')
        result,_=self.run_chunks([16000])
        self.assertEqual(result['state'],'ok')
        self.assertEqual(self.models_seen,[str(weight)])

    def test_unknown_managed_model_does_not_run(self):
        self.write_settings(speech_model='../../../bad')
        with patch.object(r.subprocess,'run') as called:
            result=r.transcribe(self.media,self.models,self.work,True)
        self.assertEqual(result['state'],'unavailable')
        called.assert_not_called()

    def test_invalid_language_does_not_run(self):
        self.write_settings(speech_language='-f /tmp/foo')
        with patch.object(r.subprocess,'run') as called:
            result=r.transcribe(self.media,self.models,self.work,True)
        self.assertEqual(result['state'],'unavailable')
        called.assert_not_called()

    def test_total_deadline_is_explicit_error(self):
        with patch.object(r,'engine',return_value='whisper-cli'), \
             patch.object(r.shutil,'which',side_effect=lambda name: '/usr/bin/ffmpeg' if name == 'ffmpeg' else None), \
             patch('time.monotonic',side_effect=[0,1201]):
            result=r.transcribe(self.media,self.models,self.work,True)
        self.assertEqual(result['state'],'error')
        self.assertIn('1200-second',result['reason'])
        self.assertEqual(result['seconds_processed'],0)

    def write_settings(self, **settings):
        (self.models/'config.local.json').write_text(json.dumps(settings))

    def test_disabled_and_api_do_not_launch_any_process(self):
        for backend, state in [('disabled','skipped'),('api','unavailable')]:
            self.write_settings(speech_backend=backend)
            with patch.object(r.subprocess,'run') as called:
                result=r.transcribe(self.media,self.models,self.work,True)
                called.assert_not_called()
            self.assertEqual(result['state'],state)
            self.assertEqual(result['backend'],backend)

    def test_custom_model_inside_root_is_selected_and_named(self):
        custom=self.models/'installed/whisper/my-model.gguf'
        custom.parent.mkdir(parents=True);custom.write_bytes(b'model')
        self.write_settings(speech_backend='custom_local',speech_custom_path=str(custom))
        result,_=self.run_chunks([16000])
        self.assertEqual(result['state'],'ok')
        self.assertEqual(result['backend'],'custom_local')
        self.assertEqual(result['model'],'my-model.gguf')
        self.assertEqual(self.models_seen,[str(custom)])

    def test_custom_path_rejects_missing_outside_parent_and_symlink(self):
        outside=self.work/'outside.bin';outside.write_bytes(b'model')
        link=self.models/'linked.bin';link.symlink_to(outside)
        for value in ('', 'missing.bin', '../outside.bin', str(outside), str(link)):
            self.write_settings(speech_backend='custom_local',speech_custom_path=value)
            with patch.object(r.subprocess,'run') as called:
                result=r.transcribe(self.media,self.models,self.work,True)
                called.assert_not_called()
            self.assertEqual(result['state'],'unavailable',value)
            self.assertIn('custom speech model',result['reason'],value)

    def test_invalid_or_symlinked_config_fails_closed(self):
        config=self.models/'config.local.json'
        config.write_text('{invalid')
        with patch.object(r.subprocess,'run') as called:
            result=r.transcribe(self.media,self.models,self.work,True)
            called.assert_not_called()
        self.assertEqual(result['state'],'unavailable')
        config.unlink();config.symlink_to(self.work/'missing.json')
        result=r.transcribe(self.media,self.models,self.work,True)
        self.assertEqual(result['state'],'unavailable')
        self.assertIn('symlink',result['reason'])

    def test_no_audio_is_skipped(self):
        result=r.transcribe(self.media,self.models,self.work,False)
        self.assertEqual(result['state'],'skipped')

if __name__=='__main__':unittest.main()
