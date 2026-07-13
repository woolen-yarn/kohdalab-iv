from __future__ import annotations

import csv
import io
import runpy
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = runpy.run_path(str(ROOT / "scripts" / "verify_distributions.py"))


def test_rejects_cache_and_platform_artifacts() -> None:
    is_forbidden = VERIFIER["_is_forbidden"]
    assert is_forbidden("kohdalab_iv/.DS_Store")
    assert is_forbidden("kohdalab_iv/__pycache__/module.pyc")
    assert not is_forbidden("kohdalab_iv/api/models.py")


def test_requires_one_artifact_of_each_kind(tmp_path: Path) -> None:
    single = VERIFIER["_single"]
    with pytest.raises(RuntimeError, match="exactly one"):
        single(tmp_path, "*.whl")
    (tmp_path / "one.whl").touch()
    assert single(tmp_path, "*.whl").name == "one.whl"


def test_record_paths_must_match_archive_members() -> None:
    verify_record = VERIFIER["_verify_record"]
    payloads = {
        "kohdalab_iv/__init__.py": b"",
        "demo.dist-info/RECORD": b"kohdalab_iv/__init__.py,,,\n",
    }
    with pytest.raises(RuntimeError, match="RECORD paths"):
        verify_record(payloads)

    output = io.StringIO()
    csv.writer(output).writerows(
        [["kohdalab_iv/__init__.py", "", ""], ["demo.dist-info/RECORD", "", ""]]
    )
    payloads["demo.dist-info/RECORD"] = output.getvalue().encode()
    verify_record(payloads)
