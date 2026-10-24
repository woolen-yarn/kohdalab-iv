"""Client contract for independent source limits; no instrument is opened."""
import threading
import unittest
from unittest.mock import Mock
from kohdalab_iv_native import NativeIV


class SourceProtectionClientTests(unittest.TestCase):
    def client(self, default_config):
        client = object.__new__(NativeIV)
        client.default_config = default_config
        client._condition = threading.Condition()
        client._state = {'run_id': 1}
        client._send = Mock()
        return client

    def test_source_limit_stop_is_terminal_and_independent_limits_are_forwarded(self):
        client = self.client({'source_compliance': None})
        result = {'run_id': 2, 'phase': 'Source limit stop', 'output': 'Off'}
        def wait(predicate, timeout):
            self.assertTrue(predicate(result))
            return result
        client.wait = wait
        self.assertEqual(client.run('unused.csv', {'source_compliance': 5., 'compliance': .2}), result)
        values = client._send.call_args.kwargs['config']
        self.assertEqual(values['source_compliance'], 5.)
        self.assertEqual(values['compliance'], .2)

    def test_explicit_new_source_limit_cannot_be_silently_ignored_by_old_engine(self):
        client = self.client({})
        with self.assertRaisesRegex(RuntimeError, 'newer KohdaLab IV engine'):
            client.run('unused.csv', {'source_compliance': 5.})
        client._send.assert_not_called()

    def test_legacy_engine_can_still_run_the_legacy_coupled_configuration(self):
        client = self.client({})
        client.wait = lambda predicate, timeout: {'run_id': 2, 'phase': 'Completed', 'output': 'Off'}
        client.run('unused.csv', {'compliance': .001})
        client._send.assert_called_once()
