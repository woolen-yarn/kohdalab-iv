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
                self.assertEqual(len(rows[0]), 26)
                self.assertEqual(rows[0]["V_meter_V"], "")
                self.assertEqual(rows[0]["I_source_A"], "")
                self.assertNotEqual(rows[0]["V_source_V"], "")
                saved = path.read_bytes()
                iv.live()
                self.assertEqual(path.read_bytes(), saved)
            self.assertEqual(iv.snapshot['output'], 'Off')
            self.assertFalse(iv.snapshot['connected'])
            metadata = json.loads(path.with_suffix('.run.json').read_text())
            self.assertFalse(metadata['in_progress'])

    def test_run_returns_after_software_compliance_stop(self):
        """A verified safety stop is terminal, rather than a client timeout."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'compliance-stop.csv'
            with NativeIV() as iv:
                iv.connect(simulation=True)
                state = iv.run(path, {
                    'pattern': 'linear',
                    'start': -0.001,
                    'stop': -0.001,
                    'step': 0.001,
                    # The simulation is a 1 kOhm reference, so -1 mV
                    # produces -1 uA and must trip this 0.5 uA limit.
                    'compliance': 0.5e-6,
                    'settle_s': 0.0,
                    'start_settle_s': 0.0,
                    'ramp_wait_s': 0.0,
                }, timeout=5)
                self.assertEqual(state['phase'], 'Compliance stop')
                self.assertEqual(state['output'], 'Off')
                self.assertEqual(state['points'], 1)

            with path.open(newline='') as stream:
                rows = list(csv.DictReader(stream))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]['software_limit_hit'], 'true')
            metadata = json.loads(path.with_suffix('.run.json').read_text())
            self.assertFalse(metadata['in_progress'])

    def test_simulated_gate_bias_stays_on_for_a_balanced_iv_sweep(self):
        """The gate and both sweep sources share one Rust engine session."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'balanced.csv'
            with NativeIV() as iv:
                iv.connect(simulation=True)
                iv.connect_source2(simulation=True)
                iv.connect_gate(simulation=True)
                gate = iv.set_gate_voltage(0.1, compliance=0.001, ramp_step=0.001)
                self.assertEqual(gate['gate_output'], 'On')
                state = iv.run(path, {
                    'pattern': 'linear',
                    'start': -0.001,
                    'stop': 0.001,
                    'step': 0.001,
                    'source_compliance': 0.001,
                    'balanced_source': {
                        'source_model': 'YOKOGAWA_GS210',
                        'source_compliance': 0.001,
                    },
                    'settle_s': 0.0,
                    'start_settle_s': 0.0,
                    'ramp_wait_s': 0.0,
                })
                self.assertEqual(state['phase'], 'Completed')
                self.assertEqual(state['output'], 'Off')
                self.assertEqual(state['gate_output'], 'On')
                metadata = json.loads(path.with_suffix('.run.json').read_text())
                self.assertEqual(metadata['balanced_drive']['ratio'], [1, 1])
                self.assertEqual(len(metadata['dual_source_points']), state['points'])
                first = metadata['dual_source_points'][0]
                self.assertEqual(first['source1']['setpoint_V'], -0.0005)
                self.assertEqual(first['source2']['setpoint_V'], 0.0005)
                for role in ('source1', 'source2'):
                    self.assertIsNone(first[role]['level_readback_V'])
                    self.assertIsNone(first[role]['output_voltage_V'])
                off = iv.gate_output_off()
                self.assertEqual(off['gate_output'], 'Off')


if __name__ == '__main__':
    unittest.main()
