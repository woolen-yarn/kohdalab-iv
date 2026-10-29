"""Validate lease expiry, Stop, GUI plots, gate sequencing and CSV without hardware.

Requires a visible desktop and a development build with --gui-control-smoke.
Private heartbeat fields are intentionally used only to inject client loss.
"""
import argparse
import csv
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'bindings/python'))
from kohdalab_iv_native import GuiIV

def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description='Validate the GUI control API with simulation instruments only.')
    parser.add_argument('binary', type=Path)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    report = args.output_dir.resolve()
    report.mkdir(parents=True, exist_ok=True)
    profile = Path(tempfile.mkdtemp(prefix='iv-gui-control-'))
    env = dict(os.environ, USERPROFILE=str(profile))
    log = (report / 'gui-control.log').open('w')
    process = subprocess.Popen([str(args.binary.resolve()), '--gui-control-smoke', '--screenshot', str(report / 'dual-gui.png')], env=env, stdout=log, stderr=log)
    (report / 'process.json').write_text(json.dumps({'pid': process.pid, 'profile': str(profile), 'command': str(args.binary.resolve()) + ' --gui-control-smoke'}))
    try:
        deadline = time.monotonic() + 20
        while True:
            endpoints = list((profile / '.kohdalab/control').glob('iv-*.json'))
            if endpoints:
                break
            if process.poll() is not None:
                raise RuntimeError((report / 'gui-control.log').read_text())
            if time.monotonic() > deadline:
                raise TimeoutError('GUI endpoint missing')
            time.sleep(0.1)
        abandoned = GuiIV(endpoint=endpoints[0])
        abandoned.wait(lambda s: s['connected'] and s['source2_connected'])
        assert all((abandoned.snapshot[field] for field in ('source_simulation', 'source2_simulation', 'meter_simulation')))
        abandoned.connect_gate(simulation=True)
        abandoned.set_gate_voltage(0.03, compliance=0.001, ramp_step=0.01)
        abandoned._stop_heartbeat.set()
        abandoned._heartbeat.join(timeout=4)
        deadline = time.monotonic() + 15
        while True:
            state = abandoned.snapshot
            if state['gate_output'] == 'Off' and state['output'] == 'Off' and (state['source2_output'] == 'Off'):
                break
            if time.monotonic() > deadline:
                raise TimeoutError('Expired Python lease did not turn outputs off')
            time.sleep(0.2)
        abandoned._closed = True
        with GuiIV(endpoint=endpoints[0]) as iv:
            iv.wait(lambda s: s['connected'] and s['source2_connected'])
            assert iv.snapshot['simulation'] is True
            try:
                other = GuiIV(endpoint=endpoints[0])
                other.acquire()
            except RuntimeError as e:
                assert 'another Python client' in str(e)
            else:
                raise AssertionError('second lease accepted')
            assert iv.snapshot['gate_connected']
            errors = []

            def interrupted_run():
                try:
                    iv.run(filename='stop_test', directory=profile / 'results', timeout=15)
                except RuntimeError as error:
                    errors.append(str(error))
            before = iv.snapshot.get('run_id', 0)
            iv.set_gate_voltage(0.01, compliance=0.001, ramp_step=0.01)
            pending = threading.Thread(target=interrupted_run)
            pending.start()
            iv.wait(lambda s: s['run_id'] > before and s['phase'] == 'Running', timeout=10)
            try:
                iv.set_gate_voltage(0.04, compliance=0.001, ramp_step=0.01)
            except RuntimeError as error:
                assert getattr(error, 'code', None) == 'busy'
            else:
                raise AssertionError('gate change admitted during IV')
            # Gate OFF remains an immediate safety operation even while the
            # normal API/measurement queue is busy.
            iv.gate_output_off(timeout=10)
            iv.stop()
            pending.join(timeout=15)
            assert not pending.is_alive() and errors, errors
            iv.wait(lambda s: s['output'] == 'Off' and s['source2_output'] == 'Off' and (s['gate_output'] == 'Off'))
            # Select the simulation's 100 mV range before the two live bias
            # values. An enabled source now keeps that verified range fixed.
            iv.set_gate_voltage(0.03, compliance=0.001, ramp_step=0.01)
            assert iv.snapshot['gate_dc_limits']['range'] == 0.1
            states = []
            for voltage in [0.01, 0.02]:
                gate = iv.set_gate_voltage(voltage, compliance=0.001, ramp_step=0.01)
                assert gate['gate_voltage'] == voltage
                assert gate['gate_dc_limits']['range'] == 0.1
                state = iv.run(filename=f'gate_{voltage:.2e}V', directory=profile / 'results', timeout=20)
                assert state['gate_output'] == 'On' and state['output'] == 'Off' and (state['source2_output'] == 'Off'), state
                rows = list(csv.DictReader(Path(state['path']).open()))
                assert len(rows) == state['points'] > 0
                states.append({'run_id': state['run_id'], 'path': state['path'], 'points': state['points'], 'gate_V': voltage})
            (report / 'gui-control-results.json').write_text(json.dumps({'simulation': True, 'runs': states}, indent=2))
        assert iv._closed
        code = process.wait(timeout=20)
        assert code == 0, (report / 'gui-control.log').read_text()
        assert (report / 'dual-gui.png').is_file()
        print(json.dumps({'ok': True, 'gui_process': process.pid, 'report': str(report), 'runs': states}))
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        log.close()
if __name__ == '__main__':
    main()
