"""Source-level guardrails for the Forge UI migration."""
import unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
class ForgeEdition(unittest.TestCase):
    def test_theme_is_opaque_and_sharp(self):
        code = (ROOT/'src/theme.rs').read_text()
        self.assertIn('pub const RADIUS: u8 = 2;', code)
        self.assertIn('pub const GOLD:', code)
        self.assertIn('Color32::from_rgb(21, 9, 12)', code)
        self.assertNotIn('Compact glass design system', code)
    def test_unified_media_entrypoint(self):
        code = (ROOT/'src/app.rs').read_text()
        self.assertIn('Media,', code)
        self.assertIn('fn media_library(', code)
        self.assertIn('"FORGE / WORKSPACE"', code)
        self.assertIn('Page::Media => self.media_library(ui)', code)
    def test_reference_and_rune_are_isolated(self):
        self.assertTrue((ROOT/'docs/design/forge_reference_dark.png').is_file())
        self.assertTrue((ROOT/'ui/forge-rune/Cargo.toml').is_file())
        deps=(ROOT/'Cargo.toml').read_text()
        self.assertNotIn('gpui-kit',deps)
        self.assertNotIn('rune-kit',deps)
if __name__=='__main__':unittest.main()
