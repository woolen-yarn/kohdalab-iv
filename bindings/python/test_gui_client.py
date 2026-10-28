"""Control protocol contracts without hardware, GUI processes or network access."""
import json
from pathlib import Path
import tempfile
import unittest
import weakref
from unittest.mock import patch, PropertyMock
from kohdalab_iv_native import GuiIV, GuiIVError, _GuiControlRejected

class GuiClientTests(unittest.TestCase):
    def test_endpoint_rejects_nonlocal_and_unknown_protocol(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'endpoint.json'
            for address,protocol in [('192.168.1.10:8000',1),('127.0.0.1:8000',99)]:
                path.write_text(json.dumps({'address':address,'protocol':protocol,'token':'test'}))
                with self.assertRaises(ValueError):
                    GuiIV._load_endpoint(path)

    def test_protocol_two_endpoint_is_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'endpoint.json'
            path.write_text(json.dumps({'address': '127.0.0.1:8000', 'protocol': 2,
                                        'token': 'test', 'pid': 100, 'instance_id': 'one'}))
            self.assertEqual(GuiIV._load_endpoint(path)['protocol'], 2)

    def test_malformed_or_stale_descriptors_are_rejected_as_value_errors(self):
        bad = (
            b'[]', b'{"protocol":2}',
            json.dumps({'protocol': 2, 'address': '127.0.0.1:8000', 'token': 'x'}).encode(),
            json.dumps({'protocol': 2, 'address': '127.0.0.1:0', 'token': 'x',
                        'pid': 1, 'instance_id': 'x'}).encode(),
            json.dumps({'protocol': 2, 'address': '127.0.0.1:8000', 'token': '',
                        'pid': 1, 'instance_id': 'x'}).encode(),
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'endpoint.json'
            for content in bad:
                path.write_bytes(content)
                with self.assertRaises(ValueError):
                    GuiIV._load_endpoint(path)

    def test_invalid_success_envelope_is_a_typed_protocol_error(self):
        class Stream:
            def readline(self, size): return b'[]\n'
            def __enter__(self): return self
            def __exit__(self, *args): pass
        class Connection:
            def settimeout(self, timeout): pass
            def sendall(self, payload): pass
            def makefile(self, mode): return Stream()
            def __enter__(self): return self
            def __exit__(self, *args): pass
        client = GuiIV.__new__(GuiIV)
        client._closed = False
        client._control_lost = None
        client._lease = None
        client.client_id = 'client'
        client._endpoint = {'address': '127.0.0.1:8000', 'token': 'token'}
        client._socket = type('Socket', (), {'create_connection': staticmethod(lambda *args, **kwargs: Connection())})
        with self.assertRaises(GuiIVError) as caught:
            client._request('status')
        self.assertEqual(caught.exception.code, 'protocol')

    def test_start_uses_caller_request_id_and_wait_uses_the_matching_job(self):
        client = GuiIV.__new__(GuiIV)
        client._heartbeat_error = None
        request_id = '9a0440b9-48cb-45dd-a58d-c6f28bf4b9f2'
        with patch.object(client, '_request', return_value={'id': 'job-7', 'status': 'running'}) as request:
            receipt = client.start(filename='field_1T', request_id=request_id)
        self.assertEqual(receipt['id'], 'job-7')
        self.assertEqual(request.call_args.kwargs['request_id'], request_id)
        with patch.object(client, 'result', side_effect=(
                {'id': 'job-7', 'status': 'running'},
                {'id': 'job-7', 'status': 'completed'})) as result, patch('time.sleep'):
            self.assertEqual(client.wait('job-7', timeout=1)['id'], 'job-7')
        self.assertEqual(result.call_args_list[0].args, ('job-7',))
        self.assertEqual(result.call_args_list[1].args, ('job-7',))

    def test_read_only_result_does_not_acquire_and_stopped_job_has_a_code(self):
        client = GuiIV.__new__(GuiIV)
        client._heartbeat_error = None
        with patch.object(client, '_request', return_value={'id': 'job-7', 'status': 'running'}) as request:
            client.result('job-7')
        self.assertNotIn('require_lease', request.call_args.kwargs)
        with patch.object(client, 'result', return_value={'id': 'job-7', 'status': 'stopped'}):
            with self.assertRaises(GuiIVError) as caught:
                client.wait('job-7', timeout=1)
        self.assertEqual(caught.exception.code, 'stopped')

    def test_set_output_omits_unset_keys_and_configuration_keeps_legacy_plan_value(self):
        client = GuiIV.__new__(GuiIV)
        with patch.object(client, '_request', return_value={}) as request:
            client.set_output(filename='field_1T')
        self.assertEqual(request.call_args.args, ('set_output',))
        self.assertEqual(request.call_args.kwargs['filename'], 'field_1T')
        self.assertNotIn('directory', request.call_args.kwargs)
        self.assertNotIn('auto_timestamp_suffix', request.call_args.kwargs)
        with patch.object(client, 'plan', return_value={'mode': 'dc_iv'}):
            self.assertEqual(client.configuration, {'mode': 'dc_iv'})

    def test_run_returns_legacy_snapshot_from_a_completed_job_receipt(self):
        client = GuiIV.__new__(GuiIV)
        snapshot = {'phase': 'Completed', 'run_id': 8}
        with patch.object(client, 'start', return_value={'id': 'job-8', 'status': 'running'}), \
             patch.object(client, 'wait', return_value={'id': 'job-8', 'status': 'completed', 'snapshot': snapshot}):
            self.assertEqual(client.run(), snapshot)

    def test_protocol_two_gate_job_waits_for_the_exact_job_id(self):
        client = GuiIV.__new__(GuiIV)
        client._endpoint = {'protocol': 2}
        snapshot = {'gate_connected': True}
        with patch.object(client, '_request', return_value={'id': 'gate-1', 'status': 'running'}), \
             patch.object(client, 'wait', return_value={'id': 'gate-1', 'status': 'completed', 'snapshot': snapshot}) as wait, \
             patch.object(GuiIV, 'snapshot', new_callable=PropertyMock, return_value={'gate_connected': False}):
            self.assertEqual(client.connect_gate(simulation=True), snapshot)
        self.assertEqual(wait.call_args.args[0], 'gate-1')

    def test_lost_control_is_never_silently_reacquired(self):
        client = GuiIV.__new__(GuiIV)
        client._control_lost = GuiIVError('taken over', 'lease_revoked')
        client._lease = None
        with self.assertRaises(GuiIVError) as caught:
            client._ensure_lease()
        self.assertEqual(caught.exception.code, 'lease_revoked')

    def test_request_ids_are_validated_and_connect_reuses_the_supplied_id(self):
        client = GuiIV.__new__(GuiIV)
        request_id = '9a0440b9-48cb-45dd-a58d-c6f28bf4b9f2'
        with patch.object(client, '_request', return_value={}) as request:
            client.connect(['source'], request_id=request_id)
        self.assertEqual(request.call_args.kwargs['request_id'], request_id)
        with self.assertRaises(GuiIVError) as caught:
            client.disconnect(request_id='not-a-uuid')
        self.assertEqual(caught.exception.code, 'invalid_request_id')

    def test_transport_heartbeat_loss_latches_ownership_loss(self):
        class OneBeat:
            def __init__(self): self.calls = 0
            def wait(self, seconds):
                self.calls += 1
                return self.calls > 1
        client = GuiIV.__new__(GuiIV)
        client._heartbeat_error = None
        client._control_lost = None
        with patch.object(client, '_request', side_effect=OSError('lost')):
            GuiIV._beat_gui_weak(weakref.ref(client), OneBeat())
        self.assertEqual(client._heartbeat_error.code, 'ownership_lost')
        self.assertEqual(client._control_lost.code, 'ownership_lost')

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

class WaitCompatibilityTests(unittest.TestCase):
    def test_protocol_one_keeps_legacy_configuration_read(self):
        client = GuiIV.__new__(GuiIV)
        client._endpoint = {"protocol": 1}
        with patch.object(client, "_request", return_value={"mode":"dc_iv"}) as request:
            self.assertEqual(client.configuration["mode"], "dc_iv")
            request.assert_called_once_with("configuration")

    def test_wait_supports_unlimited_timeout_and_rejects_invalid_polling(self):
        client = GuiIV.__new__(GuiIV)
        client._heartbeat_error = None
        with patch.object(client, "result", return_value={"id":"id", "status":"completed"}):
            self.assertEqual(client.wait("id", timeout=None)["status"], "completed")
            with self.assertRaises(ValueError):
                client.wait("id", poll_interval=0)
            with self.assertRaises(ValueError):
                client.wait("id", timeout=float("nan"))

if __name__=='__main__':unittest.main()
