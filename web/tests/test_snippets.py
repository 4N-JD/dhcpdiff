"""Unit tests for snippet slicing and unknown parsing (no server required)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from app.diff_runner import parse_unknowns_from_text  # noqa: E402
from app.entity_display import (  # noqa: E402
    build_entity_display,
    parse_option_detail_value,
    parse_option_scope_and_ref,
    parse_option_space_code,
)
from app.snippets import enrich_entries_with_snippets, slice_block  # noqa: E402


class SnippetTests(unittest.TestCase):
    def test_slice_with_end_line(self):
        text = "a\nb\nc\nd\ne\n"
        snip = slice_block(text, 2, 4, focus_line=3)
        assert snip is not None
        self.assertEqual(snip["text"], "b\nc\nd")
        self.assertEqual(snip["highlight_lines"], [2])

    def test_brace_match(self):
        text = "subnet 10.0.0.0 netmask 255.255.255.0 {\n  pool {\n    range 1 2;\n  }\n}\n"
        snip = slice_block(text, 1, None)
        assert snip is not None
        self.assertIn("subnet", snip["text"])
        self.assertTrue(snip["text"].rstrip().endswith("}"))


class UnknownParseTests(unittest.TestCase):
    def test_parse_unknowns(self):
        text = """Unknown options requiring mapping (2):
  - Avaya-IP-Phone (usages: 3, sites: subnet, pool)
  - weird-opt (usages: 1, sites: global)
unmapped options remain; run 'dhcpdiff map' or pass --ignore-unmapped
"""
        unknowns = parse_unknowns_from_text(text)
        self.assertEqual(len(unknowns), 2)
        self.assertEqual(unknowns[0]["raw_name"], "Avaya-IP-Phone")
        self.assertEqual(unknowns[0]["usage_count"], 3)


class EnrichTests(unittest.TestCase):
    def test_enrich(self):
        source = "subnet x {\n  option a;\n}\n"
        target = "subnet x {\n  option b;\n}\n"
        report = {
            "entries": [
                {
                    "category": "changed",
                    "locations": {
                        "source": {"file": "a.conf", "line": 1, "end_line": 3},
                        "target": {"file": "b.conf", "line": 1, "end_line": 3},
                    },
                }
            ]
        }
        out = enrich_entries_with_snippets(report, source, target)
        self.assertIn("snippets", out["entries"][0])
        self.assertIn("option a", out["entries"][0]["snippets"]["source"]["text"])

    def test_enrich_nested_affected(self):
        source = "line1\nline2\nline3\n"
        target = "line1\nline2\nline3\n"
        report = {
            "entries": [
                {
                    "category": "changed",
                    "locations": {
                        "source": {
                            "affected": {"file": "a.conf", "line": 2, "end_line": 3},
                            "declaration": {"file": "a.conf", "line": 1, "end_line": 1},
                        },
                        "target": {
                            "affected": {"file": "b.conf", "line": 2, "end_line": 2},
                        },
                    },
                }
            ]
        }
        out = enrich_entries_with_snippets(report, source, target)
        self.assertEqual(out["entries"][0]["snippets"]["source"]["start_line"], 2)
        self.assertEqual(out["entries"][0]["snippets"]["source"]["text"], "line2\nline3")


class EntityDisplayTests(unittest.TestCase):
    def test_option_with_vci(self):
        d = build_entity_display(
            "option",
            "pool:10.162.40.0/24:10.162.40.250-10.162.40.251:vci=PXEClient:bootp:0 (next-server)",
        )
        self.assertEqual(d["object_type"], "Option")
        self.assertEqual(d["name"], "bootp:0 (next-server)")
        self.assertEqual(
            d["parent"], "Pool 10.162.40.250-10.162.40.251 in subnet 10.162.40.0/24"
        )
        self.assertEqual(
            d["parent_key"], "pool:10.162.40.0/24:10.162.40.250-10.162.40.251"
        )
        self.assertEqual(d["option_id"], "bootp:0")
        self.assertEqual(d["vci"], "PXEClient")
        self.assertIn("affects clients with VCI PXEClient", d["summary"])

    def test_option_with_colonful_arch_vci(self):
        d = build_entity_display(
            "option",
            "pool:10.64.112.0/23:10.64.112.2-10.64.113.249:vci=PXEClient:Arch:00000:isc:0 (default-lease-time)",
        )
        self.assertEqual(d["name"], "isc:0 (default-lease-time)")
        self.assertEqual(d["option_id"], "isc:0")
        self.assertEqual(d["vci"], "PXEClient:Arch:00000")
        self.assertIn(
            "affects clients with VCI PXEClient:Arch:00000", d["summary"]
        )

    def test_pool_and_filter_parent_key(self):
        pool = build_entity_display("pool", "10.0.80.0/24:10.0.80.1-10.0.80.10")
        self.assertEqual(pool["parent_key"], "10.0.80.0/24")
        filt = build_entity_display("filter", "10.0.80.0/24:MyFilter:match")
        self.assertEqual(filt["parent_key"], "10.0.80.0/24")
        res = build_entity_display("reservation", "10.0.80.49")
        self.assertIsNone(res["parent_key"])

    def test_parse_option_space_code(self):
        self.assertEqual(
            parse_option_space_code("MSFT50:1 (foo)"),
            {"space": "MSFT50", "code": 1},
        )
        self.assertEqual(
            parse_option_space_code("global:Microsoft-Windows-Options:2"),
            {"space": "Microsoft-Windows-Options", "code": 2},
        )
        self.assertIsNone(parse_option_space_code("not-an-option"))

    def test_parse_option_scope_and_ref(self):
        ref = parse_option_scope_and_ref("10.0.0.0/24:MSFT50:3 (x)")
        self.assertEqual(ref["scope"], "10.0.0.0/24")
        self.assertEqual(ref["space"], "MSFT50")
        self.assertEqual(ref["code"], 3)

    def test_parse_option_detail_value(self):
        self.assertEqual(
            parse_option_detail_value(
                'option MSFT50:1 (foo) = String("bar") missing in target'
            ),
            'String("bar")',
        )
        self.assertEqual(
            parse_option_detail_value("option dhcp:6 = IpList([1.2.3.4]) extra in target"),
            "IpList([1.2.3.4])",
        )
        self.assertIsNone(parse_option_detail_value("changed somehow"))


if __name__ == "__main__":
    unittest.main()
