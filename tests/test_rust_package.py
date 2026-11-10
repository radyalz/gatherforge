"""Static regression checks for the omitted Rust UI methods and queue syntax."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class TestRustSource(unittest.TestCase):
    def test_methods_used_by_library_exist(self):
        app = (ROOT / "src/app.rs").read_text(encoding="utf-8")
        for name in ("toolbar", "track_list", "inspector", "download_manager"):
            self.assertRegex(app, rf"\bfn\s+{name}\s*\(", name)
        self.assertIn("self.toolbar(ui);", app)
        self.assertIn("self.track_list(ui, duplicates_only);", app)
        self.assertIn("self.inspector(ui);", app)

    def test_self_method_calls_have_definitions(self):
        app = (ROOT / "src/app.rs").read_text(encoding="utf-8")
        called = set(re.findall(r"\bself\.([a-zA-Z_][a-zA-Z_0-9]*)\s*\(", app))
        defined = set(re.findall(r"\bfn\s+([a-zA-Z_][a-zA-Z_0-9]*)\s*\(", app))
        self.assertFalse(called - defined, f"Undefined self methods: {sorted(called - defined)}")

    def test_queue_declarations_are_valid_and_present(self):
        code = (ROOT / "src/jobs.rs").read_text(encoding="utf-8")
        self.assertNotIn("let mut reel_finished=false,changed=false,finished_change=false;", code)
        for name in ("reel_finished", "changed", "finished_change"):
            self.assertRegex(code, rf"let\s+mut\s+{name}\s*=\s*false\s*;")

    def test_versions_and_manager_route(self):
        import tomllib
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertEqual(manifest["package"]["version"], "3.10.0")
        app = (ROOT / "src/app.rs").read_text(encoding="utf-8")
        self.assertIn("Page::Manager => self.download_manager(ui)", app)


if __name__ == "__main__":
    unittest.main()
