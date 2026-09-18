"""Unit tests for job store: offsets, line reads, and entry paging."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from app.jobs import (  # noqa: E402
    JobError,
    build_line_offsets,
    create_job_from_upload,
    get_entry,
    list_entries,
    load_offsets,
    read_lines,
    write_offsets,
)


class OffsetIndexTests(unittest.TestCase):
    def test_build_offsets_trailing_newline(self):
        data = b"a\nb\nc\n"
        offsets = build_line_offsets(data)
        self.assertEqual(offsets, [0, 2, 4])

    def test_build_offsets_no_trailing_newline(self):
        data = b"a\nb\nc"
        offsets = build_line_offsets(data)
        self.assertEqual(offsets, [0, 2, 4])

    def test_build_offsets_empty(self):
        self.assertEqual(build_line_offsets(b""), [])

    def test_roundtrip_offsets_file(self):
        offsets = [0, 10, 20, 30]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.offsets"
            write_offsets(path, offsets)
            self.assertEqual(load_offsets(path), offsets)


class LineReadTests(unittest.TestCase):
    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmpdir.cleanup)
        os.environ["DHCPDIFF_JOB_DIR"] = self._tmpdir.name
        # Clear caches between tests
        import app.jobs as jobs

        jobs._report_cache.clear()
        jobs._index_cache.clear()

    def _seed_job(self, text: str) -> str:
        import app.jobs as jobs

        job_id = "abc123seed"
        base = Path(self._tmpdir.name) / job_id
        base.mkdir()
        data = text.encode("utf-8")
        (base / "source.conf").write_bytes(data)
        offsets = build_line_offsets(data)
        write_offsets(base / "source.offsets", offsets)
        (base / "target.conf").write_bytes(data)
        write_offsets(base / "target.offsets", offsets)
        (base / "meta.json").write_text(
            json.dumps(
                {
                    "job_id": job_id,
                    "created_at": 0,
                    "files": {
                        "source": {"name": "s.conf", "line_count": len(offsets)},
                        "target": {"name": "t.conf", "line_count": len(offsets)},
                    },
                    "counts": {"total": 0},
                }
            ),
            encoding="utf-8",
        )
        (base / "report.json").write_text(
            json.dumps({"entries": [], "counts": {"total": 0}}),
            encoding="utf-8",
        )
        jobs._index_cache.clear()
        return job_id

    def test_read_lines_window(self):
        job_id = self._seed_job("one\ntwo\nthree\nfour\n")
        out = read_lines(job_id, "source", 2, 3)
        self.assertEqual(out["start"], 2)
        self.assertEqual(out["end"], 3)
        self.assertEqual(out["lines"], ["two", "three"])
        self.assertEqual(out["line_count"], 4)

    def test_read_lines_clamps_and_caps_window(self):
        lines = "\n".join(f"L{i}" for i in range(1, 601)) + "\n"
        job_id = self._seed_job(lines)
        out = read_lines(job_id, "source", 1, 600)
        self.assertEqual(out["start"], 1)
        self.assertEqual(out["end"], 500)
        self.assertEqual(len(out["lines"]), 500)
        self.assertEqual(out["lines"][0], "L1")
        self.assertEqual(out["lines"][-1], "L500")


class EntryPagingTests(unittest.TestCase):
    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmpdir.cleanup)
        os.environ["DHCPDIFF_JOB_DIR"] = self._tmpdir.name
        import app.jobs as jobs

        jobs._report_cache.clear()
        jobs._index_cache.clear()

        self.job_id = "entryjob1"
        base = Path(self._tmpdir.name) / self.job_id
        base.mkdir()
        entries = [
            {"category": "missing", "entity": {"kind": "subnet", "key": "10.0.0.0/24", "display": {"object_type": "Subnet", "name": "10.0.0.0/24", "summary": "Subnet 10.0.0.0/24"}}, "detail": "a"},
            {"category": "extra", "entity": {"kind": "subnet", "key": "10.0.1.0/24", "display": {"object_type": "Subnet", "name": "10.0.1.0/24", "summary": "Subnet 10.0.1.0/24"}}, "detail": "b"},
            {"category": "missing", "entity": {"kind": "pool", "key": "x", "display": {"object_type": "Pool", "name": "x", "summary": "Pool x"}}, "detail": "c"},
        ]
        (base / "report.json").write_text(
            json.dumps({"entries": entries, "counts": {"total": 3, "missing": 2, "extra": 1}}),
            encoding="utf-8",
        )
        (base / "meta.json").write_text(
            json.dumps({"job_id": self.job_id, "created_at": 0, "counts": {"total": 3}}),
            encoding="utf-8",
        )
        (base / "source.conf").write_bytes(b"x\n")
        (base / "target.conf").write_bytes(b"x\n")
        write_offsets(base / "source.offsets", [0])
        write_offsets(base / "target.offsets", [0])

    def test_list_all_and_filter(self):
        all_page = list_entries(self.job_id, category="all", offset=0, limit=10)
        self.assertEqual(all_page["total"], 3)
        self.assertEqual(len(all_page["entries"]), 3)
        self.assertEqual(all_page["entries"][0]["index"], 0)

        missing = list_entries(self.job_id, category="missing", offset=0, limit=10)
        self.assertEqual(missing["total"], 2)
        self.assertEqual([e["index"] for e in missing["entries"]], [0, 2])

        page = list_entries(self.job_id, category="all", offset=1, limit=1)
        self.assertEqual(page["total"], 3)
        self.assertEqual(page["entries"][0]["index"], 1)

    def test_get_entry(self):
        entry = get_entry(self.job_id, 1)
        self.assertEqual(entry["category"], "extra")
        self.assertEqual(entry["index"], 1)
        with self.assertRaises(JobError):
            get_entry(self.job_id, 99)


class CreateJobApiTests(unittest.TestCase):
    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmpdir.cleanup)
        os.environ["DHCPDIFF_JOB_DIR"] = self._tmpdir.name
        import app.jobs as jobs

        jobs._report_cache.clear()
        jobs._index_cache.clear()

    def test_create_job_with_mocked_diff(self):
        fake_report = {
            "entries": [
                {
                    "category": "changed",
                    "entity": {"kind": "subnet", "key": "10.0.0.0/8"},
                    "detail": "opt",
                }
            ],
            "counts": {"total": 1, "changed": 1, "missing": 0, "extra": 0, "unmapped": 0},
            "has_differences": True,
        }
        fake_result = mock.Mock(
            ok=True,
            report=fake_report,
            error=None,
            error_kind=None,
            output=None,
            unknowns=None,
        )
        with mock.patch("app.jobs.run_diff", return_value=fake_result):
            summary = create_job_from_upload(
                source_bytes=b"subnet 10.0.0.0 netmask 255.0.0.0 {\n}\n",
                target_bytes=b"subnet 10.0.0.0 netmask 255.0.0.0 {\n}\n",
                source_name="a.conf",
                target_name="b.conf",
                mapping_text="aliases: []\nequivalences: []\nignore: []\n",
                source_vendor="auto",
                target_vendor="auto",
                ignore_unmapped=True,
                ignore_subnet_mask=True,
            )
        self.assertTrue(summary.job_id)
        self.assertEqual(summary.counts["total"], 1)
        self.assertEqual(summary.files["source"]["line_count"], 2)

        lines = read_lines(summary.job_id, "source", 1, 2)
        self.assertEqual(len(lines["lines"]), 2)

        page = list_entries(summary.job_id, category="all", offset=0, limit=10)
        self.assertEqual(page["total"], 1)
        entry = get_entry(summary.job_id, 0)
        self.assertIn("display", entry["entity"])


class FastApiJobsTests(unittest.TestCase):
    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmpdir.cleanup)
        os.environ["DHCPDIFF_JOB_DIR"] = self._tmpdir.name
        import app.jobs as jobs

        jobs._report_cache.clear()
        jobs._index_cache.clear()

        try:
            from fastapi.testclient import TestClient
            from app.main import app
        except ImportError as e:
            self.skipTest(f"fastapi test client unavailable: {e}")

        self.client = TestClient(app)
        fixtures = Path(__file__).resolve().parents[2] / "tests" / "fixtures"
        self.source = fixtures / "paired_qip.conf"
        self.target = fixtures / "paired_infoblox.conf"
        if not self.source.is_file() or not self.target.is_file():
            self.skipTest("fixtures missing")

    def test_jobs_endpoints_with_real_cli(self):
        bin_path = Path(__file__).resolve().parents[2] / "target" / "release" / "dhcpdiff"
        if not bin_path.is_file():
            self.skipTest("release binary not built")
        os.environ["DHCPDIFF_BIN"] = str(bin_path)

        with self.source.open("rb") as sf, self.target.open("rb") as tf:
            res = self.client.post(
                "/api/jobs",
                files={
                    "source": ("paired_qip.conf", sf, "text/plain"),
                    "target": ("paired_infoblox.conf", tf, "text/plain"),
                },
                data={
                    "source_vendor": "qip",
                    "target_vendor": "infoblox",
                    "ignore_unmapped": "true",
                    "ignore_subnet_mask": "true",
                    "mapping_yaml": "aliases: []\nequivalences: []\nignore: []\n",
                },
            )
        self.assertEqual(res.status_code, 200, res.text)
        body = res.json()
        self.assertIn("job_id", body)
        self.assertNotIn("text", body.get("files", {}).get("source", {}))
        job_id = body["job_id"]

        meta = self.client.get(f"/api/jobs/{job_id}")
        self.assertEqual(meta.status_code, 200)

        entries = self.client.get(f"/api/jobs/{job_id}/entries?limit=5")
        self.assertEqual(entries.status_code, 200)
        page = entries.json()
        self.assertIn("entries", page)
        self.assertIn("total", page)

        if page["total"] > 0:
            idx = page["entries"][0]["index"]
            one = self.client.get(f"/api/jobs/{job_id}/entries/{idx}")
            self.assertEqual(one.status_code, 200)
            self.assertIn("locations", one.json() or {"locations": {}})

        lines = self.client.get(f"/api/jobs/{job_id}/files/source/lines?start=1&end=5")
        self.assertEqual(lines.status_code, 200)
        payload = lines.json()
        self.assertGreaterEqual(payload["line_count"], 1)
        self.assertTrue(payload["lines"])


if __name__ == "__main__":
    unittest.main()
