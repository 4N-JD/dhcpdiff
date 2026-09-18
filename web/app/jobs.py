"""Filesystem-backed diff jobs with line-offset indexes and entry paging."""

from __future__ import annotations

import json
import os
import shutil
import struct
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .diff_runner import DiffRunResult, run_diff
from .entity_display import enrich_entries_with_display

OFFSET_FMT = "<Q"
OFFSET_SIZE = struct.calcsize(OFFSET_FMT)
MAX_LINE_WINDOW = 500
DEFAULT_JOB_DIR = "/tmp/dhcpdiff-jobs"
DEFAULT_TTL_HOURS = 24.0

_report_cache: dict[str, dict[str, Any]] = {}
_index_cache: dict[str, dict[str, list[int]]] = {}


class JobError(Exception):
    def __init__(self, message: str, *, status_code: int = 400, detail: dict[str, Any] | None = None):
        super().__init__(message)
        self.message = message
        self.status_code = status_code
        self.detail = detail or {"error": "error", "message": message}


class JobNotFound(JobError):
    def __init__(self, job_id: str):
        super().__init__(
            f"job not found: {job_id}",
            status_code=404,
            detail={"error": "not_found", "message": f"job not found: {job_id}"},
        )


@dataclass
class JobSummary:
    job_id: str
    counts: dict[str, Any]
    files: dict[str, Any]
    source_vendor: str
    target_vendor: str
    has_differences: bool

    def as_dict(self) -> dict[str, Any]:
        return {
            "job_id": self.job_id,
            "counts": self.counts,
            "files": self.files,
            "source_vendor": self.source_vendor,
            "target_vendor": self.target_vendor,
            "has_differences": self.has_differences,
        }


def job_root() -> Path:
    root = Path(os.environ.get("DHCPDIFF_JOB_DIR", DEFAULT_JOB_DIR))
    root.mkdir(parents=True, exist_ok=True)
    return root


def ttl_seconds() -> float:
    raw = os.environ.get("DHCPDIFF_JOB_TTL_HOURS", str(DEFAULT_TTL_HOURS))
    try:
        hours = float(raw)
    except ValueError:
        hours = DEFAULT_TTL_HOURS
    return max(hours, 0.0) * 3600.0


def job_dir(job_id: str) -> Path:
    # Reject path traversal
    if not job_id or "/" in job_id or "\\" in job_id or job_id in (".", ".."):
        raise JobNotFound(job_id)
    path = job_root() / job_id
    if not path.is_dir():
        raise JobNotFound(job_id)
    return path


def build_line_offsets(data: bytes) -> list[int]:
    """Return byte offsets for the start of each line (1-based line i at offsets[i-1])."""
    if not data:
        return []
    offsets = [0]
    for i, b in enumerate(data):
        if b == 0x0A and i + 1 < len(data):
            offsets.append(i + 1)
    return offsets


def write_offsets(path: Path, offsets: list[int]) -> None:
    with path.open("wb") as f:
        for off in offsets:
            f.write(struct.pack(OFFSET_FMT, off))


def load_offsets(path: Path) -> list[int]:
    data = path.read_bytes()
    if len(data) % OFFSET_SIZE != 0:
        raise JobError(
            "corrupt offset index",
            status_code=500,
            detail={"error": "corrupt_index", "message": "corrupt offset index"},
        )
    count = len(data) // OFFSET_SIZE
    return [struct.unpack_from(OFFSET_FMT, data, i * OFFSET_SIZE)[0] for i in range(count)]


def cleanup_expired_jobs() -> None:
    root = job_root()
    ttl = ttl_seconds()
    if ttl <= 0:
        return
    now = time.time()
    for child in root.iterdir():
        if not child.is_dir():
            continue
        meta_path = child / "meta.json"
        created = None
        if meta_path.is_file():
            try:
                meta = json.loads(meta_path.read_text(encoding="utf-8"))
                created = float(meta.get("created_at", 0))
            except (OSError, ValueError, TypeError, json.JSONDecodeError):
                created = None
        if created is None:
            try:
                created = meta_path.stat().st_mtime if meta_path.is_file() else child.stat().st_mtime
            except OSError:
                continue
        if now - created > ttl:
            job_id = child.name
            _report_cache.pop(job_id, None)
            _index_cache.pop(job_id, None)
            shutil.rmtree(child, ignore_errors=True)


