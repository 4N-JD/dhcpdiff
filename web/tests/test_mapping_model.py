"""Unit tests for user mapping YAML parse helper."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from app.mapping_model import EMPTY_MAPPING, parse_user_mapping  # noqa: E402


SAMPLE = """
aliases:
- source_name: arp-cache-timeout
  canonical:
    space: dhcp
    code: 35
  note: mapped via TUI
equivalences:
- source:
    space: MSFT50
    code: 1
  target:
    space: Microsoft-Windows-Options
    code: 1
  confirmed: true
ignore:
- space: dhcp
  code: 28
ignore_subnet_mask: true
"""


class ParseUserMappingTests(unittest.TestCase):
    def test_empty_and_invalid(self):
        self.assertEqual(parse_user_mapping(""), EMPTY_MAPPING)
        self.assertEqual(parse_user_mapping("   "), EMPTY_MAPPING)
        self.assertEqual(parse_user_mapping(": not: valid: ["), EMPTY_MAPPING)
        self.assertEqual(parse_user_mapping("42"), EMPTY_MAPPING)

    def test_full_sample_migrates_legacy_flag(self):
        m = parse_user_mapping(SAMPLE)
        self.assertEqual(len(m["aliases"]), 1)
        self.assertEqual(m["aliases"][0]["source_name"], "arp-cache-timeout")
        self.assertEqual(m["aliases"][0]["canonical"], {"space": "dhcp", "code": 35})
        self.assertEqual(m["aliases"][0]["note"], "mapped via TUI")
        self.assertEqual(len(m["equivalences"]), 1)
        self.assertEqual(m["equivalences"][0]["source"], {"space": "MSFT50", "code": 1})
        self.assertEqual(
            m["equivalences"][0]["target"],
            {"space": "Microsoft-Windows-Options", "code": 1},
        )
        self.assertTrue(m["equivalences"][0]["confirmed"])
        self.assertEqual(
            m["ignore"],
            [{"space": "dhcp", "code": 28}, {"space": "dhcp", "code": 1}],
        )
        self.assertNotIn("ignore_subnet_mask", m)

    def test_missing_keys_and_flow_style(self):
        text = """
aliases: []
equivalences:
- source: { space: A, code: 2 }
  target: { space: B, code: 3 }
  confirmed: false
ignore_subnet_mask: false
"""
        m = parse_user_mapping(text)
        self.assertEqual(m["aliases"], [])
        self.assertEqual(m["ignore"], [])
        self.assertEqual(len(m["equivalences"]), 1)
        self.assertFalse(m["equivalences"][0]["confirmed"])
        self.assertNotIn("ignore_subnet_mask", m)

    def test_legacy_flag_does_not_duplicate_dhcp1(self):
        text = """
ignore:
- space: dhcp
  code: 1
ignore_subnet_mask: true
"""
        m = parse_user_mapping(text)
        self.assertEqual(m["ignore"], [{"space": "dhcp", "code": 1}])

    def test_skips_malformed_entries(self):
        text = """
aliases:
- source_name: only-name
- source_name: ok
  canonical: { space: dhcp, code: 6 }
equivalences:
- source: { space: X, code: 1 }
ignore:
- space: dhcp
"""
        m = parse_user_mapping(text)
        self.assertEqual(len(m["aliases"]), 1)
        self.assertEqual(m["aliases"][0]["source_name"], "ok")
        self.assertEqual(m["equivalences"], [])
        self.assertEqual(m["ignore"], [])


class ParseMappingApiTests(unittest.TestCase):
    def setUp(self):
        try:
            from fastapi.testclient import TestClient
            from app.main import app
        except ImportError as e:
            self.skipTest(f"fastapi test client unavailable: {e}")
        self.client = TestClient(app)

    def test_parse_mapping_form_text(self):
        res = self.client.post(
            "/api/mapping/parse",
            data={
                "mapping_yaml": "aliases: []\nequivalences:\n"
                '- source: { space: A, code: 1 }\n'
                "  target: { space: B, code: 2 }\n"
                "  confirmed: true\n"
                "ignore: []\n"
                "ignore_subnet_mask: true\n"
            },
        )
        self.assertEqual(res.status_code, 200, res.text)
        body = res.json()
        self.assertEqual(len(body["mapping"]["equivalences"]), 1)
        self.assertEqual(body["mapping"]["equivalences"][0]["source"]["space"], "A")
        self.assertEqual(body["mapping"]["ignore"], [{"space": "dhcp", "code": 1}])
        self.assertNotIn("ignore_subnet_mask", body["mapping"])

    def test_parse_mapping_file_upload(self):
        content = b"aliases:\n- source_name: foo\n  canonical: { space: dhcp, code: 6 }\nequivalences: []\nignore: []\n"
        res = self.client.post(
            "/api/mapping/parse",
            files={"file": ("user.yaml", content, "text/yaml")},
        )
        self.assertEqual(res.status_code, 200, res.text)
        self.assertEqual(res.json()["mapping"]["aliases"][0]["source_name"], "foo")

    def test_parse_mapping_empty_rejected(self):
        res = self.client.post("/api/mapping/parse", data={"mapping_yaml": "  "})
        self.assertEqual(res.status_code, 400)


if __name__ == "__main__":
    unittest.main()
