from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

from kohdalab_iv.api import diagnostics
from kohdalab_iv.api.config import ConfigPathResolution


def test_collect_diagnostics_reports_valid_config_and_resources(
    monkeypatch, tmp_path: Path
) -> None:
    path = tmp_path / "config.json"
    resolution = ConfigPathResolution(path, "explicit", [{"path": str(path)}])
    monkeypatch.setattr(diagnostics, "resolve_config_path", lambda _path: resolution)
    monkeypatch.setattr(diagnostics, "load_config", lambda _path: {"valid": True})
    monkeypatch.setattr(
        diagnostics,
        "iv_plan_from_config",
        lambda _config: SimpleNamespace(summary="DC I-V, 3 points"),
    )

    report = diagnostics.collect_diagnostics(
        path, resource_lister=lambda: ("GPIB0::2::INSTR",)
    )

    assert report["ok"]
    assert report["config"] == {
        "ok": True,
        "path": str(path),
        "source": "explicit",
        "candidates": [{"path": str(path)}],
        "plan": "DC I-V, 3 points",
    }
    assert report["visa"] == {
        "ok": True,
        "resources": ["GPIB0::2::INSTR"],
    }
    assert report["version"]
    assert report["python"]
    assert report["platform"]


def test_collect_diagnostics_keeps_independent_failure_details(monkeypatch) -> None:
    resolution = ConfigPathResolution(None, "none", [])
    monkeypatch.setattr(diagnostics, "resolve_config_path", lambda _path: resolution)

    def fail_resources() -> tuple[str, ...]:
        raise RuntimeError("VISA backend missing")

    report = diagnostics.collect_diagnostics(resource_lister=fail_resources)

    assert not report["ok"]
    assert report["config"]["error"] == "No configuration file could be resolved."
    assert report["visa"]["error"] == "RuntimeError: VISA backend missing"


def test_collect_diagnostics_reports_invalid_resolved_config(monkeypatch) -> None:
    resolution = ConfigPathResolution(Path("invalid.json"), "explicit", [])
    monkeypatch.setattr(diagnostics, "resolve_config_path", lambda _path: resolution)
    monkeypatch.setattr(
        diagnostics,
        "load_config",
        lambda _path: (_ for _ in ()).throw(ValueError("bad config")),
    )

    report = diagnostics.collect_diagnostics(resource_lister=lambda: ())

    assert not report["ok"]
    assert report["config"]["error"] == "ValueError: bad config"
    assert report["visa"] == {"ok": True, "resources": []}
