"""Simulation acceptance test; requires KOHDALAB_IV_EXE, no hardware access."""
import csv
import json
from pathlib import Path
import tempfile
import unittest
from kohdalab_iv_native import NativeIV


class ClientTests(unittest.TestCase):
    def test_monitor_sweep_and_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'sweep.csv'
            with NativeIV() as iv:
                iv.connect(simulation=True)
                switched = iv.set_mode("dc_vi")
                self.assertEqual(switched["live"]["mode"], "dc_vi")
                self.assertEqual(switched["points"], 0)
                iv.set_mode("dc_iv")
                first = iv.live()
                second = iv.live()
                self.assertNotEqual(first['timestamp'], second['timestamp'])
                self.assertEqual(iv.snapshot['output'], 'Off')
                self.assertFalse(path.exists())
                state = iv.run(path, {'pattern': 'linear', 'start': -0.001,
                    'stop': 0.001, 'step': 0.001, 'settle_s': 0.0,
                    'start_settle_s': 0.0, 'ramp_wait_s': 0.0})
                self.assertEqual(state['phase'], 'Completed')
                self.assertEqual(state['output'], 'Off')
                with path.open(newline='') as stream:
                    rows = list(csv.DictReader(stream))
                self.assertEqual(len(rows), 3)
                self.assertEqual(len(rows[0]), 14)
                saved = path.read_bytes()
                iv.live()
                self.assertEqual(path.read_bytes(), saved)
            self.assertEqual(iv.snapshot['output'], 'Off')
            self.assertFalse(iv.snapshot['connected'])
            metadata = json.loads(path.with_suffix('.run.json').read_text())
            self.assertFalse(metadata['in_progress'])


if __name__ == '__main__':
    unittest.main()
