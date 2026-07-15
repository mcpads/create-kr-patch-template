import hashlib
import unittest

from scan_ascii import build_report, scan_null_terminated_ascii


class ScanAsciiTests(unittest.TestCase):
    def test_reports_only_printable_runs_terminated_by_nul(self) -> None:
        data = b"HEAD\x01HELLO\x00TAIL"

        self.assertEqual(
            scan_null_terminated_ascii(data, min_length=4),
            [
                {
                    "offset": 5,
                    "length": 5,
                    "raw_hex": "48 45 4C 4C 4F",
                    "text": "HELLO",
                }
            ],
        )

    def test_report_binds_observations_to_input_identity(self) -> None:
        data = b"TEXT\x00"
        report = build_report(data, min_length=4)

        self.assertEqual(report["source"]["len"], len(data))
        self.assertEqual(report["source"]["sha256"], hashlib.sha256(data).hexdigest())
        self.assertFalse(report["product_input"])

    def test_rejects_non_positive_minimum_length(self) -> None:
        with self.assertRaises(ValueError):
            scan_null_terminated_ascii(b"A\x00", min_length=0)


if __name__ == "__main__":
    unittest.main()
