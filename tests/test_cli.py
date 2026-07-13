from __future__ import annotations

from pathlib import Path

from kohdalab_iv.api import cli


def test_check_config_uses_resolved_environment_path(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    config_path = tmp_path / "config.json"
    config_path.write_text("{}", encoding="utf-8")
    monkeypatch.setenv("KOHDALAB_IV_CONFIG", str(config_path))

    assert cli.main(["check-config"]) == 0
    output = capsys.readouterr()
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
