"""Protocol compatibility checks; no engine process or device is started."""
import unittest

from kohdalab_iv_native import (_negotiate_protocol, SETTINGS_SCHEMA_VERSION, RUN_METADATA_SCHEMA_VERSION, CSV_SCHEMA)


CURRENT = {
    "version": 1,
    "minimum_client_version": 1,
    "settings_schema_version": SETTINGS_SCHEMA_VERSION,
    "run_metadata_schema_version": RUN_METADATA_SCHEMA_VERSION,
    "csv_schema": CSV_SCHEMA,
}


class ProtocolCompatibilityTests(unittest.TestCase):
    def test_missing_declaration_is_the_supported_legacy_engine(self):
        info = _negotiate_protocol()
        self.assertTrue(info["legacy"])
        self.assertEqual(info["version"], 1)

    def test_current_declaration_is_accepted(self):
        info = _negotiate_protocol(CURRENT)
        self.assertFalse(info["legacy"])
        self.assertEqual(info["csv_schema"], CSV_SCHEMA)

    def test_legacy_analysis_v3_is_still_accepted(self):
        info = _negotiate_protocol({**CURRENT, "settings_schema_version": 3,
                                    "run_metadata_schema_version": 8,
                                    "csv_schema": "analysis_v3"})
        self.assertEqual(info["csv_schema"], "analysis_v3")

    def test_future_declarations_explain_required_upgrade(self):
        future_protocol = {**CURRENT, "version": 2}
        with self.assertRaisesRegex(RuntimeError, "newer than this Python client"):
            _negotiate_protocol(future_protocol)
        future_settings = {**CURRENT, "settings_schema_version": SETTINGS_SCHEMA_VERSION + 1}
        with self.assertRaisesRegex(RuntimeError, f"settings schema {SETTINGS_SCHEMA_VERSION + 1}"):
            _negotiate_protocol(future_settings)
        future_csv = {**CURRENT, "csv_schema": "analysis_v999"}
        with self.assertRaisesRegex(RuntimeError, "CSV schema 'analysis_v999'"):
            _negotiate_protocol(future_csv)
        future_metadata = {**CURRENT, "run_metadata_schema_version": RUN_METADATA_SCHEMA_VERSION + 1}
        with self.assertRaisesRegex(RuntimeError, f"run metadata schema {RUN_METADATA_SCHEMA_VERSION + 1}"):
            _negotiate_protocol(future_metadata)

    def test_malformed_declarations_are_rejected_before_connect(self):
        missing_csv = dict(CURRENT)
        del missing_csv["csv_schema"]
        with self.assertRaisesRegex(RuntimeError, "missing csv_schema"):
            _negotiate_protocol(missing_csv)
        with self.assertRaisesRegex(RuntimeError, "non-negative integer"):
            _negotiate_protocol({**CURRENT, "minimum_client_version": True})
        with self.assertRaisesRegex(RuntimeError, "incompatible version range"):
            _negotiate_protocol({**CURRENT, "minimum_client_version": 2})


if __name__ == "__main__":
    unittest.main()
