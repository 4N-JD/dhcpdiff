(() => {
  const LIST_ROW_H = 72;
  const LINE_H = 16;
  const LIST_PAGE = 100;
  const LINE_WINDOW = 120;
  const LINE_BUFFER = 40;

  const state = {
    defaults: null,
    mapping: emptyMapping(),
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
    hide: emptyHide(),
    hiddenCount: 0,
    visibleCounts: null, // hide-adjusted category totals; null = use state.counts
    listCache: new Map(), // `${filter}:${page}` -> entries[]
    lineCache: new Map(), // `${side}:${start}:${end}` -> lines
    listScrollTop: 0,
    needsRerun: false,
    lastRunMappingYaml: null,
    lastRunIgnoreUnmapped: null,
  };

  const SESSION_KEY = "dhcpdiff-session";

  const $ = (id) => document.getElementById(id);

  function emptyHide() {
    return { entries: [], parents: [] };
  }

  function hideStorageKey(jobId) {
    return `dhcpdiff-hide-${jobId}`;
  }

  function loadHideForJob(jobId) {
    if (!jobId) return emptyHide();
    const key = hideStorageKey(jobId);
    try {
      let raw = localStorage.getItem(key);
      if (!raw) {
        raw = sessionStorage.getItem(key);
        if (raw) {
          localStorage.setItem(key, raw);
          sessionStorage.removeItem(key);
        }
      }
      if (!raw) return emptyHide();
      return normalizeHide(JSON.parse(raw));
    } catch {
      return emptyHide();
    }
  }

  function normalizeHide(raw) {
    const out = emptyHide();
    if (!raw || typeof raw !== "object") return out;
    if (Array.isArray(raw.entries)) {
      out.entries = raw.entries
        .filter((e) => e && e.kind != null && e.key != null)
        .map((e) => ({ kind: String(e.kind), key: String(e.key) }));
    }
    if (Array.isArray(raw.parents)) {
      out.parents = raw.parents
        .filter((p) => p && p.kind && p.parent_key != null && p.parent_key !== "")
        .map((p) => {
          const rule = { kind: String(p.kind), parent_key: String(p.parent_key) };
          if (p.option_id != null && p.option_id !== "") {
            rule.option_id = String(p.option_id);
          }
          return rule;
        });
    }
    return out;
  }

  function persistHide() {
    if (!state.jobId) return;
    const key = hideStorageKey(state.jobId);
    sessionStorage.removeItem(key);
    if (!state.hide.entries.length && !state.hide.parents.length) {
      localStorage.removeItem(key);
      return;
    }
    localStorage.setItem(key, JSON.stringify(state.hide));
  }

  function loadSession() {
    try {
      const raw = localStorage.getItem(SESSION_KEY);
      if (!raw) return null;
      const data = JSON.parse(raw);
      if (!data || typeof data !== "object") return null;
      return {
        jobId: data.jobId ? String(data.jobId) : null,
        sourceVendor: data.sourceVendor ? String(data.sourceVendor) : "auto",
        targetVendor: data.targetVendor ? String(data.targetVendor) : "auto",
        ignoreUnmapped: !!data.ignoreUnmapped,
        mapping: normalizeMapping(data.mapping),
        lastRunMappingYaml:
          typeof data.lastRunMappingYaml === "string" ? data.lastRunMappingYaml : null,
        lastRunIgnoreUnmapped:
          typeof data.lastRunIgnoreUnmapped === "boolean"
            ? data.lastRunIgnoreUnmapped
            : null,
      };
    } catch {
      return null;
    }
  }

  function saveSession() {
    try {
      const payload = {
        jobId: state.jobId,
        sourceVendor: $("sourceVendor")?.value || "auto",
        targetVendor: $("targetVendor")?.value || "auto",
        ignoreUnmapped: !!$("ignoreUnmapped")?.checked,
        mapping: state.mapping,
        lastRunMappingYaml: state.lastRunMappingYaml,
        lastRunIgnoreUnmapped: state.lastRunIgnoreUnmapped,
      };
      localStorage.setItem(SESSION_KEY, JSON.stringify(payload));
    } catch {
      /* quota / private mode */
    }
  }

  function hideQueryParam() {
    if (!state.hide.entries.length && !state.hide.parents.length) return "";
    return `&hide=${encodeURIComponent(JSON.stringify(state.hide))}`;
  }

  function hasActiveHides() {
    return state.hide.entries.length > 0 || state.hide.parents.length > 0;
  }

  async function refreshVisibleCounts() {
    if (!state.jobId || !hasActiveHides()) {
      state.visibleCounts = null;
      return;
    }
    const cats = ["all", "missing", "extra", "changed", "unmapped"];
    try {
      const results = await Promise.all(
        cats.map(async (cat) => {
          const res = await fetch(
            `/api/jobs/${state.jobId}/entries?category=${encodeURIComponent(cat)}&offset=0&limit=1${hideQueryParam()}`
          );
          if (!res.ok) throw new Error("visible count failed");
          const data = await res.json();
          return [cat, data.total ?? 0];
        })
      );
      const visible = { total: 0, missing: 0, extra: 0, changed: 0, unmapped: 0 };
      for (const [cat, n] of results) {
        if (cat === "all") visible.total = n;
        else visible[cat] = n;
      }
      state.visibleCounts = visible;
    } catch {
      state.visibleCounts = null;
    }
  }

  async function refreshHiddenMeta() {
    const box = $("hiddenDiffsMeta");
    const resetBtn = $("resetHiddenBtn");
    if (!box || !resetBtn) return;
    await refreshVisibleCounts();
    if (!state.jobId || !hasActiveHides()) {
      state.hiddenCount = 0;
      box.hidden = true;
      resetBtn.hidden = true;
      box.textContent = "";
      return;
    }
    if (state.visibleCounts) {
      const total = state.counts?.total ?? 0;
      state.hiddenCount = Math.max(0, total - (state.visibleCounts.total ?? 0));
    } else {
      state.hiddenCount = state.hide.entries.length + state.hide.parents.length;
    }
    const n = state.hiddenCount;
    box.textContent = `${n} diff${n === 1 ? "" : "s"} hidden`;
    box.hidden = n === 0;
    resetBtn.hidden = n === 0;
  }

  async function applyHideAndRefresh() {
    persistHide();
    state.listCache.clear();
    state.listScrollTop = 0;
    state.selectedEntry = null;
    await bootstrapListSelection();
    await refreshHiddenMeta();
    renderToolbar();
    renderListShell();
    await paintList();
    await showSelectedDetail();
  }

  function ignoreSelectedEntry() {
    const e = state.selectedEntry;
    if (!e?.entity) return;
    const kind = e.entity.kind;
    const key = e.entity.key;
    if (!state.hide.entries.some((x) => x.kind === kind && x.key === key)) {
      state.hide.entries.push({ kind, key });
    }
    applyHideAndRefresh().catch((err) => showError(String(err)));
  }

  function ignoreSelectedParent() {
    const e = state.selectedEntry;
    const kind = e?.entity?.kind;
    if (!kind) return;
    const d = e.entity.display || {};
    if (kind === "option") {
      const decl = d.declaration_key;
      const oid = d.option_id;
      if (decl == null || decl === "" || oid == null || oid === "") return;
      const exists = state.hide.parents.some(
        (p) =>
          p.kind === "option" &&
          p.parent_key === String(decl) &&
          p.option_id === String(oid)
      );
      if (!exists) {
        state.hide.parents.push({
          kind: "option",
          parent_key: String(decl),
          option_id: String(oid),
        });
      }
    } else {
      const pk = d.parent_key;
      if (pk == null || pk === "") return;
      if (!state.hide.parents.some((p) => p.kind === kind && p.parent_key === String(pk) && !p.option_id)) {
        state.hide.parents.push({ kind, parent_key: String(pk) });
      }
    }
    applyHideAndRefresh().catch((err) => showError(String(err)));
  }

  async function resetHiddenDiffs() {
    if (!state.jobId) return;
    state.hide = emptyHide();
    persistHide();
    state.listCache.clear();
    state.listScrollTop = 0;
    await bootstrapListSelection();
    await refreshHiddenMeta();
    renderToolbar();
    renderListShell();
    await paintList();
    await showSelectedDetail();
  }

  function updateRerunBanner() {
    const el = $("rerunBanner");
    if (!el) return;
    el.hidden = !state.needsRerun;
  }

  function markNeedsRerun() {
    if (!state.jobId) return;
    state.needsRerun = true;
    updateRerunBanner();
  }

  function clearNeedsRerun() {
    state.needsRerun = false;
    updateRerunBanner();
  }

  /** Persist mapping/prefs and flag stale results when a job is active. */
  function onMappingOrPrefsChanged() {
    saveSession();
    markNeedsRerun();
  }

  function mappingIsStaleVsLastRun() {
    if (state.lastRunMappingYaml == null && state.lastRunIgnoreUnmapped == null) {
      return false;
    }
    const mappingStale =
      state.lastRunMappingYaml != null &&
      serializeMappingYaml(state.mapping) !== state.lastRunMappingYaml;
    const ignoreStale =
      state.lastRunIgnoreUnmapped != null &&
      !!$("ignoreUnmapped")?.checked !== state.lastRunIgnoreUnmapped;
    return mappingStale || ignoreStale;
  }

  function emptyMapping() {
    return {
      aliases: [],
      equivalences: [],
      ignore: [],
    };
  }

  function normalizeMapping(raw) {
    const m = emptyMapping();
    if (!raw || typeof raw !== "object") return m;
    m.aliases = Array.isArray(raw.aliases)
      ? raw.aliases
          .filter((a) => a && a.source_name && a.canonical?.space != null && a.canonical?.code != null)
          .map((a) => ({
            source_name: String(a.source_name),
            canonical: { space: String(a.canonical.space), code: Number(a.canonical.code) },
            ...(a.note ? { note: String(a.note) } : {}),
          }))
      : [];
    m.equivalences = Array.isArray(raw.equivalences)
      ? raw.equivalences
          .filter((e) => e?.source?.space != null && e?.target?.space != null)
          .map((e) => ({
            source: { space: String(e.source.space), code: Number(e.source.code) },
            target: { space: String(e.target.space), code: Number(e.target.code) },
            confirmed: !!e.confirmed,
          }))
      : [];
    m.ignore = Array.isArray(raw.ignore)
      ? raw.ignore
          .filter((i) => i?.space != null && i?.code != null)
          .map((i) => ({ space: String(i.space), code: Number(i.code) }))
      : [];
    // Legacy flag from older defaults payloads
    if (raw.ignore_subnet_mask === true) {
      const has = m.ignore.some((i) => i.space === "dhcp" && i.code === 1);
      if (!has) m.ignore.push({ space: "dhcp", code: 1 });
    }
    return m;
  }

  function yamlQuote(s) {
    return JSON.stringify(String(s));
  }

  function serializeMappingYaml(mapping) {
    const m = normalizeMapping(mapping);
    const out = [];
    if (!m.aliases.length) {
      out.push("aliases: []");
    } else {
      out.push("aliases:");
      for (const a of m.aliases) {
        out.push(`- source_name: ${yamlQuote(a.source_name)}`);
        out.push(
          `  canonical: { space: ${yamlQuote(a.canonical.space)}, code: ${a.canonical.code} }`
        );
        if (a.note) out.push(`  note: ${yamlQuote(a.note)}`);
      }
    }
    if (!m.equivalences.length) {
      out.push("equivalences: []");
    } else {
      out.push("equivalences:");
      for (const e of m.equivalences) {
        out.push(
          `- source: { space: ${yamlQuote(e.source.space)}, code: ${e.source.code} }`
        );
        out.push(
          `  target: { space: ${yamlQuote(e.target.space)}, code: ${e.target.code} }`
        );
        out.push(`  confirmed: ${e.confirmed ? "true" : "false"}`);
      }
    }
    if (!m.ignore.length) {
      out.push("ignore: []");
    } else {
      out.push("ignore:");
      for (const i of m.ignore) {
        out.push(`- space: ${yamlQuote(i.space)}`);
        out.push(`  code: ${i.code}`);
      }
    }
    return out.join("\n") + "\n";
  }

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
      parent_key: null,
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

  function equivalenceAlreadyMapped(source, target) {
    return (state.mapping.equivalences || []).some(
      (e) =>
        e.source.space === source.space &&
        e.source.code === source.code &&
        e.target.space === target.space &&
        e.target.code === target.code
    );
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
    applyMapping(state.defaults.mapping || emptyMapping());
    $("ignoreUnmapped").checked = !!state.defaults.ignore_unmapped;
  }

  async function openJobWorkspace(data, { restored = false, carryHide = null } = {}) {
    state.jobId = data.job_id;
    state.counts = data.counts;
    state.files = data.files;
    state.filter = "all";
    if (carryHide != null) {
      state.hide = normalizeHide(carryHide);
      persistHide();
    } else {
      state.hide = loadHideForJob(state.jobId);
    }
    state.listCache.clear();
    state.lineCache.clear();
    state.listScrollTop = 0;
    state.selectedEntry = null;
    state.suggestions = [];
    state.mapOptionOpen = false;
    state.suggestionsOpen = false;
    $("aliasRow").hidden = true;

    await bootstrapListSelection();
    await refreshHiddenMeta();
    renderToolbar();
    $("workspace").hidden = false;
    renderListShell();
    await paintList();
    await showSelectedDetail();

    const n = state.visibleCounts?.total ?? state.counts?.total ?? state.filteredTotal ?? 0;
    $("statusMeta").textContent = restored
      ? `${n} difference${n === 1 ? "" : "s"} (restored)`
      : `${n} difference${n === 1 ? "" : "s"}`;
  }

  async function restoreJob(jobId) {
    const res = await fetch(`/api/jobs/${encodeURIComponent(jobId)}`);
    if (!res.ok) return false;
    const data = await res.json();
    await openJobWorkspace(data, { restored: true });
    return true;
  }

  async function boot() {
    await loadDefaults();
    const session = loadSession();
    if (!session) return;

    const vendors = state.defaults.vendors || ["auto"];
    fillVendors($("sourceVendor"), vendors, session.sourceVendor);
    fillVendors($("targetVendor"), vendors, session.targetVendor);
    applyMapping(session.mapping);
    $("ignoreUnmapped").checked = session.ignoreUnmapped;
    state.lastRunMappingYaml = session.lastRunMappingYaml;
    state.lastRunIgnoreUnmapped = session.lastRunIgnoreUnmapped;

    if (session.jobId) {
      try {
        const ok = await restoreJob(session.jobId);
        if (!ok) {
          state.jobId = null;
          saveSession();
          $("statusMeta").textContent = "Previous diff expired — mapping restored; choose files to run again";
        } else if (mappingIsStaleVsLastRun()) {
          markNeedsRerun();
        }
      } catch {
        state.jobId = null;
        saveSession();
      }
    } else {
      saveSession();
    }
  }

  function applyMapping(raw) {
    state.mapping = normalizeMapping(raw);
    renderMappingEditor();
  }

  function renderMappingEditor() {
    const root = $("mappingEditor");
    if (!root) return;
    const m = state.mapping;
    const aliasRows = m.aliases.length
      ? m.aliases
          .map(
            (a, i) => `<tr>
          <td>${escapeHtml(a.source_name)}</td>
          <td>${escapeHtml(a.canonical.space)}</td>
          <td>${a.canonical.code}</td>
          <td>${escapeHtml(a.note || "")}</td>
          <td><button type="button" class="row-del" data-kind="alias" data-i="${i}">Remove</button></td>
        </tr>`
          )
          .join("")
      : `<tr class="empty-row"><td colspan="5">No aliases</td></tr>`;
    const equivRows = m.equivalences.length
      ? m.equivalences
          .map(
            (e, i) => `<tr>
          <td>${escapeHtml(e.source.space)}</td>
          <td>${e.source.code}</td>
          <td>${escapeHtml(e.target.space)}</td>
          <td>${e.target.code}</td>
          <td>${e.confirmed ? "yes" : "no"}</td>
          <td><button type="button" class="row-del" data-kind="equiv" data-i="${i}">Remove</button></td>
        </tr>`
          )
          .join("")
      : `<tr class="empty-row"><td colspan="6">No equivalences</td></tr>`;
    const ignoreRows = m.ignore.length
      ? m.ignore
          .map(
            (ig, i) => `<tr>
          <td>${escapeHtml(ig.space)}</td>
          <td>${ig.code}</td>
          <td><button type="button" class="row-del" data-kind="ignore" data-i="${i}">Remove</button></td>
        </tr>`
          )
          .join("")
      : `<tr class="empty-row"><td colspan="3">No ignore rules</td></tr>`;

    root.innerHTML = `
      <section class="mapping-section">
        <h3>Aliases</h3>
        <table class="mapping-table">
          <thead><tr><th>Name</th><th>Space</th><th>Code</th><th>Note</th><th></th></tr></thead>
          <tbody>${aliasRows}</tbody>
        </table>
        <div class="mapping-add aliases">
          <input type="text" id="mapAliasName" placeholder="option name" />
          <input type="text" id="mapAliasSpace" placeholder="space" value="dhcp" />
          <input type="number" id="mapAliasCode" placeholder="code" min="0" />
          <input type="text" id="mapAliasNote" placeholder="note (optional)" />
          <button type="button" id="mapAddAliasBtn">Add</button>
        </div>
      </section>
      <section class="mapping-section">
        <h3>Equivalences</h3>
        <table class="mapping-table">
          <thead><tr><th>Src space</th><th>Code</th><th>Tgt space</th><th>Code</th><th>Confirmed</th><th></th></tr></thead>
          <tbody>${equivRows}</tbody>
        </table>
        <div class="mapping-add equivalences">
          <input type="text" id="mapEqSrcSpace" placeholder="source space" />
          <input type="number" id="mapEqSrcCode" placeholder="code" min="0" />
          <input type="text" id="mapEqTgtSpace" placeholder="target space" />
          <input type="number" id="mapEqTgtCode" placeholder="code" min="0" />
          <label class="check"><input type="checkbox" id="mapEqConfirmed" checked /> confirmed</label>
          <button type="button" id="mapAddEquivBtn">Add</button>
        </div>
      </section>
      <section class="mapping-section">
        <h3>Ignore</h3>
        <table class="mapping-table">
          <thead><tr><th>Space</th><th>Code</th><th></th></tr></thead>
          <tbody>${ignoreRows}</tbody>
        </table>
        <div class="mapping-add ignore">
          <input type="text" id="mapIgnoreSpace" placeholder="space" value="dhcp" />
          <input type="number" id="mapIgnoreCode" placeholder="code" min="0" />
          <button type="button" id="mapAddIgnoreBtn">Add</button>
        </div>
      </section>`;
    bindMappingEditor();
  }

  function bindMappingEditor() {
    $("mappingEditor")?.querySelectorAll(".row-del").forEach((btn) => {
      btn.addEventListener("click", () => {
        const kind = btn.dataset.kind;
        const i = Number(btn.dataset.i);
        if (kind === "alias") state.mapping.aliases.splice(i, 1);
        else if (kind === "equiv") state.mapping.equivalences.splice(i, 1);
        else if (kind === "ignore") state.mapping.ignore.splice(i, 1);
        onMappingOrPrefsChanged();
        renderMappingEditor();
      });
    });
    $("mapAddAliasBtn")?.addEventListener("click", () => {
      const name = $("mapAliasName").value.trim();
      const space = $("mapAliasSpace").value.trim() || "dhcp";
      const code = Number($("mapAliasCode").value);
      const note = $("mapAliasNote").value.trim();
      if (!name || Number.isNaN(code)) {
        showError("Alias needs a name and numeric code.");
        return;
      }
      addAliasToModel(name, space, code, note || null);
    });
    $("mapAddEquivBtn")?.addEventListener("click", () => {
      const source = {
        space: $("mapEqSrcSpace").value.trim(),
        code: Number($("mapEqSrcCode").value),
      };
      const target = {
        space: $("mapEqTgtSpace").value.trim(),
        code: Number($("mapEqTgtCode").value),
      };
      const confirmed = $("mapEqConfirmed").checked;
      if (!insertEquivalence(source, target, confirmed)) return;
      $("mapEqSrcSpace").value = "";
      $("mapEqSrcCode").value = "";
      $("mapEqTgtSpace").value = "";
      $("mapEqTgtCode").value = "";
    });
    $("mapAddIgnoreBtn")?.addEventListener("click", () => {
      const space = $("mapIgnoreSpace").value.trim();
      const code = Number($("mapIgnoreCode").value);
      if (!space || Number.isNaN(code)) {
        showError("Ignore needs space and numeric code.");
        return;
      }
      if (state.mapping.ignore.some((ig) => ig.space === space && ig.code === code)) {
        showError("That ignore rule is already in the mapping.");
        return;
      }
      state.mapping.ignore.push({ space, code });
      clearError();
      onMappingOrPrefsChanged();
      renderMappingEditor();
    });
  }

  function addAliasToModel(name, space, code, note) {
    const exists = state.mapping.aliases.some(
      (a) => a.source_name === name && a.canonical.space === space && a.canonical.code === code
    );
    if (exists) {
      showError("That alias is already in the mapping.");
      return false;
    }
    const entry = { source_name: name, canonical: { space, code } };
    if (note) entry.note = note;
    state.mapping.aliases.push(entry);
    clearError();
    onMappingOrPrefsChanged();
    renderMappingEditor();
    return true;
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
      `/api/jobs/${state.jobId}/entries?category=${encodeURIComponent(state.filter)}&offset=${offset}&limit=${LIST_PAGE}${hideQueryParam()}`
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
    const counts = state.visibleCounts ||
      state.counts || {
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

  function canIgnoreParentFor(entry) {
    const kind = entry?.entity?.kind;
    const d = entry?.entity?.display || {};
    if (kind === "option") {
      return !!(d.declaration_key && d.option_id);
    }
    return d.parent_key != null && d.parent_key !== "";
  }

  function ignoreParentTooltip(entry) {
    const kind = entry?.entity?.kind;
    const d = entry?.entity?.display || {};
    if (kind === "option") {
      if (!d.declaration_key || !d.option_id) {
        return "No declaration site for this option";
      }
      return `Hide ${d.option_id} declared at ${d.declaration_key}`;
    }
    if (d.parent_key) {
      return `Hide all related ${kind || "items"} under ${d.parent_key}`;
    }
    return "No related group for this entry";
  }

  function mappingIgnoreRefFromEntry(entry) {
    if (!entry || entry.entity?.kind !== "option") return null;
    const oid = entry.entity?.display?.option_id;
    if (oid) {
      const fromId = spaceCodeFromLabel(oid);
      if (fromId) return fromId;
    }
    return optionRefFromEntry(entry);
  }

  function canAddMappingIgnore(entry) {
    return !!mappingIgnoreRefFromEntry(entry);
  }

  function mappingIgnoreTooltip(entry) {
    const ref = mappingIgnoreRefFromEntry(entry);
    if (!ref) return "Only options can be added to the mapping ignore list";
    const already = (state.mapping.ignore || []).some(
      (ig) => ig.space === ref.space && ig.code === ref.code
    );
    if (already) return `${formatOptionRef(ref)} is already in the mapping ignore list`;
    return `Add ${formatOptionRef(ref)} to mapping ignore`;
  }

  function addSelectedToMappingIgnore() {
    const e = state.selectedEntry;
    const ref = mappingIgnoreRefFromEntry(e);
    if (!ref) {
      showError("Only options with a space:code can be added to mapping ignore.");
      return;
    }
    if ((state.mapping.ignore || []).some((ig) => ig.space === ref.space && ig.code === ref.code)) {
      showError(`${formatOptionRef(ref)} is already in the mapping ignore list.`);
      return;
    }
    state.mapping.ignore.push({ space: ref.space, code: ref.code });
    clearError();
    onMappingOrPrefsChanged();
    renderMappingEditor();
    const panel = $("mappingPanel");
    if (panel) panel.open = true;
    $("statusMeta").textContent = `Added ${formatOptionRef(ref)} to mapping ignore`;
    renderDetailShell();
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
    const canHideRelated = canIgnoreParentFor(e);
    const hideRelatedTitle = ignoreParentTooltip(e);
    const canMapIgnore = canAddMappingIgnore(e);
    const mapIgnoreRef = mappingIgnoreRefFromEntry(e);
    const mapIgnoreAlready =
      !!mapIgnoreRef &&
      (state.mapping.ignore || []).some(
        (ig) => ig.space === mapIgnoreRef.space && ig.code === mapIgnoreRef.code
      );
    const mapIgnoreTitle = mappingIgnoreTooltip(e);
    pane.innerHTML = `
      <div class="detail-head">
        <span class="badge ${escapeHtml(e.category)}">${escapeHtml(categoryLabel(e.category))}</span>
        <h2>${escapeHtml(d.summary)}</h2>
        <div class="detail-actions">
          <button type="button" id="ignoreEntryBtn" title="Hide this difference from the list">Hide</button>
          <button type="button" id="ignoreParentBtn" ${canHideRelated ? "" : "disabled"} title="${escapeHtml(hideRelatedTitle)}">Hide related</button>
          <button type="button" id="mappingIgnoreBtn" ${canMapIgnore && !mapIgnoreAlready ? "" : "disabled"} title="${escapeHtml(mapIgnoreTitle)}">Ignore in mapping</button>
        </div>
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
    $("ignoreEntryBtn")?.addEventListener("click", ignoreSelectedEntry);
    $("ignoreParentBtn")?.addEventListener("click", ignoreSelectedParent);
    $("mappingIgnoreBtn")?.addEventListener("click", addSelectedToMappingIgnore);
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
        $("statusMeta").textContent = "Equivalence added";
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
      (s) => !equivalenceAlreadyMapped(s.source, s.target)
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
        $("statusMeta").textContent = "Equivalence added";
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
    const entry = state.selectedEntry;
    const hl = sideHighlights(entry?.locations?.[side]);
    const isAbsentSide =
      (entry?.category === "missing" && side === "target") ||
      (entry?.category === "extra" && side === "source");
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
    const parentBanner = isAbsentSide
      ? `<div class="parent-scope-banner">Not present here — showing parent scope</div>`
      : "";
    return `<div class="code-panel${isAbsentSide ? " parent-context" : ""}">
      <header>
        <span class="side">${escapeHtml(label)}</span>
        <span>${escapeHtml(meta.name || side)} · ${escapeHtml(parts.join(" · "))}</span>
      </header>
      ${parentBanner}
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
      fd.append("mapping_yaml", serializeMappingYaml(state.mapping));
      fd.append("ignore_unmapped", $("ignoreUnmapped").checked ? "true" : "false");

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
        refreshHiddenMeta();
        clearNeedsRerun();
        saveSession();
        return;
      }

      state.lastRunMappingYaml = serializeMappingYaml(state.mapping);
      state.lastRunIgnoreUnmapped = !!$("ignoreUnmapped").checked;
      const prevJobId = state.jobId;
      const carryHide = {
        entries: [...state.hide.entries],
        parents: [...state.hide.parents],
      };
      await openJobWorkspace(data, { restored: false, carryHide });
      if (prevJobId && prevJobId !== state.jobId) {
        const oldKey = hideStorageKey(prevJobId);
        localStorage.removeItem(oldKey);
        sessionStorage.removeItem(oldKey);
      }
      clearNeedsRerun();
      saveSession();
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
    if (!addAliasToModel(name, space, code, null)) return;
    const panel = $("mappingPanel");
    if (panel) panel.open = true;
  }

  function insertEquivalence(source, target, confirmed = true) {
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
    if (equivalenceAlreadyMapped(source, target)) {
      showError("That equivalence is already in the mapping.");
      return false;
    }
    state.mapping.equivalences.push({
      source: { space: source.space, code: source.code },
      target: { space: target.space, code: target.code },
      confirmed: !!confirmed,
    });
    const panel = $("mappingPanel");
    if (panel) panel.open = true;
    clearError();
    onMappingOrPrefsChanged();
    renderMappingEditor();
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
    const blob = new Blob([serializeMappingYaml(state.mapping)], { type: "text/yaml" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "user.yaml";
    a.click();
    URL.revokeObjectURL(a.href);
  }

  async function loadMappingFromFile(file) {
    if (!file) return;
    const fd = new FormData();
    fd.append("file", file, file.name || "user.yaml");
    try {
      const res = await fetch("/api/mapping/parse", { method: "POST", body: fd });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        const detail = data.detail || data;
        const msg =
          typeof detail === "string"
            ? detail
            : detail.message || detail.error || JSON.stringify(detail);
        showError(msg);
        return;
      }
      applyMapping(data.mapping || emptyMapping());
      clearError();
      onMappingOrPrefsChanged();
      $("statusMeta").textContent = `Loaded mapping from ${file.name}`;
      const panel = $("mappingPanel");
      if (panel) panel.open = true;
    } catch (err) {
      showError(String(err));
    }
  }

  $("runBtn").addEventListener("click", runDiff);
  $("resetHiddenBtn")?.addEventListener("click", () => {
    resetHiddenDiffs().catch((e) => showError(String(e)));
  });
  $("addAliasBtn").addEventListener("click", addAlias);
  $("loadMappingBtn").addEventListener("click", () => $("loadMappingFile").click());
  $("loadMappingFile").addEventListener("change", async (ev) => {
    const file = ev.target.files?.[0];
    ev.target.value = "";
    await loadMappingFromFile(file);
  });
  $("downloadMappingBtn").addEventListener("click", downloadMapping);
  $("resetMappingBtn").addEventListener("click", () => {
    if (!state.defaults) return;
    applyMapping(state.defaults.mapping || emptyMapping());
    onMappingOrPrefsChanged();
  });

  $("ignoreUnmapped")?.addEventListener("change", () => {
    onMappingOrPrefsChanged();
  });

  $("sourceVendor")?.addEventListener("change", () => {
    saveSession();
    markNeedsRerun();
  });
  $("targetVendor")?.addEventListener("change", () => {
    saveSession();
    markNeedsRerun();
  });

  boot().catch((e) => showError(String(e)));
})();
