"""Resolution tests only; no Rust process or instrument is started."""
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
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
            ), patch("kohdalab_iv_native.sys.platform", "win32"), patch(
                "kohdalab_iv_native._windows_installations", return_value=iter(())
            ):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_invalid_override_does_not_fall_back(self):
        with patch("kohdalab_iv_native.shutil.which") as which:
            with self.assertRaises(FileNotFoundError):
                resolve_executable("/missing/iv-cli")
            which.assert_not_called()

    def test_macos_user_application(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            app = home / "Applications" / "KohdaLab IV.app"
            executable = app / "Contents" / "MacOS" / "iv-cli"
            executable.parent.mkdir(parents=True)
            executable.touch()
            with patch.dict(os.environ, {}, clear=True), patch(
                "kohdalab_iv_native.shutil.which", return_value=None
            ), patch("kohdalab_iv_native.sys.platform", "darwin"), patch(
                "kohdalab_iv_native.Path.home", return_value=home
            ):
                self.assertEqual(resolve_executable(), str(executable.resolve()))
                self.assertEqual(resolve_executable(app), str(executable.resolve()))

    def test_installation_directory_argument(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "iv-cli.exe"
            executable.touch()
            with patch("kohdalab_iv_native.sys.platform", "win32"):
                self.assertEqual(resolve_executable(directory), str(executable.resolve()))

    def test_windows_native_program_files_from_32_bit_notebook(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "Kohdalab" / "KohdaLab IV" / "iv-cli.exe"
            executable.parent.mkdir(parents=True)
            executable.touch()
            with patch.dict(os.environ, {"ProgramW6432": directory, "ProgramFiles": "missing"}, clear=True), patch(
                "kohdalab_iv_native.shutil.which", return_value=None
            ), patch("kohdalab_iv_native.sys.platform", "win32"), patch(
                "kohdalab_iv_native._windows_installations", return_value=iter(())
            ):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_stale_registry_candidate_is_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "custom installation" / "iv-cli.exe"
            executable.parent.mkdir()
            executable.touch()
            with patch.dict(os.environ, {}, clear=True), patch(
                "kohdalab_iv_native.shutil.which", return_value=None
            ), patch("kohdalab_iv_native.sys.platform", "win32"), patch(
                "kohdalab_iv_native._windows_installations",
                return_value=iter((Path(directory) / "stale.exe", executable))
            ):
                self.assertEqual(resolve_executable(), str(executable.resolve()))

    def test_windows_registry_reads_both_views_and_skips_missing_key(self):
        handle = unittest.mock.MagicMock()
        registry = SimpleNamespace(
            HKEY_CURRENT_USER=1, HKEY_LOCAL_MACHINE=2,
            KEY_WOW64_64KEY=256, KEY_WOW64_32KEY=512, KEY_READ=1,
            REG_SZ=1, REG_EXPAND_SZ=2,
            OpenKey=unittest.mock.Mock(side_effect=[OSError(), handle, OSError(), OSError()]),
            QueryValueEx=unittest.mock.Mock(return_value=("custom folder", 1)),
        )
        from kohdalab_iv_native import _windows_installations
        with patch.dict("sys.modules", {"winreg": registry}):
            self.assertEqual(list(_windows_installations()), [Path("custom folder") / "iv-cli.exe"])
        self.assertEqual(registry.OpenKey.call_count, 4)
        self.assertEqual(registry.OpenKey.call_args_list[1].args[-1], 513)

    def test_error_explains_that_client_does_not_install_engine(self):
        with patch.dict(os.environ, {}, clear=True), patch(
            "kohdalab_iv_native.shutil.which", return_value=None
        ), patch("kohdalab_iv_native._installed_candidates", return_value=iter(())):
            with self.assertRaisesRegex(FileNotFoundError, "does not install or build"):
                resolve_executable()


if __name__ == "__main__":
    unittest.main()
