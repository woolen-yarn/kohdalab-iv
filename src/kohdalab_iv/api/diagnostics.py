from __future__ import annotations

import platform
from collections.abc import Callable
from pathlib import Path
from typing import Any

from kohdalab_iv import __version__
from kohdalab_iv.api.config import load_config, resolve_config_path
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.interfaces.common import list_visa_resources


def collect_diagnostics(
    config_path: str | Path | None = None,
    *,
    resource_lister: Callable[[], tuple[str, ...]] | None = None,
) -> dict[str, Any]:
    resolution = resolve_config_path(config_path)
    config_report: dict[str, Any] = {
        "ok": False,
        "path": str(resolution.path) if resolution.path is not None else None,
        "source": resolution.source,
        "candidates": resolution.candidates,
    }
    if resolution.path is None:
        config_report["error"] = "No configuration file could be resolved."
    else:
        try:
            plan = iv_plan_from_config(load_config(resolution.path))
            config_report.update({"ok": True, "plan": plan.summary})
        except Exception as error:
            config_report["error"] = f"{type(error).__name__}: {error}"

    visa_report: dict[str, Any] = {"ok": False, "resources": []}
    try:
        resources = (resource_lister or list_visa_resources)()
        visa_report.update({"ok": True, "resources": list(resources)})
    except Exception as error:
        visa_report["error"] = f"{type(error).__name__}: {error}"

    return {
        "ok": bool(config_report["ok"] and visa_report["ok"]),
        "version": __version__,
        "python": platform.python_version(),
        "platform": platform.platform(),
        "config": config_report,
        "visa": visa_report,
    }
