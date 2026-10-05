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

    # Retain Popen's class/type interface for libraries importing Popen[bytes].
    class PythonScriptPopen(original_popen):
        def __init__(self, args, *positional, **kwargs):
            if isinstance(args, (list, tuple)) and args:
                path = Path(args[0])
                if path.is_file():
                    with path.open("rb") as stream:
                        python_script = stream.read(2) == b"#!"
                    if python_script:
                        args = [sys.executable, *args]
            super().__init__(args, *positional, **kwargs)

    monkeypatch.setattr(subprocess, "Popen", PythonScriptPopen)
