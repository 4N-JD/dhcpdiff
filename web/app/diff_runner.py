"""Run dhcpdiff CLI and interpret results."""

from __future__ import annotations

import json
import os
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_VENDORS = ["auto", "qip", "infoblox", "bluecat", "microsoft"]

DEFAULT_MAPPING_YAML = """aliases: []
equivalences: []
ignore: []
ignore_subnet_mask: true
"""


def dhcpdiff_bin() -> str:
    return os.environ.get("DHCPDIFF_BIN", "dhcpdiff")


@dataclass
class DiffRunResult:
    ok: bool
    report: dict[str, Any] | None = None
    error: str | None = None
    error_kind: str | None = None
    output: str | None = None
    unknowns: list[dict[str, Any]] | None = None
    returncode: int | None = None


def parse_unknowns_from_text(text: str) -> list[dict[str, Any]]:
    """Best-effort parse of CLI unknown-option listing."""
    unknowns: list[dict[str, Any]] = []
    # Lines like:  - option-name (usages: 3, sites: a, b)
    pattern = re.compile(
        r"^\s*-\s+(\S+)\s+\(usages:\s*(\d+),\s*sites:\s*(.*)\)\s*$",
        re.MULTILINE,
    )
    for m in pattern.finditer(text or ""):
        unknowns.append(
            {
                "raw_name": m.group(1),
                "usage_count": int(m.group(2)),
                "sites": [s.strip() for s in m.group(3).split(",") if s.strip()],
            }
        )
    return unknowns


def run_diff(
    *,
    source_path: Path,
    target_path: Path,
    source_vendor: str,
    target_vendor: str,
    mapping_path: Path,
    ignore_unmapped: bool,
    ignore_subnet_mask: bool,
    timeout: float = 600.0,
) -> DiffRunResult:
    cmd = [
        dhcpdiff_bin(),
        "diff",
        "--source",
        str(source_path),
        "--target",
        str(target_path),
        "--source-vendor",
        source_vendor or "auto",
        "--target-vendor",
        target_vendor or "auto",
        "--mapping",
        str(mapping_path),
        "--format",
        "json",
        "--ignore-subnet-mask",
        "true" if ignore_subnet_mask else "false",
    ]
    if ignore_unmapped:
        cmd.append("--ignore-unmapped")

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except FileNotFoundError:
        return DiffRunResult(
            ok=False,
            error=f"dhcpdiff binary not found: {dhcpdiff_bin()}",
            error_kind="binary_missing",
        )
    except subprocess.TimeoutExpired:
        return DiffRunResult(
            ok=False,
            error="dhcpdiff timed out",
            error_kind="timeout",
        )

    stdout = proc.stdout or ""
    stderr = proc.stderr or ""
    combined = (stdout + "\n" + stderr).strip()

    # Unmapped options cause bail before JSON (non-zero, often with listing on stdout)
    if "unmapped options remain" in combined.lower():
        return DiffRunResult(
            ok=False,
            error="Unmapped options remain; update the mapping YAML or enable ignore unmapped.",
            error_kind="unmapped",
            output=combined,
            unknowns=parse_unknowns_from_text(combined),
            returncode=proc.returncode,
        )

    # Diff with differences exits 1 but still prints JSON
    if proc.returncode not in (0, 1):
        return DiffRunResult(
            ok=False,
            error=combined or f"dhcpdiff exited with code {proc.returncode}",
            error_kind="cli_error",
            output=combined,
            returncode=proc.returncode,
        )

    text = stdout.strip()
    if not text:
        return DiffRunResult(
            ok=False,
            error=combined or "dhcpdiff produced no JSON output",
            error_kind="cli_error",
            output=combined,
            returncode=proc.returncode,
        )

    try:
        report = json.loads(text)
    except json.JSONDecodeError:
        # Sometimes progress or other noise — try last JSON object
        start = text.find("{")
        end = text.rfind("}")
        if start >= 0 and end > start:
            try:
                report = json.loads(text[start : end + 1])
            except json.JSONDecodeError as e:
                return DiffRunResult(
                    ok=False,
                    error=f"failed to parse JSON: {e}",
                    error_kind="parse_error",
                    output=combined,
                    returncode=proc.returncode,
                )
        else:
            return DiffRunResult(
                ok=False,
                error="failed to parse JSON from dhcpdiff stdout",
                error_kind="parse_error",
                output=combined,
                returncode=proc.returncode,
            )

    return DiffRunResult(ok=True, report=report, returncode=proc.returncode)


def load_default_mapping(repo_root: Path | None = None) -> str:
    candidates: list[Path] = []
    if repo_root:
        candidates.append(repo_root / "mappings" / "user.yaml")
    env_map = os.environ.get("DHCPDIFF_DEFAULT_MAPPING")
    if env_map:
        candidates.insert(0, Path(env_map))
    # Relative to web package: ../../mappings/user.yaml when running from repo
    here = Path(__file__).resolve()
    candidates.append(here.parents[2] / "mappings" / "user.yaml")
    candidates.append(Path("/app/mappings/user.yaml"))

    for path in candidates:
        try:
            if path.is_file():
                return path.read_text(encoding="utf-8")
        except OSError:
            continue
    return DEFAULT_MAPPING_YAML