def _side_paths(base: Path, side: str) -> tuple[Path, Path]:
    if side == "source":
        return base / "source.conf", base / "source.offsets"
    if side == "target":
        return base / "target.conf", base / "target.offsets"
    raise JobError(
        f"invalid side: {side}",
        status_code=400,
        detail={"error": "bad_side", "message": "side must be source or target"},
    )


def _get_offsets(job_id: str, side: str) -> list[int]:
    cached = _index_cache.get(job_id)
    if cached and side in cached:
        return cached[side]
    conf_path, off_path = _side_paths(job_dir(job_id), side)
    if not off_path.is_file():
        raise JobError(
            f"missing offset index for {side}",
            status_code=500,
            detail={"error": "missing_index", "message": f"missing offset index for {side}"},
        )
    offsets = load_offsets(off_path)
    _index_cache.setdefault(job_id, {})[side] = offsets
    # conf_path existence checked by callers of read_lines
    _ = conf_path
    return offsets


def load_report(job_id: str) -> dict[str, Any]:
    if job_id in _report_cache:
        return _report_cache[job_id]
    path = job_dir(job_id) / "report.json"
    if not path.is_file():
        raise JobError(
            "report missing",
            status_code=500,
            detail={"error": "missing_report", "message": "report missing"},
        )
    report = json.loads(path.read_text(encoding="utf-8"))
    _report_cache[job_id] = report
    return report


def load_meta(job_id: str) -> dict[str, Any]:
    path = job_dir(job_id) / "meta.json"
    if not path.is_file():
        raise JobNotFound(job_id)
    return json.loads(path.read_text(encoding="utf-8"))


def create_job_from_upload(
    *,
    source_bytes: bytes,
    target_bytes: bytes,
    source_name: str,
    target_name: str,
    mapping_text: str,
    source_vendor: str,
    target_vendor: str,
    ignore_unmapped: bool,
    ignore_subnet_mask: bool,
) -> JobSummary:
    cleanup_expired_jobs()

    job_id = uuid.uuid4().hex
    base = job_root() / job_id
    base.mkdir(parents=True, exist_ok=False)

    source_path = base / "source.conf"
    target_path = base / "target.conf"
    mapping_path = base / "user.yaml"

    source_path.write_bytes(source_bytes)
    target_path.write_bytes(target_bytes)
    mapping_path.write_text(mapping_text, encoding="utf-8")

    source_offsets = build_line_offsets(source_bytes)
    target_offsets = build_line_offsets(target_bytes)
    write_offsets(base / "source.offsets", source_offsets)
    write_offsets(base / "target.offsets", target_offsets)

    result: DiffRunResult = run_diff(
        source_path=source_path,
        target_path=target_path,
        source_vendor=source_vendor,
        target_vendor=target_vendor,
        mapping_path=mapping_path,
        ignore_unmapped=ignore_unmapped,
        ignore_subnet_mask=ignore_subnet_mask,
    )

    if not result.ok:
        status = 400 if result.error_kind == "unmapped" else 500
        if result.error_kind == "binary_missing":
            status = 503
        shutil.rmtree(base, ignore_errors=True)
        raise JobError(
            result.error or "diff failed",
            status_code=status,
            detail={
                "error": result.error_kind or "error",
                "message": result.error,
                "output": result.output,
                "unknowns": result.unknowns or [],
            },
        )

    report = enrich_entries_with_display(result.report or {})
    # Do not embed file bodies; locations drive the virtualized viewer.
    report.pop("files", None)

    (base / "report.json").write_text(
        json.dumps(report, separators=(",", ":"), ensure_ascii=False),
        encoding="utf-8",
    )

    counts = report.get("counts") or {}
    meta = {
        "job_id": job_id,
        "created_at": time.time(),
        "source_name": source_name,
        "target_name": target_name,
        "source_vendor": source_vendor,
        "target_vendor": target_vendor,
        "counts": counts,
        "has_differences": bool(report.get("has_differences")),
        "files": {
            "source": {"name": source_name, "line_count": len(source_offsets)},
            "target": {"name": target_name, "line_count": len(target_offsets)},
        },
    }
    (base / "meta.json").write_text(json.dumps(meta), encoding="utf-8")

    _report_cache[job_id] = report
    _index_cache[job_id] = {"source": source_offsets, "target": target_offsets}

    return JobSummary(
        job_id=job_id,
        counts=counts,
        files=meta["files"],
        source_vendor=source_vendor,
        target_vendor=target_vendor,
        has_differences=bool(report.get("has_differences")),
    )


