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
    suggestions: [],
    mapOptionOpen: false,
    suggestionsOpen: false,
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

  function peelOptionSuffix(key) {
    const text = String(key || "");
    if (!text) return null;
    const labelStart = text.lastIndexOf(" (");
    const codeRegionEnd = labelStart >= 0 ? labelStart : text.length;
    let i = codeRegionEnd;
    while (i > 0 && /\d/.test(text[i - 1])) i -= 1;
    if (i === codeRegionEnd || i === 0 || text[i - 1] !== ":") {
      const colon = text.lastIndexOf(":");
      if (colon > 0) {
        const scope = text.slice(0, colon);
        const option = text.slice(colon + 1);
        if (option.includes("(") || !option.includes(".")) return [scope, option];
      }
      return null;
    }
    const codeColon = i - 1;
    let j = codeColon;
    while (j > 0 && text[j - 1] !== ":") j -= 1;
    if (j === 0) return null;
    const spaceStart = j;
    const scope = text.slice(0, spaceStart - 1);
    const option = text.slice(spaceStart);
    if (!scope || !option) return null;
    return [scope, option];
  }

  function spaceCodeFromLabel(label) {
    let text = String(label || "").trim();
    if (!text) return null;
    const paren = text.lastIndexOf(" (");
    if (paren >= 0 && text.endsWith(")")) text = text.slice(0, paren);
    const colon = text.lastIndexOf(":");
    if (colon < 0) return null;
    const space = text.slice(0, colon).trim();
    const codeS = text.slice(colon + 1).trim();
    if (!space || !/^\d+$/.test(codeS)) return null;
    return { space, code: Number(codeS) };
  }

  function parseOptionSpaceCode(keyOrName) {
    const text = String(keyOrName || "").trim();
    if (!text) return null;
    const peeled = peelOptionSuffix(text);
    const label = peeled ? peeled[1] : text;
    return spaceCodeFromLabel(label);
  }

  function optionRefFromEntry(entry) {
    if (!entry || entry.entity?.kind !== "option") return null;
    const name = entry.entity?.display?.name;
    return parseOptionSpaceCode(name) || parseOptionSpaceCode(entry.entity?.key || "");
  }

  function formatOptionRef(ref) {
    if (!ref) return "";
    return `${ref.space}:${ref.code}`;
  }

  function equivalenceAlreadyInYaml(source, target) {
    const yaml = $("mappingYaml")?.value || "";
    if (!yaml) return false;
    // Heuristic: both space/code pairs appear near each other under equivalences.
    const srcPat = new RegExp(
      `space:\\s*["']?${escapeRegex(source.space)}["']?[\\s\\S]{0,80}?code:\\s*${source.code}`
    );
    const tgtPat = new RegExp(
      `space:\\s*["']?${escapeRegex(target.space)}["']?[\\s\\S]{0,80}?code:\\s*${target.code}`
    );
    const eqIdx = yaml.search(/^equivalences:/m);
    const slice = eqIdx >= 0 ? yaml.slice(eqIdx) : yaml;
    return srcPat.test(slice) && tgtPat.test(slice);
  }

  function escapeRegex(s) {
    return String(s).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  function suggestionForEntry(entry) {
    const ref = optionRefFromEntry(entry);
    if (!ref || !state.suggestions?.length) return null;
    const cat = entry.category;
    for (const s of state.suggestions) {
      if (cat === "missing" && s.source.space === ref.space && s.source.code === ref.code) {
        return s;
      }
      if (cat === "extra" && s.target.space === ref.space && s.target.code === ref.code) {
        return s;
      }
    }
    return null;
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
        state.mapOptionOpen = false;
        state.suggestionsOpen = false;
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
        const next = Number(el.dataset.i);
        if (next !== state.selected) {
          state.mapOptionOpen = false;
          state.suggestionsOpen = false;
        }
        state.selected = next;
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
    const mapUi = mapOptionUiHtml(e);
    pane.innerHTML = `
      <div class="detail-head">
        <span class="badge ${escapeHtml(e.category)}">${escapeHtml(categoryLabel(e.category))}</span>
        <h2>${escapeHtml(d.summary)}</h2>
      </div>
      <div class="fact-grid">${facts}</div>
      <p class="detail-desc">${escapeHtml(e.detail || "")}</p>
      ${values}
      ${mapUi}
      <div class="dual file-dual">
        ${filePanelHtml("Source", "source", src)}
        ${filePanelHtml("Target", "target", tgt)}
      </div>
      ${legend}
    `;
    bindMapOptionUi(e);
  }

  function canMapOption(entry) {
    return (
      (entry.category === "missing" || entry.category === "extra") &&
      entry.entity?.kind === "option" &&
      !!optionRefFromEntry(entry)
    );
  }

  function mapOptionUiHtml(entry) {
    if (!canMapOption(entry)) return "";
    const open = state.mapOptionOpen;
    const toggleLabel = open ? "Hide mapping" : "Map Option";
    let body = "";
    if (open) {
      body = equivalenceFormHtml(entry);
    }
    return `<div class="map-option" id="mapOptionUi">
      <button type="button" class="map-option-toggle" id="mapOptionBtn">${escapeHtml(toggleLabel)}</button>
      ${body}
    </div>`;
  }

  function equivalenceFormHtml(entry) {
    const ref = optionRefFromEntry(entry);
    if (!ref) return "";
    const match = state.suggestionsOpen ? suggestionForEntry(entry) : null;
    let srcSpace = "";
    let srcCode = "";
    let tgtSpace = "";
    let tgtCode = "";
    if (entry.category === "missing") {
      srcSpace = ref.space;
      srcCode = String(ref.code);
      if (match) {
        tgtSpace = match.target.space;
        tgtCode = String(match.target.code);
      }
    } else {
      tgtSpace = ref.space;
      tgtCode = String(ref.code);
      if (match) {
        srcSpace = match.source.space;
        srcCode = String(match.source.code);
      }
    }
    const suggestionsBlock = state.suggestionsOpen
      ? `<div class="equiv-suggestions" id="equivSuggestionsInForm"></div>`
      : "";
    return `<div class="equiv-form" id="equivForm">
      <div class="equiv-form-title">Insert equivalence</div>
      <div class="equiv-form-grid">
        <div>
          <div class="side-label">Source</div>
          <div class="equiv-side">
            <input type="text" id="equivSrcSpace" placeholder="space" value="${escapeHtml(srcSpace)}" />
            <input type="number" id="equivSrcCode" placeholder="code" min="0" value="${escapeHtml(srcCode)}" />
          </div>
        </div>
        <div>
          <div class="side-label">Target</div>
          <div class="equiv-side">
            <input type="text" id="equivTgtSpace" placeholder="space" value="${escapeHtml(tgtSpace)}" />
            <input type="number" id="equivTgtCode" placeholder="code" min="0" value="${escapeHtml(tgtCode)}" />
          </div>
        </div>
        <button type="button" id="addEquivalenceBtn">Add equivalence</button>
      </div>
      <div class="equiv-form-actions">
        <button type="button" id="suggestEquivalencesBtn">${
          state.suggestionsOpen ? "Hide suggestions" : "Suggest equivalences"
        }</button>
      </div>
      ${suggestionsBlock}
      <p class="equiv-form-hint">Adds a confirmed mapping to the YAML above; re-run the diff to apply.</p>
    </div>`;
  }

  function bindMapOptionUi(entry) {
    const toggle = $("mapOptionBtn");
    if (toggle) {
      toggle.addEventListener("click", () => {
        state.mapOptionOpen = !state.mapOptionOpen;
        if (!state.mapOptionOpen) {
          state.suggestionsOpen = false;
        }
        renderDetailShell();
        Promise.all(["source", "target"].map((side) => setupFilePane(side, false)));
      });
    }
    bindEquivalenceForm(entry);
  }

  function bindEquivalenceForm(entry) {
    const addBtn = $("addEquivalenceBtn");
    if (addBtn) {
      addBtn.addEventListener("click", () => {
        const source = {
          space: $("equivSrcSpace").value.trim(),
          code: Number($("equivSrcCode").value),
        };
        const target = {
          space: $("equivTgtSpace").value.trim(),
          code: Number($("equivTgtCode").value),
        };
        if (!insertEquivalence(source, target)) return;
        $("statusMeta").textContent = "Equivalence added — re-run diff to apply";
        if (state.suggestionsOpen) {
          renderEquivalenceSuggestionsInForm();
        }
      });
    }
    const suggestBtn = $("suggestEquivalencesBtn");
    if (suggestBtn) {
      suggestBtn.addEventListener("click", async () => {
        if (state.suggestionsOpen) {
          state.suggestionsOpen = false;
          renderDetailShell();
          await Promise.all(["source", "target"].map((side) => setupFilePane(side, false)));
          return;
        }
        suggestBtn.disabled = true;
        suggestBtn.textContent = "Loading…";
        await fetchEquivalenceSuggestions();
        state.suggestionsOpen = true;
        renderDetailShell();
        await Promise.all(["source", "target"].map((side) => setupFilePane(side, false)));
        renderEquivalenceSuggestionsInForm();
      });
    }
    if (state.suggestionsOpen) {
      renderEquivalenceSuggestionsInForm();
    }
  }

  function renderEquivalenceSuggestionsInForm() {
    const box = $("equivSuggestionsInForm");
    if (!box) return;
    const pending = (state.suggestions || []).filter(
      (s) => !equivalenceAlreadyInYaml(s.source, s.target)
    );
    if (!pending.length) {
      box.innerHTML = `<p class="equiv-form-hint">No suggested pairs for this diff (same scope + value on missing/extra).</p>`;
      return;
    }
    box.innerHTML = `
      <div class="equiv-suggestions-head">
        <strong>Suggested equivalences</strong>
        <span class="hint">Same scope + value on missing/extra options</span>
      </div>
      <div class="equiv-suggestion-list">
        ${pending
          .map((s, i) => {
            const label = `${formatOptionRef(s.source)} ↔ ${formatOptionRef(s.target)}`;
            const count =
              s.count > 1 ? `<span class="count">(${s.count} scopes)</span>` : "";
            return `<div class="equiv-suggestion-row">
              <span class="pair">${escapeHtml(label)}</span>
              ${count}
              <button type="button" class="add-suggestion" data-i="${i}">Add</button>
            </div>`;
          })
          .join("")}
      </div>`;
    box.querySelectorAll(".add-suggestion").forEach((btn) => {
      btn.addEventListener("click", () => {
        const s = pending[Number(btn.dataset.i)];
        if (!s) return;
        if (!insertEquivalence(s.source, s.target)) return;
        $("statusMeta").textContent = "Equivalence added — re-run diff to apply";
        // Prefill form fields from the added pair
        if ($("equivSrcSpace")) {
          $("equivSrcSpace").value = s.source.space;
          $("equivSrcCode").value = String(s.source.code);
          $("equivTgtSpace").value = s.target.space;
          $("equivTgtCode").value = String(s.target.code);
        }
        renderEquivalenceSuggestionsInForm();
      });
    });
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
        state.suggestions = [];
        state.mapOptionOpen = false;
        state.suggestionsOpen = false;
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
      state.suggestions = [];
      state.mapOptionOpen = false;
      state.suggestionsOpen = false;
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
    const block = `- source_name: ${JSON.stringify(name)}\n  canonical: { space: ${JSON.stringify(space)}, code: ${code} }\n`;
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

  function insertEquivalence(source, target) {
    if (
      !source?.space ||
      !target?.space ||
      Number.isNaN(source.code) ||
      Number.isNaN(target.code)
    ) {
      showError("Equivalence needs source and target space + numeric code.");
      return false;
    }
    if (source.space === target.space && source.code === target.code) {
      showError("Source and target must differ.");
      return false;
    }
    if (equivalenceAlreadyInYaml(source, target)) {
      showError("That equivalence is already in the mapping YAML.");
      return false;
    }
    const block =
      `- source: { space: ${JSON.stringify(source.space)}, code: ${source.code} }\n` +
      `  target: { space: ${JSON.stringify(target.space)}, code: ${target.code} }\n` +
      `  confirmed: true\n`;
    let yaml = $("mappingYaml").value;
    if (/^equivalences:\s*\[\s*\]\s*$/m.test(yaml)) {
      yaml = yaml.replace(/^equivalences:\s*\[\s*\]\s*$/m, `equivalences:\n${block}`);
    } else if (/^equivalences:\s*$/m.test(yaml)) {
      yaml = yaml.replace(/^equivalences:\s*$/m, `equivalences:\n${block}`);
    } else if (/^equivalences:/m.test(yaml)) {
      yaml = yaml.replace(/^(equivalences:\s*\n)/m, `$1${block}`);
    } else {
      yaml = `equivalences:\n${block}` + yaml;
    }
    $("mappingYaml").value = yaml;
    const panel = $("mappingPanel");
    if (panel) panel.open = true;
    clearError();
    return true;
  }

  async function fetchEquivalenceSuggestions() {
    if (!state.jobId) {
      state.suggestions = [];
      return;
    }
    try {
      const res = await fetch(`/api/jobs/${state.jobId}/equivalence-suggestions`);
      if (!res.ok) {
        state.suggestions = [];
        return;
      }
      const data = await res.json();
      state.suggestions = data.suggestions || [];
    } catch {
      state.suggestions = [];
    }
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
