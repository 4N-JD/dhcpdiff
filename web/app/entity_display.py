"""Parse opaque entity keys into human-readable display fields (fallback if CLI omits them)."""

from __future__ import annotations

from typing import Any


def build_entity_display(kind: str, key: str) -> dict[str, Any]:
    if kind == "subnet":
        return {
            "object_type": "Subnet",
            "name": key,
            "parent": None,
            "parent_key": None,
            "vci": None,
            "declared_in": None,
            "declaration_key": None,
            "option_id": None,
            "summary": f"Subnet {key}",
        }
    if kind == "pool":
        if ":" in key:
            cidr, range = key.split(":", 1)
            return {
                "object_type": "Pool",
                "name": range,
                "parent": f"Subnet {cidr}",
                "parent_key": cidr,
                "vci": None,
                "declared_in": None,
                "declaration_key": None,
                "option_id": None,
                "summary": f"Pool {range} in subnet {cidr}",
            }
        return {
            "object_type": "Pool",
            "name": key,
            "parent": None,
            "parent_key": None,
            "vci": None,
            "declared_in": None,
            "declaration_key": None,
            "option_id": None,
            "summary": f"Pool {key}",
        }
    if kind == "reservation":
        return {
            "object_type": "Reservation",
            "name": key,
            "parent": None,
            "parent_key": None,
            "vci": None,
            "declared_in": None,
            "declaration_key": None,
            "option_id": None,
            "summary": f"Reservation {key}",
        }
    if kind == "filter":
        parts = key.split(":", 2)
        if len(parts) >= 2:
            scope, name = parts[0], parts[1]
            parent = "Global" if scope == "global" else f"Subnet {scope}"
            return {
                "object_type": "Filter",
                "name": name,
                "parent": parent,
                "parent_key": scope,
                "vci": None,
                "declared_in": None,
                "declaration_key": None,
                "option_id": None,
                "summary": f"Filter {name} ({parent})",
            }
        return {
            "object_type": "Filter",
            "name": key,
            "parent": None,
            "parent_key": None,
            "vci": None,
            "declared_in": None,
            "declaration_key": None,
            "option_id": None,
            "summary": f"Filter {key}",
        }
    if kind == "option":
        return _parse_option_display(key)
    return {
        "object_type": kind[:1].upper() + kind[1:] if kind else "Entity",
        "name": key,
        "parent": None,
        "parent_key": None,
        "vci": None,
        "declared_in": None,
        "declaration_key": None,
        "option_id": None,
        "summary": f"{kind}:{key}",
    }


def _option_id_from_label(option_name: str) -> str | None:
    text = (option_name or "").strip()
    if not text:
        return None
    if text.endswith(")") and " (" in text:
        text = text[: text.rfind(" (")]
    text = text.strip()
    return text if ":" in text else None


def _affected_as_declaration_key(scope: str) -> str:
    base = (scope or "").split(":vci=", 1)[0]
    return base if base else "global"


def _parse_option_display(key: str) -> dict[str, Any]:
    scope, option_name, vci = _split_option_key(key)
    object_type, name, parent, parent_key, summary_base = _describe_option_scope(
        scope, option_name
    )
    summary = (
        f"{summary_base} — affects clients with VCI {vci}" if vci else summary_base
    )
    return {
        "object_type": object_type,
        "name": name,
        "parent": parent,
        "parent_key": parent_key,
        "vci": vci,
        "declared_in": None,
        "declaration_key": _affected_as_declaration_key(scope),
        "option_id": _option_id_from_label(option_name),
        "summary": summary,
    }


def _split_option_key(key: str) -> tuple[str, str, str | None]:
    marker = ":vci="
    idx = key.find(marker)
    if idx >= 0:
        before = key[:idx]
        after = key[idx + len(marker) :]
        # Peel option suffix from the right so VCIs that contain colons
        # (e.g. PXEClient:Arch:00000) stay intact.
        peeled = _peel_option_suffix(after)
        if peeled:
            vci, option = peeled
            return before, option, vci
        return before, "", after

    peeled = _peel_option_suffix(key)
    if peeled:
        return peeled[0], peeled[1], None
    return "", key, None


