"""Tests for check_version.py: python -m unittest discover -s packaging-rs  (needs Python 3.11+)."""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

import check_version


def run(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = check_version.main(["check_version.py", *argv])
    return code, out.getvalue().strip(), err.getvalue()


class CheckVersion(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.rs = Path(self._tmp.name)
        (self.rs / "app").mkdir()
        self._old = check_version.RS
        check_version.RS = self.rs
        self.addCleanup(setattr, check_version, "RS", self._old)

    def write(self, cargo="0.2.0", tauri="0.2.0", workspace=True):
        if workspace:
            (self.rs / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{cargo}"\n', encoding="utf-8")
            app = '[package]\nname = "clawd-app"\nversion.workspace = true\n'
        else:
            app = f'[package]\nname = "clawd-app"\nversion = "{cargo}"\n'
        (self.rs / "app" / "Cargo.toml").write_text(app, encoding="utf-8")
        (self.rs / "app" / "tauri.conf.json").write_text(json.dumps({"version": tauri}), encoding="utf-8")

    def test_agreeing_versions_print(self):
        self.write()
        self.assertEqual(run([]), (0, "0.2.0", ""))

    def test_own_version_in_app_cargo_toml(self):
        self.write(workspace=False)
        self.assertEqual(run(["rust-v0.2.0"])[:2], (0, "0.2.0"))

    def test_prerelease_version_and_tag(self):
        self.write("0.2.0-test.1", "0.2.0-test.1")
        self.assertEqual(run(["rust-v0.2.0-test.1"])[:2], (0, "0.2.0-test.1"))

    def test_tag_must_match(self):
        self.write()
        for tag in ("rust-v0.2.1", "v0.2.0", "rust-0.2.0", "rust-v0.2.0-x", "rust-v"):
            with self.subTest(tag=tag):
                self.assertEqual(run([tag])[0], 1)

    def test_cargo_and_tauri_must_match(self):
        self.write(tauri="0.2.1")
        self.assertEqual(run([])[0], 1)

    def test_version_format_is_checked(self):
        for bad in ("0.2", "v0.2.0", "0.2.0\nx=1", "0.2.0-", "0.2.0 ", "1.2.3+build", "../0.2.0", "0.2.0;rm"):
            with self.subTest(version=bad):
                self.write(bad, bad)
                code, out, err = run([])
                self.assertEqual((code, out), (1, ""))
                # a raw newline is not even valid TOML: refused as unreadable instead
                self.assertTrue("not X.Y.Z" in err or "cannot read" in err, err)

    def test_unreadable_files_fail(self):
        self.assertEqual(run([])[0], 1)

    def test_too_many_arguments(self):
        self.write()
        self.assertEqual(run(["rust-v0.2.0", "x"])[0], 1)


if __name__ == "__main__":
    unittest.main()
