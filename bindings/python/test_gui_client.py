"""Control protocol contracts without hardware, GUI processes or network access."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, PropertyMock
from kohdalab_iv_native import GuiIV, _GuiControlRejected

class GuiClientTests(unittest.TestCase):
    def test_endpoint_rejects_nonlocal_and_unknown_protocol(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'endpoint.json'
            for address,protocol in [('192.168.1.10:8000',1),('127.0.0.1:8000',99)]:
                path.write_text(json.dumps({'address':address,'protocol':protocol,'token':'test'}))
                with self.assertRaises(ValueError):
                    GuiIV._load_endpoint(path)

    def test_start_failure_still_attempts_both_stop_and_off(self):
        client=GuiIV.__new__(GuiIV)
        with patch.object(client,'_request',return_value={'run_id':2}), \
             patch.object(client,'wait',side_effect=TimeoutError('run timeout')), \
             patch.object(client,'stop',side_effect=OSError('pipe lost')) as stop, \
             patch.object(client,'output_off') as off:
            with self.assertRaisesRegex(RuntimeError,'cleanup could not be confirmed'):
                client.run(filename='field_1T')
            stop.assert_called_once();off.assert_called_once()

    def test_protection_stop_is_not_returned_as_success(self):
        client=GuiIV.__new__(GuiIV)
        state={'run_id':2,'phase':'Compliance stop','message':'Meter limit reached'}
        with patch.object(client,'_request',return_value={'run_id':2}), \
             patch.object(client,'wait',return_value=state), \
             patch.object(client,'stop') as stop,patch.object(client,'output_off') as off:
            with self.assertRaisesRegex(RuntimeError,'Compliance stop'):
                client.run()
            stop.assert_called_once();off.assert_called_once()

    def test_cancel_before_engine_run_is_reported_without_waiting_for_a_new_run_id(self):
        client=GuiIV.__new__(GuiIV)
        state={'run_id':1,'phase':'Stopped','safety_revision':5}
        def wait(predicate,timeout):
            self.assertTrue(predicate(state))
            return state
        with patch.object(client,'_request',return_value={'run_id':2,'safety_revision':4}), \
             patch.object(client,'wait',side_effect=wait), \
             patch.object(client,'stop'),patch.object(client,'output_off'):
            with self.assertRaisesRegex(RuntimeError,'cancelled before it started'):
                client.run()

    def test_lost_start_acknowledgement_still_requests_stop_and_off(self):
        client=GuiIV.__new__(GuiIV)
        with patch.object(client,'_request',side_effect=TimeoutError('lost reply')), \
             patch.object(client,'stop') as stop,patch.object(client,'output_off') as off:
            with self.assertRaises(TimeoutError):client.run()
            stop.assert_called_once();off.assert_called_once()

    def test_busy_rejection_does_not_stop_a_manual_gui_measurement(self):
        client=GuiIV.__new__(GuiIV)
        with patch.object(client,'_request',side_effect=_GuiControlRejected('GUI is busy')), \
             patch.object(client,'stop') as stop,patch.object(client,'output_off') as off:
            with self.assertRaises(_GuiControlRejected):client.run()
            stop.assert_not_called();off.assert_not_called()

    def test_gate_wait_timeout_requests_both_safety_operations(self):
        client=GuiIV.__new__(GuiIV)
        with patch.object(GuiIV,'snapshot',new_callable=PropertyMock,return_value={'gate_revision':1}), \
             patch.object(client,'_request'),patch.object(client,'wait',side_effect=TimeoutError('gate timeout')), \
             patch.object(client,'stop') as stop,patch.object(client,'output_off') as off:
            with self.assertRaises(TimeoutError):client.set_gate_voltage(.01,compliance=.001)
            stop.assert_called_once();off.assert_called_once()

if __name__=='__main__':unittest.main()