def get_job_summary(job_id: str) -> dict[str, Any]:
    cleanup_expired_jobs()
    meta = load_meta(job_id)
    return {
        "job_id": job_id,
        "counts": meta.get("counts") or {},
        "files": meta.get("files") or {},
        "source_vendor": meta.get("source_vendor"),
        "target_vendor": meta.get("target_vendor"),
        "has_differences": meta.get("has_differences"),
        "created_at": meta.get("created_at"),
    }


def _filtered_indices(entries: list[dict[str, Any]], category: str) -> list[int]:
    if not category or category == "all":
        return list(range(len(entries)))
    return [i for i, e in enumerate(entries) if e.get("category") == category]


def list_entries(
    job_id: str,
    *,
    category: str = "all",
    offset: int = 0,
    limit: int = 100,
) -> dict[str, Any]:
    if offset < 0:
        offset = 0
    if limit < 1:
        limit = 1
    if limit > 500:
        limit = 500

    report = load_report(job_id)
    entries = report.get("entries") or []
    indices = _filtered_indices(entries, category)
    total = len(indices)
    page = indices[offset : offset + limit]
    items = []
    for i in page:
        entry = entries[i]
        entity = entry.get("entity") or {}
        items.append(
            {
                "index": i,
                "category": entry.get("category"),
                "entity": {"display": entity.get("display"), "kind": entity.get("kind"), "key": entity.get("key")},
            }
        )
    return {
        "total": total,
        "offset": offset,
        "limit": limit,
        "category": category or "all",
        "entries": items,
    }


def get_entry(job_id: str, index: int) -> dict[str, Any]:
    report = load_report(job_id)
    entries = report.get("entries") or []
    if index < 0 or index >= len(entries):
        raise JobError(
            f"entry index out of range: {index}",
            status_code=404,
            detail={"error": "not_found", "message": f"entry index out of range: {index}"},
        )
    entry = dict(entries[index])
    entry["index"] = index
    return entry


def read_lines(job_id: str, side: str, start: int, end: int) -> dict[str, Any]:
    if start < 1:
        start = 1
    if end < start:
        end = start
    if end - start + 1 > MAX_LINE_WINDOW:
        end = start + MAX_LINE_WINDOW - 1

    base = job_dir(job_id)
    conf_path, _off_path = _side_paths(base, side)
    offsets = _get_offsets(job_id, side)
    line_count = len(offsets)
    if line_count == 0:
        return {"side": side, "start": 1, "end": 0, "line_count": 0, "lines": []}

    start = min(start, line_count)
    end = min(end, line_count)

    byte_start = offsets[start - 1]
    if end < line_count:
        byte_end = offsets[end]
    else:
        byte_end = conf_path.stat().st_size

    with conf_path.open("rb") as f:
        f.seek(byte_start)
        chunk = f.read(byte_end - byte_start)

    text = chunk.decode("utf-8", errors="replace")
    lines = text.splitlines()
    # Guard against off-by-one if trailing content differs
    expected = end - start + 1
    if len(lines) > expected:
        lines = lines[:expected]
    elif len(lines) < expected:
        lines.extend([""] * (expected - len(lines)))

    return {
        "side": side,
        "start": start,
        "end": end,
        "line_count": line_count,
        "lines": lines,
    }
