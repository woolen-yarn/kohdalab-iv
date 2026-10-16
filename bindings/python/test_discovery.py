"""Resolution tests only; no Rust process or instrument is started."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from kohdalab_iv_native import resolve_executable


class ExecutableDiscoveryTests(unittest.TestCase):
    def test_explicit_path_wins_over_environment_and_path(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "iv-cli"
            executable.touch()
            with patch.dict(os.environ, {"KOHDALAB_IV_EXE": "missing"}), patch(
                "kohdalab_iv_native.shutil.which", return_value="also-missing"
            ):
                self.assertEqual(resolve_executable(executable), str(executable.resolve()))

    def test_environment_wins_over_path(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "from-env"
            executable.touch()
            with patch.dict(os.environ, {"KOHDALAB_IV_EXE": str(executable)}), patch(
                "kohdalab_iv_native.shutil.which", return_value=None
            ):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_path_is_used_before_windows_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "from-path"
            executable.touch()
            with patch.dict(os.environ, {}, clear=True), patch(
                "kohdalab_iv_native.shutil.which", side_effect=[str(executable), None]
            ), patch("kohdalab_iv_native.sys.platform", "win32"):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_windows_installation_is_the_last_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "Kohdalab" / "KohdaLab IV" / "iv-cli.exe"
            executable.parent.mkdir(parents=True)
            executable.touch()
            with patch.dict(os.environ, {"ProgramFiles": str(root)}, clear=True), patch(
                "kohdalab_iv_native.shutil.which", return_value=None
            ), patch("kohdalab_iv_native.sys.platform", "win32"):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_error_explains_that_client_does_not_install_engine(self):
        with patch.dict(os.environ, {}, clear=True), patch(
            "kohdalab_iv_native.shutil.which", return_value=None
        ), patch("kohdalab_iv_native.sys.platform", "darwin"):
            with self.assertRaisesRegex(FileNotFoundError, "does not install or build"):
                resolve_executable()


if __name__ == "__main__":
    unittest.main()
