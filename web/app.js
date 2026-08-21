import init, { compilePack, graphVault, lintVault, parseNote, version } from "./pkg/memoryfs_wasm.js";

const DEMO_PATHS = ["01-current-policy.md", "02-stale-observation.md", "03-conflict-a.md", "04-conflict-b.md", "05-broken-link.md", "06-duplicate-id.md", "07-restricted-instruction.md", "08-secret-canary.md", "09-attachment-and-block.md", "10-unknown-metadata.md"];
const state = { files: [], parsed: [], lint: null, graph: null, pack: null };
const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];

async function boot() {
  try {
    await init();
    $("#runtime-status").textContent = `WASM ${version()} ready`;
    bindEvents();
  } catch (error) {
    showError(error);
    $("#runtime-status").textContent = "WASM unavailable";
  }
}

function bindEvents() {
  $("#vault-input").addEventListener("change", async (event) => {
    const pending = [...event.target.files]
      .filter((file) => file.name.toLowerCase().endsWith(".md"))
      .map(async (file) => ({ path: file.webkitRelativePath || file.name, source: await file.text() }));
    await loadVault(await Promise.all(pending));
  });
  $("#load-demo").addEventListener("click", loadDemo);
  $("#compile-form").addEventListener("submit", (event) => { event.preventDefault(); compile(); });
  $$(".tab").forEach((tab) => tab.addEventListener("click", () => selectTab(tab.dataset.tab)));
}

async function loadDemo() {
  try {
    const files = await Promise.all(DEMO_PATHS.map(async (path) => {
      const response = await fetch(`../fixtures/vault/${path}`);
      if (!response.ok) throw new Error(`Could not load ${path}`);
      return { path, source: await response.text() };
    }));
    await loadVault(files);
  } catch (error) { showError(error); }
}

async function loadVault(files) {
  if (!files.length) return;
  clearError();
  try {
    state.files = files.sort((left, right) => left.path.localeCompare(right.path));
    state.parsed = state.files.map((file) => JSON.parse(parseNote(file.path, file.source)));
    const payload = JSON.stringify(state.files);
    state.lint = JSON.parse(lintVault(payload, new Date().toISOString()));
    state.graph = JSON.parse(graphVault(payload));
    $(".workspace").classList.add("has-data");
    renderVault();
    renderFindings();
    renderGraph();
    compile();
  } catch (error) { showError(error); }
}

function compile() {
  if (!state.files.length) return;
  clearError();
  try {
    const request = {
      task: $("#task").value.trim(),
      caller: { project: $("#project").value.trim(), agent: "release-agent", environment: "production", max_sensitivity: $("#clearance").value },
      allowed_note_types: [],
      token_budget: Number($("#budget").value),
      required_evidence: [],
      policy_version: "browser-v1",
      ranking: document.querySelector('input[name="ranking"]:checked').value,
      stale_after_days: 180,
      max_link_depth: 1,
    };
    state.pack = JSON.parse(compilePack(JSON.stringify(state.files), JSON.stringify(request), new Date().toISOString()));
    renderContext();
    renderSummary();
  } catch (error) { showError(error); }
}

function renderVault() {
  $("#note-count").textContent = String(state.parsed.length);
  $("#note-list").replaceChildren(...state.parsed.map((parsed) => {
    const item = document.createElement("li");
    const title = document.createElement("strong");
    const meta = document.createElement("span");
    title.textContent = parsed.note.title || parsed.note.id;
    meta.textContent = `${parsed.note.type} · ${parsed.note.sensitivity} · ${parsed.note.trust}`;
    item.append(title, meta);
    return item;
  }));
}

function renderFindings() {
  const findings = state.lint?.findings || [];
  $("#findings-body").replaceChildren(...findings.map((finding) => row([
    pill(finding.suppressed ? "suppressed" : finding.severity, finding.severity),
    finding.code,
    finding.note_id,
    `${shortPath(finding.location.file)}:${finding.location.line}`,
    finding.suppressed ? `${finding.message} (${finding.suppression_reason})` : finding.message,
  ])));
}

function renderContext() {
  const candidates = state.pack?.candidates || [];
  $("#context-body").replaceChildren(...candidates.map((candidate) => {
    const status = document.createElement("div");
    status.className = "status-stack";
    [candidate.temporal_status, candidate.conflict_status, candidate.authority_status].forEach((value) => {
      const label = document.createElement("span");
      label.textContent = value.replaceAll("_", " ");
      status.append(label);
    });
    return row([pill(candidate.decision, candidate.decision), candidate.note_id, status, String(candidate.score), candidate.reason]);
  }));
}

function renderSummary() {
  const candidates = state.pack?.candidates || [];
  $("#finding-count").textContent = String(state.lint?.findings.length || 0);
  $("#included-count").textContent = String(candidates.filter((item) => item.decision === "included").length);
  $("#excluded-count").textContent = String(candidates.filter((item) => item.decision === "excluded").length);
  $("#token-count").textContent = String(state.pack?.total_estimated_tokens || 0);
  $("#pack-digest").textContent = state.pack ? state.pack.receipt.value : "Not compiled";
}

function renderGraph() {
  const svg = $("#graph");
  svg.replaceChildren();
  const nodes = state.graph?.nodes || [];
  const edges = state.graph?.edges || [];
  const radius = Math.min(210, 34 * nodes.length);
  const positions = nodes.map((node, index) => {
    const angle = (index / Math.max(nodes.length, 1)) * Math.PI * 2 - Math.PI / 2;
    return { node, x: 500 + Math.cos(angle) * radius, y: 280 + Math.sin(angle) * radius };
  });
  edges.forEach((edge) => {
    const source = positions.find((position) => position.node.id === edge.source);
    const target = positions.find((position) => position.node.id === edge.target || edge.target.includes(position.node.id));
    if (source && target) svg.append(svgElement("line", { x1: source.x, y1: source.y, x2: target.x, y2: target.y, class: "graph-edge" }));
  });
  positions.forEach(({ node, ...position }) => {
    svg.append(svgElement("circle", { cx: position.x, cy: position.y, r: 28, class: `graph-node ${node.sensitivity === "restricted" ? "restricted" : ""}` }));
    const label = svgElement("text", { x: position.x, y: position.y + 45, class: "graph-label" });
    label.textContent = node.id;
    svg.append(label);
  });
}

function row(values) {
  const tr = document.createElement("tr");
  values.forEach((value) => {
    const td = document.createElement("td");
    if (value instanceof Node) td.append(value); else td.textContent = value;
    tr.append(td);
  });
  return tr;
}

function pill(text, className) {
  const span = document.createElement("span");
  span.className = `pill ${className}`;
  span.textContent = text;
  return span;
}

function svgElement(name, attributes) {
  const element = document.createElementNS("http://www.w3.org/2000/svg", name);
  Object.entries(attributes).forEach(([key, value]) => element.setAttribute(key, String(value)));
  return element;
}

function selectTab(name) {
  $$(".tab").forEach((tab) => tab.classList.toggle("active", tab.dataset.tab === name));
  $$(".view").forEach((view) => view.classList.toggle("active", view.id === `${name}-view`));
}

function shortPath(path) { return path.split("/").pop(); }
function showError(error) { const banner = $("#error-banner"); banner.textContent = String(error?.message || error); banner.hidden = false; }
function clearError() { $("#error-banner").hidden = true; }

boot();
