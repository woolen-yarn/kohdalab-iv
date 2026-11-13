"""NativeIV.close must not re-reject a run error the caller already received.

A mock engine stream drives the real reader; no process or instrument is opened.
"""
import io
import json
import threading
import unittest
from unittest.mock import Mock
from kohdalab_iv_native import NativeIV

RUN_ERROR = "Meter read failed during sweep"
SAFE = {"output": "Off", "source_connected": False, "meter_connected": False}


def line(**state):
    return json.dumps({"event": "state", "data": state}) + "\n"


class CloseObservedErrorTests(unittest.TestCase):
    def client(self, returncode=0):
        client = object.__new__(NativeIV)
        client._condition = threading.Condition()
        client._write_lock = threading.Lock()
        client._stop_heartbeat = threading.Event()
        client._state = {}
        client._received_snapshot = False
        client._reader_error = None
        client._observed_error = None
        client._eof = False
        client._closed = False
        client._source_was_connected = False
        client._source2_was_connected = False
        client._gate_was_connected = False
        client._send = Mock()
        client._reader = Mock()
        client._process = Mock(returncode=returncode, pid=1)
        client._process.wait.return_value = returncode
        return client

    def feed(self, client, *lines):
        """Deliver engine snapshots through the real reader."""
        client._eof = False
        client._process.stdout = io.StringIO("".join(lines))
        client._read()

    def failed_run(self, client):
        # Run fails with outputs verified OFF; error is retained in snapshots.
        self.feed(client, line(source_connected=True, output="On", error=""),
                  line(source_connected=True, output="Off", phase="Failed", error=RUN_ERROR))
        with self.assertRaisesRegex(RuntimeError, RUN_ERROR):
            client.wait(lambda _: False, timeout=1)
        # Independent disconnect confirmation still carries the same error.
        self.feed(client, line(error=RUN_ERROR, **SAFE))

    def test_received_run_error_does_not_fail_verified_close(self):
        client = self.client()
        self.failed_run(client)
        client.close()
        client._send.assert_called_once_with("shutdown")
        self.assertTrue(client._closed)

    def test_unreceived_retained_error_still_fails_close(self):
        client = self.client()
        self.feed(client, line(source_connected=True, output="Off", error=RUN_ERROR))
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_new_shutdown_error_still_fails_close(self):
        client = self.client()
        self.failed_run(client)
        self.feed(client, line(error="Device close failed", **SAFE))
        with self.assertRaisesRegex(RuntimeError, "Device close failed"):
            client.close()

    def test_error_reported_again_after_change_is_new(self):
        client = self.client()
        self.failed_run(client)
        self.feed(client, line(error="", **SAFE), line(error=RUN_ERROR, **SAFE))
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_unsafe_output_still_fails_close(self):
        client = self.client()
        self.failed_run(client)
        self.feed(client, line(error=RUN_ERROR, source_connected=False, output="On"))
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_process_failure_still_fails_close(self):
        client = self.client(returncode=1)
        self.failed_run(client)
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_still_connected_fails_close(self):
        client = self.client()
        self.failed_run(client)
        self.feed(client, line(error=RUN_ERROR, source_connected=True, output="Off"))
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_broken_shutdown_stream_fails_close(self):
        client = self.client()
        self.failed_run(client)
        self.feed(client, "broken JSON")
        with self.assertRaisesRegex(RuntimeError, "could not be verified"):
            client.close()

    def test_shutdown_timeout_still_reports_unknown_output(self):
        import subprocess
        client = self.client()
        self.failed_run(client)
        client._process.wait.side_effect = subprocess.TimeoutExpired("iv-cli", 1)
        with self.assertRaisesRegex(TimeoutError, "output status unknown"):
            client.close()


if __name__ == "__main__":
    unittest.main()
