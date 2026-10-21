"""Meaningful packaging regressions: firmware in nested source/binaries."""
import io
from pathlib import Path
import sys
import tempfile
import unittest
from zipfile import ZipFile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from distribution_policy import LICENSES, SOURCE_FILES, check_member, inspect_zip, validate_archive


def archive(files):
    result = io.BytesIO()
    with ZipFile(result, "w") as z:
        for name, data in files.items():
            z.writestr(name, data)
    return result.getvalue()


class DistributionPolicyTests(unittest.TestCase):
    def test_firmware_hidden_in_corresponding_source_is_rejected(self):
        source = archive({"native/vendor/renamed.hex": b":00000001FF"})
        with self.assertRaisesRegex(ValueError, "firmware"):
            inspect_zip(archive({"portable/support/SOURCE.zip": source}))

    def test_old_downloader_binary_ascii_and_wide_is_rejected(self):
        for encoding in ("utf-8", "utf-16le"):
            with self.subTest(encoding=encoding), self.assertRaisesRegex(ValueError, "Obsolete"):
                check_member("support/USB-Setup.exe", "raw.githubusercontent.com".encode(encoding))

    def test_missing_license_or_corresponding_source_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "portable.zip"
            file.write_bytes(archive({"portable/REDISTRIBUTION.md": b"terms"}))
            with self.assertRaisesRegex(ValueError, "Missing .*redistribution material"):
                validate_archive(file)

    def test_inbox_driver_program_and_license_documents_are_allowed(self):
        check_member("support/USB-Setup.exe", b"WinUSB inbox-only")
        check_member("docs/redistribution_audit.md", b"Excluded measat_releaseX1.8.hex; no grant established")
        inspect_zip(archive({"support/licenses/GPL-2.0.txt": b"license"}))

    def test_complete_source_in_nested_zip_is_accepted_and_removing_gpl_text_fails(self):
        # Corresponding source lives at the root of SOURCE.zip, rather than
        # under the outer portable directory. Both locations must be checked.
        source_files = {name: b"source" for name in SOURCE_FILES}
        source_files.update({f"native/licenses/{name}": b"license" for name in LICENSES})
        source_files["packaging/licenses/THIRD_PARTY_NOTICES.md"] = b"source notices"
        source_files["REDISTRIBUTION.md"] = b"source terms"
        source_files["native/vendor/libusb-1.0.30.tar.bz2"] = (
            Path(__file__).resolve().parents[1] / "native/vendor/libusb-1.0.30.tar.bz2"
        ).read_bytes()
        source = archive(source_files)
        files = {
            "portable/REDISTRIBUTION.md": b"terms",
            "portable/support/THIRD_PARTY_NOTICES.md": b"notices",
            "portable/support/SOURCE.zip": source,
            **{f"portable/support/licenses/{name}": b"license" for name in LICENSES},
        }
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "portable.zip"
            file.write_bytes(archive(files))
            validate_archive(file)
            del files["portable/support/licenses/GPL-2.0.txt"]
            file.write_bytes(archive(files))
            with self.assertRaisesRegex(ValueError, "GPL-2.0.txt"):
                validate_archive(file)
            files["portable/support/licenses/GPL-2.0.txt"] = b"license"
            del files["portable/support/THIRD_PARTY_NOTICES.md"]
            file.write_bytes(archive(files))
            with self.assertRaisesRegex(ValueError, "THIRD_PARTY_NOTICES.md"):
                validate_archive(file)


if __name__ == "__main__":
    unittest.main()
