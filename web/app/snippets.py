"""Snippet extraction from DHCP config text using diff location spans."""

from __future__ import annotations

from typing import Any


def _lines(text: str) -> list[str]:
    # Keep trailing empty line behavior consistent with splitlines(keepends=False)
    if text.endswith("\n"):
        return text[:-1].split("\n")
    return text.split("\n") if text else []


def brace_end_line(lines: list[str], start_1based: int) -> int:
    """Find closing brace line starting from start_1based (1-based inclusive)."""
    if start_1based < 1 or start_1based > len(lines):
        return start_1based
    depth = 0
    started = False
    for i in range(start_1based - 1, len(lines)):
        for ch in lines[i]:
            if ch == "{":
                depth += 1
                started = True
            elif ch == "}":
                depth -= 1
                if started and depth == 0:
                    return i + 1
    return len(lines)


def slice_block(
    text: str,
    line: int | None,
    end_line: int | None = None,
    focus_line: int | None = None,
) -> dict[str, Any] | None:
    if line is None or line < 1:
        return None
    lines = _lines(text)
    if not lines:
        return None
    start = min(line, len(lines))
    if end_line is not None and end_line >= start:
        end = min(end_line, len(lines))
    else:
        end = brace_end_line(lines, start)

    block_lines = lines[start - 1 : end]
    highlight: list[int] = []
    if focus_line is not None and start <= focus_line <= end:
        highlight.append(focus_line - start + 1)

    return {
        "text": "\n".join(block_lines),
        "start_line": start,
        "end_line": end,
        "highlight_lines": highlight,
    }


def resolve_span(side: Any) -> tuple[int | None, int | None, int | None]:
    """Accept nested {affected,declaration} or legacy flat LocationRef."""
    if not isinstance(side, dict):
        return None, None, None
    loc = side.get("affected") if isinstance(side.get("affected"), dict) else side
    if not isinstance(loc, dict):
        return None, None, None
    return loc.get("line"), loc.get("end_line"), loc.get("focus_line")


def enrich_entries_with_snippets(
    report: dict[str, Any],
    source_text: str,
    target_text: str,
) -> dict[str, Any]:
    entries = report.get("entries") or []
    enriched = []
    for entry in entries:
        loc = entry.get("locations") or {}
        snippets: dict[str, Any] = {}
        src = loc.get("source")
        tgt = loc.get("target")
        if src:
            line, end_line, focus_line = resolve_span(src)
            snip = slice_block(source_text, line, end_line, focus_line)
            if snip:
                snippets["source"] = snip
        if tgt:
            line, end_line, focus_line = resolve_span(tgt)
            snip = slice_block(target_text, line, end_line, focus_line)
            if snip:
                snippets["target"] = snip
        item = dict(entry)
        if snippets:
            item["snippets"] = snippets
        enriched.append(item)
    out = dict(report)
    out["entries"] = enriched
    return out
