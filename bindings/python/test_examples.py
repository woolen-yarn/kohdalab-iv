"""Run shipped Notebook code cells top-to-bottom using the simulation engine."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


class NotebookExamplesTests(unittest.TestCase):
    def test_simulation_notebooks(self):
        examples = Path(__file__).parent / "examples"
        for name in ("native-control.ipynb", "migrate-to-native.ipynb"):
            with self.subTest(notebook=name), tempfile.TemporaryDirectory() as directory:
                notebook = json.loads((examples / name).read_text(encoding="utf-8"))
                namespace = {"__name__": "__main__"}
                # Keep tutorial CSVs inspectable during execution, then clean up.
                with patch("tempfile.mkdtemp", return_value=directory):
                    for index, cell in enumerate(notebook["cells"]):
                        if cell["cell_type"] == "code":
                            source = "".join(cell["source"])
                            exec(compile(source, f"{name}:cell-{index}", "exec"), namespace)
                self.assertTrue(namespace["output"].is_file())


if __name__ == "__main__":
    unittest.main()
