(() => {
  const state = {
    defaults: null,
    report: null,
    filter: "all",
    selected: 0,
    unknowns: [],
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
    // Fallback if older CLI JSON lacks display
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

  function filteredEntries() {
    const entries = state.report?.entries || [];
    if (state.filter === "all") return entries.map((e, i) => ({ e, i }));
    return entries
      .map((e, i) => ({ e, i }))
      .filter(({ e }) => e.category === state.filter);
  }

  function renderToolbar() {
    const counts = state.report?.counts || {
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
      b.addEventListener("click", () => {
        state.filter = b.dataset.f;
        const vis = filteredEntries();
        if (!vis.find(({ i }) => i === state.selected)) {
          state.selected = vis[0]?.i ?? 0;
        }
        render();
      });
    });
  }

  function formatBlock(text, hlLines) {
    const set = new Set(hlLines || []);
    return text
      .split("\n")
      .map((line, i) => {
        const body = escapeHtml(line) || " ";
        return set.has(i + 1) ? `<span class="hl">${body}</span>\n` : `${body}\n`;
      })
      .join("")
      .replace(/\n$/, "");
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

  /** Normalize side locations: nested {affected,declaration} or legacy flat LocationRef. */
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

  function splitFileLines(text) {
    if (text == null || text === "") return [];
    const normalized = text.endsWith("\n") ? text.slice(0, -1) : text;
    return normalized.split("\n");
  }

  function renderFileViewer(side, fileMeta, sideLoc) {
    const name = fileMeta?.name || side;
    const text = fileMeta?.text;
    const { affected, declaration } = sideHighlights(sideLoc);
    if (text == null) {
      return `<div class="code-panel absent">
        <header><span class="side">${escapeHtml(side)}</span><span>${escapeHtml(name)}</span></header>
        <div class="file-scroll"><div class="file-empty">(file not available)</div></div>
      </div>`;
    }
    if (!affected && !declaration) {
      return `<div class="code-panel absent">
        <header><span class="side">${escapeHtml(side)}</span><span>${escapeHtml(name)}</span></header>
        <div class="file-scroll"><div class="file-empty">(not present in this file for the selected difference)</div></div>
      </div>`;
    }
    const scrollTo =
      (declaration && (declaration.focus || declaration.start)) ||
      (affected && (affected.focus || affected.start));
    const parts = [];
    if (declaration) parts.push(`declared ${declaration.start}–${declaration.end}`);
    if (affected) parts.push(`affects ${affected.start}–${affected.end}`);
    return `<div class="code-panel">
      <header>
        <span class="side">${escapeHtml(side)}</span>
        <span>${escapeHtml(name)} · ${escapeHtml(parts.join(" · "))}</span>
      </header>
      <div class="file-scroll" data-side="${escapeHtml(side)}" data-scroll-to="${scrollTo}">
        ${renderFileLines(text, affected, declaration)}
      </div>
    </div>`;
  }

  function renderFileLines(text, affected, declaration) {
    const lines = splitFileLines(text);
    const hasAny = !!(affected || declaration);
    return lines
      .map((line, i) => {
        const n = i + 1;
        const isDecl = inRange(n, declaration);
        const isAff = inRange(n, affected);
        const isFocus =
          (declaration && declaration.focus === n) || (affected && affected.focus === n);
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
        return `<div class="${classes}" data-line="${n}"><span class="num">${n}</span><span class="code">${escapeHtml(line) || " "}</span></div>`;
      })
      .join("");
  }

  function scrollFileViewersToHighlight() {
    document.querySelectorAll(".file-scroll[data-scroll-to]").forEach((el) => {
      const line = el.dataset.scrollTo;
      const target = el.querySelector(`[data-line="${line}"]`);
      if (target) {
        target.scrollIntoView({ block: "center", behavior: "smooth" });
      }
    });
  }

  function renderList() {
    const items = filteredEntries();
    $("list").innerHTML = items
      .map(({ e, i }) => {
        const d = entityDisplay(e);
        const meta = [
          d.parent ? `<span class="meta-bit">${escapeHtml(d.parent)}</span>` : "",
          d.vci
            ? `<span class="vci-bit">Clients with VCI <strong>${escapeHtml(d.vci)}</strong></span>`
            : "",
        ]
          .filter(Boolean)
          .join("");
        return `<div class="list-item ${i === state.selected ? "selected" : ""}" data-i="${i}">
          <span class="badge ${escapeHtml(e.category)}">${escapeHtml(categoryLabel(e.category))}</span>
          <div class="entity-main">
            <span class="object-type">${escapeHtml(d.object_type)}</span>
            <span class="object-name">${escapeHtml(d.name)}</span>
          </div>
          ${meta ? `<div class="entity-meta">${meta}</div>` : ""}
        </div>`;
      })
      .join("");
    document.querySelectorAll(".list-item").forEach((el) => {
      el.addEventListener("click", () => {
        state.selected = Number(el.dataset.i);
        render();
      });
    });
  }

  function renderDetail() {
    const pane = $("detail");
    const e = state.report?.entries?.[state.selected];
    if (!e) {
      pane.className = "detail-pane empty";
      pane.textContent = "No differences in this filter";
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
    const files = state.report?.files || {};
    const hasDecl =
      e.locations?.source?.declaration || e.locations?.target?.declaration;
    const legend = hasDecl
      ? `<div class="hl-legend">
          <span class="leg-decl"><i></i> Declared here</span>
          <span class="leg-aff"><i></i> Affects this scope</span>
        </div>`
      : "";
    pane.innerHTML = `
      <div class="detail-head">
        <span class="badge ${escapeHtml(e.category)}">${escapeHtml(categoryLabel(e.category))}</span>
        <h2>${escapeHtml(d.summary)}</h2>
      </div>
      <div class="fact-grid">${facts}</div>
      <p class="detail-desc">${escapeHtml(e.detail || "")}</p>
      ${values}
      <div class="dual file-dual">
        ${renderFileViewer("Source", files.source, e.locations?.source)}
        ${renderFileViewer("Target", files.target, e.locations?.target)}
      </div>
      ${legend}
    `;
    requestAnimationFrame(() => scrollFileViewersToHighlight());
  }

  function render() {
    if (!state.report) return;
    renderToolbar();
    renderList();
    renderDetail();
    $("workspace").hidden = false;
    const n = state.report.counts?.total ?? state.report.entries?.length ?? 0;
    $("statusMeta").textContent = `${n} difference${n === 1 ? "" : "s"}`;
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

      const res = await fetch("/api/diff", { method: "POST", body: fd });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        const detail = data.detail || data;
        const msg =
          typeof detail === "string"
            ? detail
            : detail.message || detail.error || JSON.stringify(detail);
        showError(msg, detail.unknowns || []);
        $("statusMeta").textContent = "Diff failed";
        state.report = null;
        $("workspace").hidden = true;
        $("toolbar").hidden = true;
        return;
      }
      state.report = data;
      state.filter = "all";
      state.selected = 0;
      $("aliasRow").hidden = true;
      render();
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
