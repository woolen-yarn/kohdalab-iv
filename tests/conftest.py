"""Portable execution of Python-based native-helper simulators on Windows."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest


@pytest.fixture(autouse=True)
def python_helper_scripts_on_windows(monkeypatch):
    if os.name != "nt":
        return
    original_popen = subprocess.Popen

    def popen(args, *positional, **kwargs):
        if isinstance(args, (list, tuple)) and args:
            path = Path(args[0])
            if path.is_file():
                with path.open("rb") as stream:
                    python_script = stream.read(2) == b"#!"
                if python_script:
                    args = [sys.executable, *args]
        return original_popen(args, *positional, **kwargs)

    monkeypatch.setattr(subprocess, "Popen", popen)