def _peel_option_suffix(key: str) -> tuple[str, str] | None:
    label_start = key.rfind(" (")
    code_region_end = label_start if label_start >= 0 else len(key)
    i = code_region_end
    while i > 0 and key[i - 1].isdigit():
        i -= 1
    if i == code_region_end or i == 0 or key[i - 1] != ":":
        if ":" in key:
            scope, option = key.rsplit(":", 1)
            if "(" in option or "." not in option:
                return scope, option
        return None
    code_colon = i - 1
    j = code_colon
    while j > 0 and key[j - 1] != ":":
        j -= 1
    if j == 0:
        return None
    space_start = j
    scope = key[: space_start - 1]
    option = key[space_start:]
    if not scope or not option:
        return None
    return scope, option


def parse_option_space_code(key_or_name: str) -> dict[str, Any] | None:
    """Extract {space, code} from an option label or full entity key."""
    text = (key_or_name or "").strip()
    if not text:
        return None
    # Prefer peeling from a full key (scope:space:code …); fall back to label alone.
    peeled = _peel_option_suffix(text)
    label = peeled[1] if peeled else text
    return _space_code_from_label(label)


def parse_option_scope_and_ref(key: str) -> dict[str, Any] | None:
    """Return {scope, space, code, label} for an option entity key, or None."""
    text = (key or "").strip()
    if not text:
        return None
    scope, option_name, _vci = _split_option_key(text)
    ref = _space_code_from_label(option_name)
    if not ref:
        return None
    return {
        "scope": scope or "global",
        "space": ref["space"],
        "code": ref["code"],
        "label": option_name,
    }


def _space_code_from_label(label: str) -> dict[str, Any] | None:
    """Parse `space:code` or `space:code (name)` into {space, code}."""
    text = (label or "").strip()
    if not text:
        return None
    without_name = text
    paren = without_name.rfind(" (")
    if paren >= 0 and without_name.endswith(")"):
        without_name = without_name[:paren]
    if ":" not in without_name:
        return None
    space, code_s = without_name.rsplit(":", 1)
    space = space.strip()
    code_s = code_s.strip()
    if not space or not code_s.isdigit():
        return None
    return {"space": space, "code": int(code_s)}


def parse_option_detail_value(detail: str) -> str | None:
    """Extract the value portion from missing/extra option detail strings."""
    text = (detail or "").strip()
    if not text:
        return None
    for suffix in (" missing in target", " extra in target"):
        if text.endswith(suffix):
            head = text[: -len(suffix)]
            eq = head.find(" = ")
            if eq < 0:
                return None
            return head[eq + 3 :]
    return None


def _describe_option_scope(
    scope: str, option_name: str
) -> tuple[str, str, str | None, str | None, str]:
    name = option_name or "option"
    if not scope or scope == "global":
        return "Option", name, "Global", "global", f"Option {name} (global)"
    if scope.startswith("pool:"):
        rest = scope[len("pool:") :]
        if ":" in rest:
            cidr, range = rest.split(":", 1)
            parent = f"Pool {range} in subnet {cidr}"
            return "Option", name, parent, scope, f"Option {name} on {parent}"
        parent = f"Pool {rest}"
        return "Option", name, parent, scope, f"Option {name} on {parent}"
    if scope.startswith("reservation:"):
        ip = scope[len("reservation:") :]
        parent = f"Reservation {ip}"
        return "Option", name, parent, scope, f"Option {name} on {parent}"
    return "Option", name, scope, scope, f"Option {name} ({scope})"


def enrich_entries_with_display(report: dict[str, Any]) -> dict[str, Any]:
    entries = report.get("entries") or []
    out_entries = []
    for entry in entries:
        item = dict(entry)
        entity = dict(item.get("entity") or {})
        if not entity.get("display"):
            kind = entity.get("kind") or ""
            key = entity.get("key") or ""
            entity["display"] = build_entity_display(kind, key)
            item["entity"] = entity
        out_entries.append(item)
    out = dict(report)
    out["entries"] = out_entries
    return out
