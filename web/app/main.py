from __future__ import annotations

import tempfile
from pathlib import Path

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from .diff_runner import DEFAULT_VENDORS, load_default_mapping, run_diff
from .entity_display import enrich_entries_with_display
from .snippets import enrich_entries_with_snippets

STATIC_DIR = Path(__file__).resolve().parent.parent / "static"

app = FastAPI(title="dhcpdiff web", version="0.1.0")


@app.get("/api/health")
def health() -> dict:
    return {"status": "ok"}


@app.get("/api/defaults")
def defaults() -> dict:
    return {
        "vendors": DEFAULT_VENDORS,
        "mapping_yaml": load_default_mapping(),
        "ignore_unmapped": True,
        "ignore_subnet_mask": True,
    }


@app.post("/api/diff")
async def diff_configs(
    source: UploadFile = File(...),
    target: UploadFile = File(...),
    source_vendor: str = Form("auto"),
    target_vendor: str = Form("auto"),
    mapping_yaml: str = Form(""),
    ignore_unmapped: str = Form("true"),
    ignore_subnet_mask: str = Form("true"),
) -> dict:
    def as_bool(v: str) -> bool:
        return str(v).strip().lower() in {"1", "true", "yes", "on"}

    ignore_unmapped_b = as_bool(ignore_unmapped)
    ignore_subnet_mask_b = as_bool(ignore_subnet_mask)

    source_bytes = await source.read()
    target_bytes = await target.read()
    if not source_bytes:
        raise HTTPException(status_code=400, detail="source file is empty")
    if not target_bytes:
        raise HTTPException(status_code=400, detail="target file is empty")

    source_name = Path(source.filename or "source.conf").name
    target_name = Path(target.filename or "target.conf").name
    mapping_text = mapping_yaml if mapping_yaml.strip() else load_default_mapping()

    with tempfile.TemporaryDirectory(prefix="dhcpdiff-web-") as tmp:
        tmp_path = Path(tmp)
        source_path = tmp_path / source_name
        target_path = tmp_path / target_name
        mapping_path = tmp_path / "user.yaml"
        source_path.write_bytes(source_bytes)
        target_path.write_bytes(target_bytes)
        mapping_path.write_text(mapping_text, encoding="utf-8")

        result = run_diff(
            source_path=source_path,
            target_path=target_path,
            source_vendor=source_vendor,
            target_vendor=target_vendor,
            mapping_path=mapping_path,
            ignore_unmapped=ignore_unmapped_b,
            ignore_subnet_mask=ignore_subnet_mask_b,
        )

        if not result.ok:
            status = 400 if result.error_kind == "unmapped" else 500
            if result.error_kind == "binary_missing":
                status = 503
            raise HTTPException(
                status_code=status,
                detail={
                    "error": result.error_kind or "error",
                    "message": result.error,
                    "output": result.output,
                    "unknowns": result.unknowns or [],
                },
            )

        source_text = source_bytes.decode("utf-8", errors="replace")
        target_text = target_bytes.decode("utf-8", errors="replace")
        report = enrich_entries_with_snippets(result.report or {}, source_text, target_text)
        report = enrich_entries_with_display(report)
        report["files"] = {
            "source": {"name": source_name, "text": source_text},
            "target": {"name": target_name, "text": target_text},
        }
        return report


@app.get("/")
def index() -> FileResponse:
    index_path = STATIC_DIR / "index.html"
    if not index_path.is_file():
        raise HTTPException(status_code=404, detail="UI not found")
    return FileResponse(index_path)


if STATIC_DIR.is_dir():
    app.mount("/static", StaticFiles(directory=str(STATIC_DIR)), name="static")
