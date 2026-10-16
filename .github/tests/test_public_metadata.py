"""Synthetic tests of the production publication guard; no personal identifiers."""

import importlib.util
import pathlib
import unittest

path = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "check_public_metadata.py"
spec = importlib.util.spec_from_file_location("public_metadata", path)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class PublicMetadataTests(unittest.TestCase):
    def test_personal_path_locations_without_value_disclosure(self):
        text = 'Heading\n"path": "/Users/private-operator/work/input"\n'
        self.assertEqual(guard.inspect_text(text), [(2, "personal-home-path")])
        self.assertNotIn("private-operator", repr(guard.inspect_text(text)))

    def test_generic_examples_and_icon_names_are_not_private(self):
        self.assertEqual(guard.inspect_text(
            '/private/tmp/build\n<HOME>/work\n/Users/<name>/work\n'
            '/Users/runner/work\n/Users/example/work\n/Users/test-user/fixture\n'
            'icons/128x128@2x.png\nAppIcon-20x20@1x.png\n'
        ), [])

    def test_index_payloads_and_summaries(self):
        self.assertEqual(guard.inspect_text('{"raw_index":{"before_base64":"RElSQw=="}}'),
                         [(1, "embedded-raw-index")])
        self.assertEqual(guard.inspect_text('{"raw_index":{"new_payload_base64":"RElSQw=="}}'),
                         [(1, "embedded-raw-index")])
        self.assertEqual(guard.inspect_text('{"raw_index":{"before":{"sha256":"synthetic","bytes":12},"before_after_equal":true}}'), [])
        self.assertEqual(guard.inspect_text('{"unrelated_base64":"synthetic"}'), [])

    def test_device_context_required_for_serial_shape(self):
        serial = "RF" + "SYNTHETIC99"
        self.assertEqual(guard.inspect_text("adb -s " + serial + " get-state"),
                         [(1, "physical-device-serial")])
        self.assertEqual(guard.inspect_text("device serial: " + serial),
                         [(1, "physical-device-serial")])
        self.assertEqual(guard.inspect_text("public unrelated identifier " + serial), [])
        self.assertEqual(guard.inspect_text("adb -s <DEVICE_SERIAL> get-state"), [])
        self.assertEqual(guard.inspect_text("adb -s emulator-5554 get-state"), [])

    def test_multiple_findings_keep_lines_and_redaction(self):
        text = '/Users/private-operator/work\n{"after_base64":"RElSQw=="}\n'
        self.assertEqual(guard.inspect_text(text),
                         [(1, "personal-home-path"), (2, "embedded-raw-index")])


if __name__ == "__main__":
    unittest.main()
