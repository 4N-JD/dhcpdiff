(() => {
  const LIST_ROW_H = 72;
  const LINE_H = 16;
  const LIST_PAGE = 100;
  const LINE_WINDOW = 120;
  const LINE_BUFFER = 40;

  const state = {
    defaults: null,
    jobId: null,
    counts: null,
    files: null,
    filter: "all",
    filteredTotal: 0,
    selected: null,
    selectedEntry: null,
    unknowns: [],
    listCache: new Map(), // `${filter}:${page}` -> entries[]
    lineCache: new Map(), // `${side}:${start}:${end}` -> lines
    listScrollTop: 0,
  };

  const $ = (id) => document.getElementById(id);

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function formatValue(v) {
    if (v == null) return "";
    if (typeof v === "object" && v.type != null) {
      if (v.type === "String") return JSON.stringify(v.value);
      if (v.type === "Ip" || v.type === "Int" || v.type === "Bool") return String(v.value);
      if (v.type === "IpList" && Array.isArray(v.value)) return v.value.join(", ");
      return `${v.type}(${JSON.stringify(v.value)})`;
    }
    return String(v);
  }

  function entityDisplay(e) {
    const d = e.entity?.display;
    if (d) return d;
    return {
      object_type: (e.entity?.kind || "entity").replace(/^./, (c) => c.toUpperCase()),
      name: e.entity?.key || "",
      parent: null,
      vci: null,
      summary: e.entity?.key || "",
    };
  }

  function categoryLabel(cat) {
    return String(cat || "").toUpperCase();
  }

  function fillVendors(select, vendors, selected) {
    select.innerHTML = vendors
      .map((v) => `<option value="${escapeHtml(v)}" ${v === selected ? "selected" : ""}>${escapeHtml(v)}</option>`)
      .join("");
  }

  async function loadDefaults() {
    const res = await fetch("/api/defaults");
    if (!res.ok) throw new Error("Failed to load defaults");
    state.defaults = await res.json();
    fillVendors($("sourceVendor"), state.defaults.vendors, "auto");
    fillVendors($("targetVendor"), state.defaults.vendors, "auto");
    $("mappingYaml").value = state.defaults.mapping_yaml || "";
    $("ignoreUnmapped").checked = !!state.defaults.ignore_unmapped;
    $("ignoreSubnetMask").checked = !!state.defaults.ignore_subnet_mask;
  }

  function showError(msg, unknowns) {
    const box = $("errorBox");
    box.hidden = false;
    box.textContent = msg;
    state.unknowns = unknowns || [];
    const row = $("aliasRow");
    if (state.unknowns.length) {
      row.hidden = false;
      $("unknownList").innerHTML = state.unknowns
        .map((u) => {
          const name = u.raw_name || u;
          return `<li><button type="button" class="pick-unknown" data-name="${escapeHtml(name)}">${escapeHtml(name)}</button> ${u.usage_count != null ? `(${u.usage_count})` : ""}</li>`;
        })
        .join("");
      $("mappingPanel").open = true;
      document.querySelectorAll(".pick-unknown").forEach((btn) => {
        btn.addEventListener("click", () => {
          $("aliasName").value = btn.dataset.name;
        });
      });
      if (state.unknowns[0]?.raw_name) {
        $("aliasName").value = state.unknowns[0].raw_name;
      }
    } else {
      row.hidden = true;
    }
  }

  function clearError() {
    $("errorBox").hidden = true;
    $("errorBox").textContent = "";
  }

  function listPageKey(page) {
    return `${state.filter}:${page}`;
  }

  async function fetchEntriesPage(page) {
    const key = listPageKey(page);
    if (state.listCache.has(key)) return state.listCache.get(key);
    const offset = page * LIST_PAGE;
    const res = await fetch(
      `/api/jobs/${state.jobId}/entries?category=${encodeURIComponent(state.filter)}&offset=${offset}&limit=${LIST_PAGE}`
    );
    if (!res.ok) throw new Error("Failed to load entries");
    const data = await res.json();
    state.filteredTotal = data.total;
    state.listCache.set(key, data.entries);
    // Bound cache size
    if (state.listCache.size > 24) {
      const first = state.listCache.keys().next().value;
      state.listCache.delete(first);
    }
    return data.entries;
  }

  async function ensureListRange(startIdx, endIdx) {
    const startPage = Math.floor(startIdx / LIST_PAGE);
    const endPage = Math.floor(endIdx / LIST_PAGE);
    const pages = [];
    for (let p = startPage; p <= endPage; p++) pages.push(fetchEntriesPage(p));
    await Promise.all(pages);
  }

  function cachedListItem(filteredIndex) {
    const page = Math.floor(filteredIndex / LIST_PAGE);
    const entries = state.listCache.get(listPageKey(page));
    if (!entries) return null;
    return entries[filteredIndex % LIST_PAGE] || null;
  }

  async function fetchEntry(index) {
    const res = await fetch(`/api/jobs/${state.jobId}/entries/${index}`);
    if (!res.ok) throw new Error("Failed to load entry");
    return res.json();
  }

  async function fetchLines(side, start, end) {
    const key = `${side}:${start}:${end}`;
    if (state.lineCache.has(key)) return state.lineCache.get(key);
    const res = await fetch(
      `/api/jobs/${state.jobId}/files/${side}/lines?start=${start}&end=${end}`
    );
    if (!res.ok) throw new Error(`Failed to load ${side} lines`);
    const data = await res.json();
    state.lineCache.set(key, data);
    if (state.lineCache.size > 40) {
      const first = state.lineCache.keys().next().value;
      state.lineCache.delete(first);
    }
    return data;
  }

  function locationRange(loc) {
    if (!loc || loc.line == null) return null;
    const start = Number(loc.line);
    if (!Number.isFinite(start) || start < 1) return null;
    let end = loc.end_line != null ? Number(loc.end_line) : start;
    if (!Number.isFinite(end) || end < start) end = start;
    const focus =
      loc.focus_line != null && Number.isFinite(Number(loc.focus_line))
        ? Number(loc.focus_line)
        : null;
    return { start, end, focus };
  }

  function sideHighlights(sideLoc) {
    if (!sideLoc) return { affected: null, declaration: null };
    if (sideLoc.affected || sideLoc.declaration) {
      return {
        affected: locationRange(sideLoc.affected),
        declaration: locationRange(sideLoc.declaration),
      };
    }
    return { affected: locationRange(sideLoc), declaration: null };
  }

  function inRange(n, range) {
    return !!range && n >= range.start && n <= range.end;
  }

  function renderToolbar() {
    const counts = state.counts || {
      total: 0,
      missing: 0,
      extra: 0,
      changed: 0,
      unmapped: 0,
    };
    const cats = [
      ["all", "All", counts.total ?? counts.all ?? 0],
      ["missing", "Missing", counts.missing || 0],
      ["extra", "Extra", counts.extra || 0],
      ["changed", "Changed", counts.changed || 0],
      ["unmapped", "Unmapped", counts.unmapped || 0],
    ];
    $("toolbar").innerHTML = `<div class="counts">${cats
      .map(
        ([id, label, n]) =>
          `<button type="button" class="chip ${state.filter === id ? "active" : ""}" data-f="${id}">${label}<span class="n">${n}</span></button>`
      )
      .join("")}</div>`;
    $("toolbar").hidden = false;
    document.querySelectorAll(".chip").forEach((b) => {
      b.addEventListener("click", async () => {
        if (state.filter === b.dataset.f) return;
        state.filter = b.dataset.f;
        state.listCache.clear();
        state.listScrollTop = 0;
        await bootstrapListSelection();
        renderToolbar();
        renderListShell();
        await paintList();
        await showSelectedDetail();
      });
    });
  }

  function listItemHtml(item, selected) {
    const d = entityDisplay(item);
    const meta = [
      d.parent ? `<span class="meta-bit">${escapeHtml(d.parent)}</span>` : "",
      d.vci
        ? `<span class="vci-bit">Clients with VCI <strong>${escapeHtml(d.vci)}</strong></span>`
        : "",
    ]
      .filter(Boolean)
      .join("");
    return `<div class="list-item ${selected ? "selected" : ""}" data-i="${item.index}" style="height:${LIST_ROW_H}px">
      <span class="badge ${escapeHtml(item.category)}">${escapeHtml(categoryLabel(item.category))}</span>
      <div class="entity-main">
        <span class="object-type">${escapeHtml(d.object_type)}</span>
        <span class="object-name">${escapeHtml(d.name)}</span>
      </div>
      ${meta ? `<div class="entity-meta">${meta}</div>` : ""}
    </div>`;
  }

  function renderListShell() {
    const pane = $("list");
    pane.innerHTML = `<div class="virt-spacer" id="listSpacer" style="height:${state.filteredTotal * LIST_ROW_H}px">
      <div class="virt-window" id="listWindow"></div>
    </div>`;
    pane.scrollTop = state.listScrollTop;
    pane.onscroll = () => {
      state.listScrollTop = pane.scrollTop;
      paintList();
    };
  }

  async function paintList() {
    const pane = $("list");
    const windowEl = $("listWindow");
    if (!pane || !windowEl || !state.jobId) return;

    const total = state.filteredTotal;
    const viewH = pane.clientHeight || 400;
    const first = Math.max(0, Math.floor(pane.scrollTop / LIST_ROW_H) - 5);
    const visible = Math.ceil(viewH / LIST_ROW_H) + 10;
    const last = Math.min(total - 1, first + visible);
    if (total === 0) {
      windowEl.innerHTML = `<div class="list-empty">No differences in this filter</div>`;
      return;
    }
    try {
      await ensureListRange(first, last);
    } catch (err) {
      showError(String(err));
      return;
    }

    const parts = [];
    for (let fi = first; fi <= last; fi++) {
      const item = cachedListItem(fi);
      if (!item) continue;
      parts.push(
        `<div class="virt-row" style="top:${fi * LIST_ROW_H}px">${listItemHtml(
          item,
          item.index === state.selected
        )}</div>`
      );
    }
    windowEl.innerHTML = parts.join("");
    windowEl.querySelectorAll(".list-item").forEach((el) => {
      el.addEventListener("click", async () => {
        state.selected = Number(el.dataset.i);
        await paintList();
        await showSelectedDetail();
      });
    });
  }

  async function bootstrapListSelection() {
    state.listCache.clear();
    const entries = await fetchEntriesPage(0);
    state.selected = entries[0]?.index ?? null;
    state.selectedEntry = null;
  }

  function fileMeta(side) {
    return state.files?.[side] || { name: side, line_count: 0 };
  }

  function renderDetailShell() {
    const pane = $("detail");
    const e = state.selectedEntry;
    if (!e) {
      pane.className = "detail-pane empty";
      pane.textContent = "Select a difference";
      return;
    }
    pane.className = "detail-pane";
    const d = entityDisplay(e);
    const values =
      e.values != null
        ? `<div class="values"><span>source: ${escapeHtml(formatValue(e.values.source))}</span><span class="arrow">→</span><span>target: ${escapeHtml(formatValue(e.values.target))}</span></div>`
        : "";
    const facts = [
      `<div class="fact"><span class="fact-label">Type</span><span>${escapeHtml(d.object_type)}</span></div>`,
      `<div class="fact"><span class="fact-label">Name</span><span class="mono">${escapeHtml(d.name)}</span></div>`,
      d.parent
        ? `<div class="fact"><span class="fact-label">Parent</span><span>${escapeHtml(d.parent)}</span></div>`
        : "",
      d.declared_in
        ? `<div class="fact"><span class="fact-label">Declared in</span><span>${escapeHtml(d.declared_in)}</span></div>`
        : "",
      d.vci
        ? `<div class="fact vci-fact"><span class="fact-label">Client scenario</span><span>Affects clients advertising VCI <strong>${escapeHtml(d.vci)}</strong></span></div>`
        : "",
    ].join("");
    const hasDecl =
      e.locations?.source?.declaration || e.locations?.target?.declaration;
    const legend = hasDecl
      ? `<div class="hl-legend">
          <span class="leg-decl"><i></i> Declared here</span>
          <span class="leg-aff"><i></i> Affects this scope</span>
        </div>`
      : "";
    const src = fileMeta("source");
    const tgt = fileMeta("target");
    pane.innerHTML = `
      <div class="detail-head">
        <span class="badge ${escapeHtml(e.category)}">${escapeHtml(categoryLabel(e.category))}</span>
        <h2>${escapeHtml(d.summary)}</h2>
      </div>
      <div class="fact-grid">${facts}</div>
      <p class="detail-desc">${escapeHtml(e.detail || "")}</p>
      ${values}
      <div class="dual file-dual">
        ${filePanelHtml("Source", "source", src)}
        ${filePanelHtml("Target", "target", tgt)}
      </div>
      ${legend}
    `;
  }

  function filePanelHtml(label, side, meta) {
    const hl = sideHighlights(state.selectedEntry?.locations?.[side]);
    if (!hl.affected && !hl.declaration) {
      return `<div class="code-panel absent">
        <header><span class="side">${escapeHtml(label)}</span><span>${escapeHtml(meta.name || side)}</span></header>
        <div class="file-scroll"><div class="file-empty">(not present in this file for the selected difference)</div></div>
      </div>`;
    }
    const parts = [];
    if (hl.declaration) parts.push(`declared ${hl.declaration.start}–${hl.declaration.end}`);
    if (hl.affected) parts.push(`affects ${hl.affected.start}–${hl.affected.end}`);
    const lineCount = meta.line_count || 0;
    return `<div class="code-panel">
      <header>
        <span class="side">${escapeHtml(label)}</span>
        <span>${escapeHtml(meta.name || side)} · ${escapeHtml(parts.join(" · "))}</span>
      </header>
      <div class="file-scroll virt-file" data-side="${escapeHtml(side)}" data-lines="${lineCount}">
        <div class="virt-spacer" style="height:${lineCount * LINE_H}px">
          <div class="virt-window file-window"></div>
        </div>
      </div>
    </div>`;
  }

  async function showSelectedDetail() {
    if (state.selected == null || !state.jobId) {
      state.selectedEntry = null;
      renderDetailShell();
      return;
    }
    try {
      state.selectedEntry = await fetchEntry(state.selected);
    } catch (err) {
      showError(String(err));
      return;
    }
    renderDetailShell();
    await Promise.all(["source", "target"].map((side) => setupFilePane(side, true)));
  }

  function jumpLineForSide(side) {
    const hl = sideHighlights(state.selectedEntry?.locations?.[side]);
    const decl = hl.declaration;
    const aff = hl.affected;
    return (
      (decl && (decl.focus || decl.start)) ||
      (aff && (aff.focus || aff.start)) ||
      1
    );
  }

  async function setupFilePane(side, jump) {
    const scroller = document.querySelector(`.virt-file[data-side="${side}"]`);
    if (!scroller) return;
    const lineCount = Number(scroller.dataset.lines) || 0;
    if (!lineCount) return;

    if (jump) {
      const focus = jumpLineForSide(side);
      scroller.scrollTop = Math.max(0, (focus - 1) * LINE_H - scroller.clientHeight / 3);
    }

    scroller.onscroll = () => paintFilePane(side);
    await paintFilePane(side);
  }

  async function paintFilePane(side) {
    const scroller = document.querySelector(`.virt-file[data-side="${side}"]`);
    const windowEl = scroller?.querySelector(".file-window");
    if (!scroller || !windowEl) return;

    const lineCount = Number(scroller.dataset.lines) || 0;
    if (!lineCount) return;

    const viewH = scroller.clientHeight || 300;
    let first = Math.max(1, Math.floor(scroller.scrollTop / LINE_H) + 1 - LINE_BUFFER);
    let last = Math.min(
      lineCount,
      Math.ceil((scroller.scrollTop + viewH) / LINE_H) + LINE_BUFFER
    );
    if (last - first + 1 > LINE_WINDOW) {
      const mid =
        Math.floor(scroller.scrollTop / LINE_H) + Math.floor(viewH / (LINE_H * 2)) + 1;
      first = Math.max(1, mid - Math.floor(LINE_WINDOW / 2));
      last = Math.min(lineCount, first + LINE_WINDOW - 1);
      first = Math.max(1, last - LINE_WINDOW + 1);
    }

    let data;
    try {
      data = await fetchLines(side, first, last);
    } catch (err) {
      windowEl.innerHTML = `<div class="file-empty">${escapeHtml(String(err))}</div>`;
      return;
    }

    const hl = sideHighlights(state.selectedEntry?.locations?.[side]);
    const hasAny = !!(hl.affected || hl.declaration);
    const parts = [];
    for (let i = 0; i < data.lines.length; i++) {
      const n = data.start + i;
      const line = data.lines[i];
      const isDecl = inRange(n, hl.declaration);
      const isAff = inRange(n, hl.affected);
      const isFocus =
        (hl.declaration && hl.declaration.focus === n) ||
        (hl.affected && hl.affected.focus === n);
      const dim = hasAny && !isDecl && !isAff;
      const classes = [
        "line",
        isDecl ? "hl-decl" : "",
        isAff ? "hl-affected" : "",
        isFocus ? "focus" : "",
        dim ? "dim" : "",
      ]
        .filter(Boolean)
        .join(" ");
      parts.push(
        `<div class="line-abs ${classes}" style="top:${(n - 1) * LINE_H}px;height:${LINE_H}px" data-line="${n}"><span class="num">${n}</span><span class="code">${escapeHtml(line) || " "}</span></div>`
      );
    }
    windowEl.innerHTML = parts.join("");
  }

  async function runDiff() {
    clearError();
    const source = $("sourceFile").files?.[0];
    const target = $("targetFile").files?.[0];
    if (!source || !target) {
      showError("Choose both source and target config files.");
      return;
    }
    const btn = $("runBtn");
    btn.disabled = true;
    $("statusMeta").textContent = "Running diff…";
    try {
      const fd = new FormData();
      fd.append("source", source);
      fd.append("target", target);
      fd.append("source_vendor", $("sourceVendor").value);
      fd.append("target_vendor", $("targetVendor").value);
      fd.append("mapping_yaml", $("mappingYaml").value);
      fd.append("ignore_unmapped", $("ignoreUnmapped").checked ? "true" : "false");
      fd.append("ignore_subnet_mask", $("ignoreSubnetMask").checked ? "true" : "false");

      const res = await fetch("/api/jobs", { method: "POST", body: fd });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        const detail = data.detail || data;
        const msg =
          typeof detail === "string"
            ? detail
            : detail.message || detail.error || JSON.stringify(detail);
        showError(msg, detail.unknowns || []);
        $("statusMeta").textContent = "Diff failed";
        state.jobId = null;
        $("workspace").hidden = true;
        $("toolbar").hidden = true;
        return;
      }

      state.jobId = data.job_id;
      state.counts = data.counts;
      state.files = data.files;
      state.filter = "all";
      state.listCache.clear();
      state.lineCache.clear();
      state.listScrollTop = 0;
      state.selectedEntry = null;
      $("aliasRow").hidden = true;

      await bootstrapListSelection();
      renderToolbar();
      $("workspace").hidden = false;
      renderListShell();
      await paintList();
      await showSelectedDetail();

      const n = state.counts?.total ?? state.filteredTotal ?? 0;
      $("statusMeta").textContent = `${n} difference${n === 1 ? "" : "s"}`;
    } catch (err) {
      showError(String(err));
      $("statusMeta").textContent = "Diff failed";
    } finally {
      btn.disabled = false;
    }
  }

  function addAlias() {
    const name = $("aliasName").value.trim();
    const space = $("aliasSpace").value.trim() || "dhcp";
    const code = Number($("aliasCode").value);
    if (!name || Number.isNaN(code)) {
      showError("Alias needs a name and numeric code.");
      return;
    }
    const block = `  - source_name: ${JSON.stringify(name)}\n    canonical: { space: ${JSON.stringify(space)}, code: ${code} }\n`;
    let yaml = $("mappingYaml").value;
    if (/^aliases:\s*\[\s*\]\s*$/m.test(yaml)) {
      yaml = yaml.replace(/^aliases:\s*\[\s*\]\s*$/m, `aliases:\n${block}`);
    } else if (/^aliases:\s*$/m.test(yaml)) {
      yaml = yaml.replace(/^aliases:\s*$/m, `aliases:\n${block}`);
    } else if (/^aliases:/m.test(yaml)) {
      yaml = yaml.replace(/^(aliases:\s*\n)/m, `$1${block}`);
    } else {
      yaml = `aliases:\n${block}` + yaml;
    }
    $("mappingYaml").value = yaml;
    clearError();
  }

  function downloadMapping() {
    const blob = new Blob([$("mappingYaml").value], { type: "text/yaml" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "user.yaml";
    a.click();
    URL.revokeObjectURL(a.href);
  }

  $("runBtn").addEventListener("click", runDiff);
  $("addAliasBtn").addEventListener("click", addAlias);
  $("downloadMappingBtn").addEventListener("click", downloadMapping);
  $("resetMappingBtn").addEventListener("click", () => {
    if (state.defaults) $("mappingYaml").value = state.defaults.mapping_yaml || "";
  });

  loadDefaults().catch((e) => showError(String(e)));
})();
