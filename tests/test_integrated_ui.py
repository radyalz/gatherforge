"""Checks for the v3.8 integrated music + Instagram download surfaces.

These are structural tests, NOT a substitute for cargo check on Fedora.
"""
from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
APP = (ROOT / 'src/app.rs').read_text()
JOBS = (ROOT / 'src/jobs.rs').read_text()


def method(name):
    """Extract source between adjacent application methods for focused assertions."""
    m = re.search(rf'\n\s*fn\s+{re.escape(name)}\s*\(', APP)
    if not m:
        raise AssertionError(f'Rust method {name} missing')
    following = re.search(r'\n    fn [a-zA-Z_][a-zA-Z_0-9]*\s*\(', APP[m.end():])
    return APP[m.start():m.end() + following.start()] if following else APP[m.start():]


def squeezed(code):
    """Drop formatting whitespace so assertions survive `cargo fmt`."""
    return re.sub(r'\s+','',code)


class IntegratedDownloadUI(unittest.TestCase):
    def test_version(self):
        self.assertEqual(tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version'],'3.10.0')

    def test_music_screen_contains_its_queue_and_no_redirect(self):
        code = squeezed(method('downloads'))
        self.assertIn('self.queue_board(ui,QueueScope::Music)',code)
        self.assertIn('self.start_download()',code)
        self.assertIn(squeezed('Start downloads'),code)
        self.assertNotIn('self.page=Page::Manager',squeezed(method('start_download')))

    def test_reel_screen_contains_its_queue_and_both_entry_modes(self):
        code = squeezed(method('reel_downloads'))
        self.assertIn('self.queue_board(ui,QueueScope::Reels)',code)
        self.assertIn('ReelSource::Direct',code)
        self.assertIn('ReelSource::Saved',code)
        self.assertIn('self.open_saved_browser()',code)
        self.assertNotIn('self.page=Page::Manager',squeezed(method('open_saved_browser')))

    def test_batch_actions_and_safe_history(self):
        code = squeezed(method('queue_board'))
        for part in ('Cancel batch','Retry failed','Remove batch','Details & source', 'self.jobs.cancel(id)',
                     'self.jobs.retry(id)','self.jobs.remove(id)','self.jobs.cancel_batch(id)',
                     'self.jobs.retry_batch(id)','self.jobs.remove_batch(id)','Clear completed'):
            self.assertIn(squeezed(part),code)
        self.assertIn(squeezed('pub fn clear_completed_for(&mut self,reels:bool,all:bool)'),squeezed(JOBS))
        self.assertNotIn('remove_file(',code)
        self.assertNotIn('remove_dir(',code)

    def test_manager_is_optional_and_not_a_primary_tab(self):
        code=squeezed(method('sidebar'))
        self.assertIn('Page::Manager.label()',code)
        self.assertIn(squeezed('if p==Page::Downloads'),code)
        self.assertIn(squeezed('if p==Page::ReelDownload'),code)
        # The former standalone manager must not be the default result of submit.
        self.assertNotIn('self.page=Page::Manager',squeezed(method('start_download')))

    def test_theme_and_icons(self):
        theme=(ROOT/'src/theme.rs').read_text()
        icons=squeezed((ROOT/'src/components/chrome.rs').read_text())
        self.assertRegex(theme,r'pub const RADIUS:\s*u8\s*=\s*2\s*;')
        self.assertIn(squeezed('"Music downloads"=>{'),icons)
        self.assertIn(squeezed('"Reel downloads"=>{'),icons)
        self.assertIn(squeezed('"All activity"=>{'),icons)

    def test_required_existing_methods_preserved(self):
        for name in ('toolbar','track_list','inspector','library','artists','reel_manifest','choose_reel_folder',
                     'start_download','queue_board','download_manager','download_heading','reel_downloads','downloads'):
            self.assertRegex(APP,rf'\bfn\s+{name}\s*\(')


if __name__=='__main__':unittest.main()
