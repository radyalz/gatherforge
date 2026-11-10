import csv
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / 'tools'))
import reel_analysis as reel

HEADER = 'level\tpage_num\tblock_num\tpar_num\tline_num\tconf\ttext\n'
TSV = HEADER + '5\t1\t1\t1\t1\t94\tHello\n5\t1\t1\t1\t1\t12\tNoise\n5\t1\t1\t1\t1\t85\tworld\n'

class OcrTests(unittest.TestCase):
    def test_confidence_and_line_order(self):
        self.assertEqual(reel._ocr_tsv_lines(TSV), ['Hello world'])
        self.assertEqual(reel._ocr_tsv_lines(HEADER + '5\t1\t1\t1\t1\tnanx\tbad\n'), [])

    def test_unmatched_quote_does_not_consume_following_rows(self):
        data = HEADER + '5\t1\t1\t1\t1\t90\t"Hello\n5\t1\t1\t1\t2\t95\tWorld\n'
        self.assertEqual(reel._ocr_tsv_lines(data), ['"Hello', 'World'])

    def test_frame_sampling_is_bounded_and_includes_start_end(self):
        points = reel.frame_positions(20)
        self.assertEqual(len(points), 11)
        self.assertEqual(points[0], 0)
        self.assertTrue(19.0 < points[-1] < 20)
        self.assertEqual(reel.frame_positions(float('nan')), [0.0])
        self.assertEqual(reel.frame_positions(0), [0.0])
        self.assertGreaterEqual(len(reel.frame_positions(95)), 45)
        self.assertLessEqual(len(reel.frame_positions(7200)), 64)

    def test_one_language_dedup_and_partial_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp)/'models'; tess=models/'bundled/ocr/tessdata'; tess.mkdir(parents=True)
            (tess/'eng.traineddata').touch()
            ok=subprocess.CompletedProcess([],0,TSV,'')
            failure=subprocess.CompletedProcess([],1,'','bad')
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), patch.object(reel.subprocess,'run',side_effect=[ok,ok,failure]) as call:
                result=reel.read_ocr([Path('a'),Path('b'),Path('c')],models)
            self.assertEqual(result['text'], 'Hello world')
            self.assertEqual(result['languages'], ['eng'])
            self.assertEqual(result['frames_failed'], 1)
            self.assertIn('eng',call.call_args_list[0].args[0])
            self.assertEqual(result['state'],'partial')

    def test_failures_and_missing_frames_are_not_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); tess=models/'bundled/ocr/tessdata'; tess.mkdir(parents=True)
            (tess/'eng.traineddata').touch()
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'):
                self.assertEqual(reel.read_ocr([],models)['state'],'error')
                with patch.object(reel.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'bad output','')):
                    self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'error')
            with patch.object(reel.shutil,'which',return_value=None):
                self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'unavailable')

    def test_successful_empty_result_is_no_text(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); tess=models/'bundled/ocr/tessdata'; tess.mkdir(parents=True)
            (tess/'eng.traineddata').touch()
            empty=subprocess.CompletedProcess([],0,HEADER,'')
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), patch.object(reel.subprocess,'run',return_value=empty):
                self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'no_text')

    def test_disabled_api_and_invalid_settings_never_run_ocr(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); settings=models/'config.local.json'
            with patch.object(reel.subprocess,'run') as run:
                for backend,expected in [('disabled','skipped'),('api','unavailable'),('bogus','unavailable')]:
                    settings.write_text(json.dumps({'ocr_backend':backend}))
                    self.assertEqual(reel.read_ocr([Path('a')],models)['state'],expected)
                settings.write_text('{invalid')
                self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'unavailable')
                settings.unlink(); (models/'actual').write_text('{}'); settings.symlink_to(models/'actual')
                self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'unavailable')
                run.assert_not_called()

    def test_managed_installed_only_is_detected(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); tess=models/'installed/ocr/tessdata'; tess.mkdir(parents=True)
            (tess/'eng.traineddata').touch()
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), \
                 patch.object(reel.subprocess,'run',return_value=subprocess.CompletedProcess([],0,TSV,'')) as run:
                result=reel.read_ocr([Path('a')],models,[42.5])
            self.assertEqual(result['state'],'ok')
            self.assertEqual(result['languages'],['eng'])
            self.assertEqual(result['segments'][0]['time_seconds'],42.5)
            self.assertEqual(run.call_args.kwargs['env']['TESSDATA_PREFIX'],str(tess))

    def test_arabic_installed_and_english_bundled_use_separate_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); bundled=models/'bundled/ocr/tessdata'; bundled.mkdir(parents=True)
            installed=models/'installed/ocr/tessdata'; installed.mkdir(parents=True)
            (bundled/'eng.traineddata').touch(); (installed/'ara.traineddata').touch()
            english=HEADER+'5\t1\t1\t1\t1\t98\tpublic\n5\t1\t1\t1\t1\t97\tschool\n5\t1\t1\t1\t1\t96\tsystem\n5\t1\t2\t1\t1\t90\tKELL\n'
            arabic=HEADER+'5\t1\t1\t1\t1\t95\tالسنوات\n5\t1\t1\t1\t1\t95\tالثلاث\n'
            calls=[]
            def run(args,**kwargs):
                calls.append((args[args.index('-l')+1],kwargs['env']['TESSDATA_PREFIX']))
                return subprocess.CompletedProcess([],0,english if 'eng' in args else arabic,'')
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), \
                 patch.object(reel.subprocess,'run',side_effect=run):
                result=reel.read_ocr([Path('a')],models,[8.5])
            self.assertEqual(result['state'],'ok')
            self.assertEqual(result['languages'],['eng','ara'])
            self.assertIn('public school system',result['text'])
            self.assertIn('السنوات الثلاث',result['text'])
            self.assertNotIn('KELL',result['text'])
            self.assertEqual(calls,[('eng',str(bundled)),('ara',str(installed))])
            self.assertEqual(result['ocr_passes_per_frame'],2)
            self.assertEqual(result['frames_processed'],1)

    def test_recurring_short_subtitle_not_repeated_language_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            models=Path(tmp); tess=models/'bundled/ocr/tessdata'; tess.mkdir(parents=True)
            (tess/'eng.traineddata').touch(); (tess/'ara.traineddata').touch()
            short=HEADER+'5\t1\t1\t1\t1\t96\tCity\n'
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), \
                 patch.object(reel.subprocess,'run',return_value=subprocess.CompletedProcess([],0,short,'')):
                single=reel.read_ocr([Path('a')],models)
                two=reel.read_ocr([Path('a'),Path('b')],models)
            self.assertEqual(single['text'],'')
            self.assertEqual(two['text'],'City')

    def test_custom_directory_constrained_to_models(self):
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp); models=base/'models'; models.mkdir()
            tess=models/'custom'; tess.mkdir(); (tess/'fas.traineddata').touch()
            config=models/'config.local.json'
            def configure(path): config.write_text(json.dumps({'ocr_backend':'custom_local','ocr_custom_path':str(path)}))
            with patch.object(reel.shutil,'which',return_value='/usr/bin/tesseract'), patch.object(reel.subprocess,'run',return_value=subprocess.CompletedProcess([],0,TSV,'')) as run:
                configure('custom'); result=reel.read_ocr([Path('a')],models)
                self.assertEqual(result['state'],'ok'); self.assertEqual(result['languages'],['fas'])
                self.assertEqual(run.call_args.kwargs['env']['TESSDATA_PREFIX'],str(tess))
                run.reset_mock()
                for path in ('../outside',base,models/'missing'):
                    configure(path); self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'unavailable')
                alias=models/'alias'; alias.symlink_to(tess, target_is_directory=True)
                configure('alias'); self.assertEqual(reel.read_ocr([Path('a')],models)['state'],'unavailable')
                run.assert_not_called()
