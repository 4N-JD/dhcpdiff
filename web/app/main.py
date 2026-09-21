from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from .diff_runner import DEFAULT_VENDORS, load_default_mapping
from .mapping_model import parse_user_mapping
from .jobs import (
    JobError,
    create_job_from_upload,
    get_entry,
    get_job_summary,
    list_entries,
    list_equivalence_suggestions,
    read_lines,
)

STATIC_DIR = Path(__file__).resolve().parent.parent / "static"

app = FastAPI(title="dhcpdiff web", version="0.1.0")


def _as_bool(v: str) -> bool:
    return str(v).strip().lower() in {"1", "true", "yes", "on"}


def _raise_job_error(exc: JobError) -> None:
    raise HTTPException(status_code=exc.status_code, detail=exc.detail)


@app.get("/api/health")
def health() -> dict:
    return {"status": "ok"}


@app.get("/api/defaults")
def defaults() -> dict:
    mapping_yaml = load_default_mapping()
    mapping = parse_user_mapping(mapping_yaml)
    return {
        "vendors": DEFAULT_VENDORS,
        "mapping_yaml": mapping_yaml,
        "mapping": mapping,
        "ignore_unmapped": False,
    }


@app.post("/api/mapping/parse")
async def parse_mapping(
    mapping_yaml: str = Form(""),
    file: UploadFile | None = File(None),
) -> dict:
    """Parse uploaded or pasted user.yaml text into the structured mapping model."""
    text = mapping_yaml
    if file is not None and file.filename:
        raw = await file.read()
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise HTTPException(status_code=400, detail="mapping file must be UTF-8 text") from exc
    if not (text or "").strip():
        raise HTTPException(status_code=400, detail="mapping YAML is empty")
    mapping = parse_user_mapping(text)
    return {"mapping": mapping, "mapping_yaml": text}


async def _create_job_response(
    source: UploadFile,
    target: UploadFile,
    source_vendor: str,
    target_vendor: str,
    mapping_yaml: str,
    ignore_unmapped: str,
) -> dict:
    source_bytes = await source.read()
    target_bytes = await target.read()
    if not source_bytes:
        raise HTTPException(status_code=400, detail="source file is empty")
    if not target_bytes:
        raise HTTPException(status_code=400, detail="target file is empty")

    source_name = Path(source.filename or "source.conf").name
    target_name = Path(target.filename or "target.conf").name
    mapping_text = mapping_yaml if mapping_yaml.strip() else load_default_mapping()

    try:
        summary = create_job_from_upload(
            source_bytes=source_bytes,
            target_bytes=target_bytes,
            source_name=source_name,
            target_name=target_name,
            mapping_text=mapping_text,
            source_vendor=source_vendor or "auto",
            target_vendor=target_vendor or "auto",
            ignore_unmapped=_as_bool(ignore_unmapped),
        )
    except JobError as exc:
        _raise_job_error(exc)

    return summary.as_dict()


@app.post("/api/jobs")
async def create_job(
    source: UploadFile = File(...),
    target: UploadFile = File(...),
    source_vendor: str = Form("auto"),
    target_vendor: str = Form("auto"),
    mapping_yaml: str = Form(""),
    ignore_unmapped: str = Form("false"),
) -> dict:
    return await _create_job_response(
        source,
        target,
        source_vendor,
        target_vendor,
        mapping_yaml,
        ignore_unmapped,
    )


@app.post("/api/diff")
async def diff_configs(
    source: UploadFile = File(...),
    target: UploadFile = File(...),
    source_vendor: str = Form("auto"),
    target_vendor: str = Form("auto"),
    mapping_yaml: str = Form(""),
    ignore_unmapped: str = Form("false"),
) -> dict:
    """Alias of POST /api/jobs — returns a job summary, not full file bodies."""
    return await _create_job_response(
        source,
        target,
        source_vendor,
        target_vendor,
        mapping_yaml,
        ignore_unmapped,
    )


@app.get("/api/jobs/{job_id}")
def job_meta(job_id: str) -> dict:
    try:
        return get_job_summary(job_id)
    except JobError as exc:
        _raise_job_error(exc)


@app.get("/api/jobs/{job_id}/entries")
def job_entries(
    job_id: str,
    category: str = "all",
    offset: int = 0,
    limit: int = 100,
) -> dict:
    try:
        return list_entries(job_id, category=category, offset=offset, limit=limit)
    except JobError as exc:
        _raise_job_error(exc)


@app.get("/api/jobs/{job_id}/entries/{index}")
def job_entry(job_id: str, index: int) -> dict:
    try:
        return get_entry(job_id, index)
    except JobError as exc:
        _raise_job_error(exc)


@app.get("/api/jobs/{job_id}/equivalence-suggestions")
def job_equivalence_suggestions(job_id: str, limit: int = 50) -> dict:
    try:
        return list_equivalence_suggestions(job_id, limit=limit)
    except JobError as exc:
        _raise_job_error(exc)


@app.get("/api/jobs/{job_id}/files/{side}/lines")
def job_file_lines(job_id: str, side: str, start: int = 1, end: int = 100) -> dict:
    try:
        return read_lines(job_id, side, start, end)
    except JobError as exc:
        _raise_job_error(exc)


@app.get("/")
def index() -> FileResponse:
    index_path = STATIC_DIR / "index.html"
    if not index_path.is_file():
        raise HTTPException(status_code=404, detail="UI not found")
    return FileResponse(index_path)


if STATIC_DIR.is_dir():
    app.mount("/static", StaticFiles(directory=str(STATIC_DIR)), name="static")
