import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / 'tools'))
import reel_analysis as r


class EvidenceTests(unittest.TestCase):
    def test_long_sources_do_not_hide_other_evidence(self):
        speech = 'beginning ' + 'x' * 6000 + ' ending'
        evidence = r.build_evidence('caption ' * 900, {'text': speech},
                                    {'text': 'screen label'}, {'text': 'visual subject'})
        self.assertLessEqual(len(evidence), 5000)
        self.assertIn('Speech: beginning', evidence)
        self.assertIn('ending', evidence)
        self.assertIn('Screen text: screen label', evidence)
        self.assertIn('Visual: visual subject', evidence)
        self.assertIn('[excerpt]', evidence)
        self.assertEqual(speech, 'beginning ' + 'x' * 6000 + ' ending')

    def test_empty_sources_do_not_generate_placeholder_evidence(self):
        self.assertEqual(r.build_evidence('', {'text': ''}, {}, {}), '')

    def test_unextracted_frames_make_ocr_partial(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); media = root / 'video.mp4'; media.touch()
            manifest = {'source_url': 'https://www.instagram.com/reel/ABC123/',
                        'sha256': 'fixture', 'caption': ''}
            with patch.object(r, 'checked_media', return_value=media), \
                 patch.object(r, 'ensure_manifest', return_value=(media, manifest, root)), \
                 patch.object(r, 'probe', return_value={'duration': 100, 'audio': True}), \
                 patch.object(r, 'transcribe', return_value={'state': 'ok', 'text': 'full transcript'}), \
                 patch.object(r, 'frame_positions', return_value=[0,25,50,75,99.8]), \
                 patch.object(r, 'extract_frame', side_effect=[True, False, True, False, True]), \
                 patch.object(r, 'read_ocr', return_value={'state': 'ok', 'text': 'visible'}), \
                 patch.object(r, 'vision', return_value={'state': 'skipped', 'text': ''}):
                r.analyze(root, 'video.mp4', root)
            analysis = json.loads((root / 'analysis.json').read_text())
            self.assertEqual(analysis['asr']['text'], 'full transcript')
            self.assertEqual(analysis['ocr']['state'], 'partial')
            self.assertEqual(analysis['ocr']['frames_requested'], 5)
            self.assertEqual(analysis['ocr']['frames_extraction_failed'], 2)
            self.assertIn('could not be extracted', analysis['ocr']['reason'])
            self.assertTrue(any('ocr:' in warning for warning in analysis['review_warnings']))

    def test_long_videos_are_sampled_through_the_end(self):
        points = r.frame_positions(7200)
        self.assertEqual(points[0], 0)
        self.assertGreater(points[-1], 7199)
        self.assertLess(points[-1], 7200)


if __name__ == '__main__':
    unittest.main()
