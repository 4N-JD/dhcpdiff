"""Parse user mapping YAML into a structured dict for the web UI."""

from __future__ import annotations

from typing import Any

import yaml

EMPTY_MAPPING: dict[str, Any] = {
    "aliases": [],
    "equivalences": [],
    "ignore": [],
}

DHCP_SUBNET_MASK = {"space": "dhcp", "code": 1}


def _as_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.strip().isdigit():
        return int(value.strip())
    return None


def _option_key(raw: Any) -> dict[str, Any] | None:
    if not isinstance(raw, dict):
        return None
    space = raw.get("space")
    code = _as_int(raw.get("code"))
    if not isinstance(space, str) or not space.strip() or code is None:
        return None
    return {"space": space.strip(), "code": code}


def _has_ignore(ignore: list[dict[str, Any]], space: str, code: int) -> bool:
    return any(i.get("space") == space and i.get("code") == code for i in ignore)


def parse_user_mapping(text: str) -> dict[str, Any]:
    """Parse user.yaml text into {aliases, equivalences, ignore}.

    Legacy ``ignore_subnet_mask: true`` is migrated into ``ignore`` as dhcp:1.
    Invalid or empty YAML yields empty lists.
    """
    out: dict[str, Any] = {
        "aliases": [],
        "equivalences": [],
        "ignore": [],
    }
    raw_text = (text or "").strip()
    if not raw_text:
        return out

    try:
        data = yaml.safe_load(raw_text)
    except yaml.YAMLError:
        return out

    if not isinstance(data, dict):
        return out

    for item in data.get("aliases") or []:
        if not isinstance(item, dict):
            continue
        name = item.get("source_name")
        canonical = _option_key(item.get("canonical"))
        if not isinstance(name, str) or not name.strip() or not canonical:
            continue
        entry: dict[str, Any] = {
            "source_name": name.strip(),
            "canonical": canonical,
        }
        note = item.get("note")
        if isinstance(note, str) and note.strip():
            entry["note"] = note.strip()
        out["aliases"].append(entry)

    for item in data.get("equivalences") or []:
        if not isinstance(item, dict):
            continue
        source = _option_key(item.get("source"))
        target = _option_key(item.get("target"))
        if not source or not target:
            continue
        out["equivalences"].append(
            {
                "source": source,
                "target": target,
                "confirmed": bool(item.get("confirmed", False)),
            }
        )

    for item in data.get("ignore") or []:
        key = _option_key(item)
        if key:
            out["ignore"].append(key)

    # Legacy flag → normal ignore entry
    if data.get("ignore_subnet_mask") is True and not _has_ignore(
        out["ignore"], "dhcp", 1
    ):
        out["ignore"].append(dict(DHCP_SUBNET_MASK))

    return out
