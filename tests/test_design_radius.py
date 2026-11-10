"""Design-radius regression tests for the 8px music-card exception (GATHERFORGE_CORNER_RULE_V1).

Source scanning is a guardrail, not a live render test.
"""
import re
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class CornerRadiusRule(unittest.TestCase):
    def test_theme_defines_all_egui_widget_and_popup_corners(self):
        theme=(ROOT/'src/theme.rs').read_text()
        self.assertRegex(theme,r'pub const RADIUS:\s*u8\s*=\s*2\s*;')
        for key in ('visuals.widgets.noninteractive.corner_radius',
                    'visuals.widgets.inactive.corner_radius',
                    'visuals.widgets.hovered.corner_radius',
                    'visuals.widgets.active.corner_radius', 'visuals.widgets.open.corner_radius',
                    'visuals.window_corner_radius', 'visuals.menu_corner_radius'):
            self.assertRegex(theme,rf'{re.escape(key)}\s*=\s*egui::CornerRadius::same\(RADIUS\)')

    def test_no_oversized_egui_corner_radius(self):
        for path in (ROOT/'src').rglob('*.rs'):
            source=path.read_text()
            self.assertNotRegex(source,r'CornerRadius::same\(\s*[4-9]\d*\s*\)',str(path))
            self.assertNotRegex(source,r'CornerRadius::same\(\s*[1-9]\d{1,}\s*\)',str(path))
            for m in re.finditer(r'corner_radius\s*\(\s*(?:egui::CornerRadius::same\()?\s*(\d+)', source):
                self.assertLess(float(m.group(1)),4,str(path))
            for m in re.finditer(r'rect_filled\([^;]+?,\s*(\d+(?:\.\d+)?)\s*,',source):
                self.assertLess(float(m.group(1)),4,str(path))

    def test_music_card_exception_is_explicit_and_scoped(self):
        cards = (ROOT/'src/components/tracks.rs').read_text()
        self.assertRegex(cards, r'const MUSIC_CARD_RADIUS:\s*f32\s*=\s*8\.0;')
        self.assertEqual(cards.count('p.rect_filled(rect, MUSIC_CARD_RADIUS,'), 1)
        self.assertEqual(cards.count('p.rect_filled(art, MUSIC_CARD_RADIUS,'), 1)
        for path in (ROOT/'src').rglob('*.rs'):
            if path.name != 'tracks.rs':
                self.assertNotIn('MUSIC_CARD_RADIUS', path.read_text(), str(path))

    def test_browser_owned_controls_are_sharp(self):
        for path in (ROOT/'tools').rglob('*.py'):
            source=path.read_text()
            for m in re.finditer(r'border-radius\s*:\s*(\d+(?:\.\d+)?)px',source):
                self.assertLess(float(m.group(1)),4,str(path))
        browser=(ROOT/'tools/instagram_saved.py').read_text()
        self.assertEqual(browser.count('border-radius:2px'),3)

    def test_svg_rectangular_corners_are_sharp(self):
        icon=ET.parse(ROOT/'assets/gatherforge.svg')
        svg='{http://www.w3.org/2000/svg}'
        for rect in icon.iter(svg+'rect'):
            for axis in ('rx','ry'):
                if axis in rect.attrib:
                    self.assertLess(float(rect.attrib[axis]),4,rect.attrib)
        self.assertTrue(any(rect.get('rx') == '2' for rect in icon.iter(svg+'rect')))

    def test_policy_is_shipped_and_installed(self):
        docs=(ROOT/'docs/DESIGN_RULES.md').read_text()
        agents=(ROOT/'AGENTS.md').read_text()
        readme=(ROOT/'README.md').read_text()
        installer=(ROOT/'scripts/install_into_repo.sh').read_text()
        for material in (docs,agents,readme):
            self.assertIn('GATHERFORGE_CORNER_RULE_V1',material)
        self.assertIn('strictly less than 4 px',docs)
        self.assertIn('2 px',docs)
        self.assertIn('SOURCE=',installer)
        self.assertIn('README.md',installer)



if __name__=='__main__':unittest.main()
