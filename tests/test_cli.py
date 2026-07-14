from __future__ import annotations

from pathlib import Path

import pytest

from kohdalab_iv import __version__
from kohdalab_iv.api import DEFAULT_CONFIG_PATH, cli


def test_check_config_uses_resolved_environment_path(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    config_path = tmp_path / "config.json"
    config_path.write_text("{}", encoding="utf-8")
    monkeypatch.setenv("KOHDALAB_IV_CONFIG", str(config_path))

    assert cli.main(["check-config"]) == 0
    output = capsys.readouterr()
    assert f"config: {config_path}" in output.out
    assert "source.gs210" in output.out
    assert output.err == ""


def test_explicit_config_takes_priority_over_environment(
    tmp_path: Path, monkeypatch
) -> None:
    explicit = tmp_path / "explicit.json"
    explicit.write_text("{}", encoding="utf-8")
    monkeypatch.setenv("KOHDALAB_IV_CONFIG", str(tmp_path / "missing.json"))

    assert cli.main(["--config", str(explicit), "check-config"]) == 0


def test_list_resources_does_not_require_config(monkeypatch, capsys) -> None:
    monkeypatch.setattr(cli, "list_visa_resources", lambda: ("GPIB0::1::INSTR",))
    monkeypatch.setattr(
        cli,
        "resolve_config_path",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError()),
    )

    assert cli.main(["list-resources"]) == 0
    assert capsys.readouterr().out == "GPIB0::1::INSTR\n"


def test_missing_explicit_config_returns_error(tmp_path: Path, capsys) -> None:
    missing = tmp_path / "missing.json"

    assert cli.main(["--config", str(missing), "check-config"]) == 1
    error = capsys.readouterr().err
    assert "No such file or directory" in error
    assert missing.name in error


def test_init_config_creates_exact_packaged_copy_without_resolution(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    destination = tmp_path / "lab" / "config.json"
    monkeypatch.setattr(
        cli,
        "resolve_config_path",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError()),
    )

    assert cli.main(["init-config", str(destination)]) == 0

    assert destination.read_text(encoding="utf-8") == DEFAULT_CONFIG_PATH.read_text(
        encoding="utf-8"
    )
    output = capsys.readouterr()
    assert output.out == f"Created config: {destination}\n"
    assert output.err == ""


def test_version_option_prints_runtime_version(capsys) -> None:
    with pytest.raises(SystemExit, match="0"):
        cli.main(["--version"])
    assert capsys.readouterr().out == f"kohdalab-iv {__version__}\n"


def test_doctor_supports_text_json_and_failure_status(monkeypatch, capsys) -> None:
    report = {
        "ok": True,
        "version": __version__,
        "python": "3.13.0",
        "platform": "test-platform",
        "config": {
            "ok": True,
            "path": "config.json",
            "source": "explicit",
            "plan": "DC I-V, 3 points",
        },
        "visa": {"ok": True, "resources": ["GPIB0::2::INSTR"]},
    }
    monkeypatch.setattr(cli, "collect_diagnostics", lambda _path: report)

    assert cli.main(["doctor"]) == 0
    output = capsys.readouterr().out
    assert "Config: OK - config.json (explicit)" in output
    assert "VISA: OK - 1 resource(s)" in output
    assert "Overall: OK" in output

    assert cli.main(["doctor", "--json"]) == 0
    assert '"ok": true' in capsys.readouterr().out

    report["ok"] = False
    report["config"] = {"ok": False, "error": "bad config"}
    report["visa"] = {"ok": False, "resources": [], "error": "missing backend"}
    assert cli.main(["doctor"]) == 1
    output = capsys.readouterr().out
    assert "Config: ERROR - bad config" in output
    assert "VISA: ERROR - missing backend" in output
    assert "Overall: FAILED" in output


def test_init_config_requires_force_to_replace_existing_file(
    tmp_path: Path, capsys
) -> None:
    destination = tmp_path / "config.json"
    destination.write_text("keep me", encoding="utf-8")

    assert cli.main(["init-config", str(destination)]) == 1
    assert destination.read_text(encoding="utf-8") == "keep me"
    assert "Use --force" in capsys.readouterr().err

    assert cli.main(["init-config", str(destination), "--force"]) == 0
    assert destination.read_text(encoding="utf-8") == DEFAULT_CONFIG_PATH.read_text(
        encoding="utf-8"
    )
