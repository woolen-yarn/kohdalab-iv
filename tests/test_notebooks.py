from __future__ import annotations

import json
from pathlib import Path


NOTEBOOK = Path("notebook/iv_notebook.ipynb")


def test_notebook_is_clean_and_compilable() -> None:
    notebook = json.loads(NOTEBOOK.read_text(encoding="utf-8"))

    assert notebook["nbformat"] == 4
    assert notebook["metadata"]["kernelspec"]["name"] == "python3"
    assert {cell.get("id") for cell in notebook["cells"]} == {
        "introduction",
        "setup",
        "resources",
        "measurement",
    }
    for cell in notebook["cells"]:
        if cell["cell_type"] != "code":
            continue
        source = "".join(cell["source"])
        compile(source, f"{NOTEBOOK}:{cell['id']}", "exec")
        assert cell["execution_count"] is None
        assert cell["outputs"] == []
        assert "sys.path" not in source
        assert "../config/default.json" not in source


def test_notebook_setup_uses_packaged_config_without_hardware() -> None:
    notebook = json.loads(NOTEBOOK.read_text(encoding="utf-8"))
    setup = next(cell for cell in notebook["cells"] if cell.get("id") == "setup")
    namespace: dict[str, object] = {}

    exec(compile("".join(setup["source"]), f"{NOTEBOOK}:setup", "exec"), namespace)

    experiment = namespace["experiment"]
    assert namespace["config_path"] is not None
    assert namespace["config"]["profile"]["name"] == "default"
    assert not any(experiment.connected_devices().values())
