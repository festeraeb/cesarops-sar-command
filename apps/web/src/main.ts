import L, { type GeoJSON, type Map as LeafletMap } from "leaflet";
import "./styles.css";
import {
  addLog, createCase, exportCaseJson, exportReportPdf, getCase, listCases, saveTeamMember,
  type CaseRecord, type Contact,
} from "./bridge";

type Tab = "dashboard" | "intake" | "map" | "updates" | "teams" | "reports";
const app = document.querySelector<HTMLDivElement>("#app")!;
let cases: CaseRecord[] = [];
let current: CaseRecord | undefined;
let tab: Tab = "dashboard";
let map: LeafletMap | undefined;
let overlays: { name: string; layer: GeoJSON }[] = [];
let notice = "";
let error = "";

const esc = (value: unknown) => String(value ?? "").replace(/[&<>"']/g, (char) => (
  { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char]!
));
const field = (form: FormData, name: string) => String(form.get(name) ?? "").trim();
const number = (form: FormData, name: string) => {
  const value = field(form, name);
  return value === "" ? undefined : Number(value);
};
const saveFile = (name: string, content: BlobPart, type: string) => {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const a = Object.assign(document.createElement("a"), { href: url, download: name });
  a.click();
  URL.revokeObjectURL(url);
};
const setMessage = (text: string, isError = false) => {
  notice = isError ? "" : text;
  error = isError ? text : "";
};

const tabs: { id: Tab; label: string }[] = [
  { id: "dashboard", label: "Mission" }, { id: "intake", label: "Intake" },
  { id: "map", label: "Map" }, { id: "updates", label: "Updates & Clues" },
  { id: "teams", label: "Teams" }, { id: "reports", label: "Reports" },
];

async function refresh(selectId?: string) {
  cases = await listCases();
  const id = selectId ?? current?.id ?? cases[0]?.id;
  current = id ? await getCase(id) : undefined;
}

function shell(content: string) {
  app.innerHTML = `<div class="shell">
    <header class="topbar">
      <div class="brand">CESAROPS · SAR COMMAND</div>
      <span class="status">${("__TAURI_INTERNALS__" in window) ? "Desktop · SQLite" : "Web · local offline store"}</span>
      <label class="case-picker">Active case
        <select id="case-select">
          <option value="">No case selected</option>
          ${cases.map((item) => `<option value="${esc(item.id)}" ${item.id === current?.id ? "selected" : ""}>${esc(item.mission_id)} · ${esc(item.subject_name)}</option>`).join("")}
        </select>
      </label>
    </header>
    <nav class="tabs">${tabs.map((item) => `<button data-tab="${item.id}" class="${tab === item.id ? "active" : ""}">${item.label}</button>`).join("")}</nav>
    <main>
      ${notice ? `<div class="notice">${esc(notice)}</div>` : ""}
      ${error ? `<div class="notice error">${esc(error)}</div>` : ""}
      ${content}
    </main>
  </div>`;

  document.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((button) => {
    button.onclick = () => { tab = button.dataset.tab as Tab; setMessage(""); render(); };
  });
  document.querySelector<HTMLSelectElement>("#case-select")!.onchange = async (event) => {
    const id = (event.target as HTMLSelectElement).value;
    current = id ? await getCase(id) : undefined;
    render();
  };
}

function dashboard() {
  if (!current) return `<section class="panel empty"><h1>No active SAR case</h1><p>Start with a verified call intake.</p><button class="primary" id="start-intake">Create intake</button></section>`;
  const logs = [...current.updates, ...current.clues].sort((a, b) => b.time.localeCompare(a.time));
  return `<section class="panel">
    <h1>${esc(current.mission_id)} · ${esc(current.subject_name)}</h1>
    <div class="grid three">
      <div class="metric"><strong>${esc(current.priority)}</strong><span>Priority</span></div>
      <div class="metric"><strong>${current.team_members.length}</strong><span>Assigned responders</span></div>
      <div class="metric"><strong>${current.clues.length}</strong><span>Recorded clues</span></div>
    </div>
    <div class="grid" style="margin-top:14px">
      <section><h2>Situation</h2>
        <p><b>${esc(current.domain)} / ${esc(current.objective)}</b></p>
        <p>LKP: ${current.lkp_lat == null ? "Not recorded" : `${current.lkp_lat.toFixed(5)}, ${current.lkp_lon?.toFixed(5)}`}</p>
        <p>${esc(current.lkp_description || "")}</p>
      </section>
      <section><h2>Latest activity</h2><div class="feed">
        ${logs.slice(0, 4).map((item) => `<article class="${item.kind}"><div class="meta">${esc(item.time)} · ${esc(item.role)}</div>${esc(item.text)}</article>`).join("") || `<div class="empty">No operational updates yet.</div>`}
      </div></section>
    </div>
  </section>`;
}

function intake() {
  return `<section class="panel"><h1>Search and rescue intake</h1>
    <p class="status">Record reported facts only. Mission ID, subject, and a valid LKP are required.</p>
    <form id="intake-form">
      <div class="grid">
        <label>Mission ID<input name="mission_id" required placeholder="SAR-2026-001"></label>
        <label>Priority<select name="priority"><option>critical</option><option selected>high</option><option>medium</option><option>low</option></select></label>
        <label>Domain<select name="domain"><option value="water">Water</option><option value="land">Land</option></select></label>
        <label>Objective<select name="objective"><option value="vessel_sar">Vessel SAR</option><option value="aircraft_sar">Aircraft SAR</option><option value="person_search">Person search</option><option value="vehicle_sar">Vehicle SAR</option><option value="wreck_hunt">Wreck hunt</option></select></label>
        <label>Subject name<input name="subject_name" required></label>
        <label>Subject age<input name="subject_age" type="number" min="0" max="130"></label>
        <label class="span-2">Medical conditions (comma separated)<input name="medical_conditions"></label>
        <label>LKP latitude<input name="lkp_lat" type="number" step="any" min="-90" max="90" required></label>
        <label>LKP longitude<input name="lkp_lon" type="number" step="any" min="-180" max="180" required></label>
        <label>LKP time<input name="lkp_time" type="datetime-local"></label>
        <label>LKP description<input name="lkp_description" placeholder="Who reported it and how it was established"></label>
        <label>Contact name<input name="contact_name"></label>
        <label>Contact phone<input name="contact_phone" type="tel"></label>
        <label>Relationship<input name="contact_relationship"></label>
      </div>
      <div class="actions"><button class="primary" type="submit">Open SAR case</button></div>
    </form>
  </section>`;
}

function mapView() {
  if (!current) return noCase("map");
  return `<section class="map-layout">
    <aside class="panel"><h2>Operational layers</h2>
      <p class="status">Paste a drift or detection GeoJSON endpoint. Data is displayed as received and never inferred.</p>
      <form id="layer-form">
        <label>Layer name<input name="name" required placeholder="Drift model 14:00Z"></label>
        <label style="margin-top:8px">GeoJSON URL<input name="url" type="url" required placeholder="https://…/result.geojson"></label>
        <button class="primary" style="margin-top:10px">Fetch layer</button>
      </form>
      <h3 style="margin-top:18px">Loaded</h3>
      <div class="layer-list">${overlays.map((item, index) => `<div class="layer-item"><span>●</span><span>${esc(item.name)}</span><button class="secondary" data-remove-layer="${index}">×</button></div>`).join("") || `<span class="status">No live layers loaded</span>`}</div>
      <p class="status" style="margin-top:18px">Clues: ${current.clues.filter((item) => item.lat != null).length} · LKP is amber.</p>
    </aside>
    <div id="map"></div>
  </section>`;
}

function updates() {
  if (!current) return noCase("updates");
  const logs = [...current.updates, ...current.clues].sort((a, b) => b.time.localeCompare(a.time));
  return `<div class="grid">
    <section class="panel"><h2>Add operational record</h2>
      <form id="log-form">
        <div class="grid">
          <label>Type<select name="kind"><option value="update">Update</option><option value="clue">Clue</option></select></label>
          <label>Role<input name="role" required placeholder="Incident Commander"></label>
          <label class="span-2">Observed fact / decision<textarea name="text" required></textarea></label>
          <label>Latitude (clue)<input name="lat" type="number" step="any" min="-90" max="90"></label>
          <label>Longitude (clue)<input name="lon" type="number" step="any" min="-180" max="180"></label>
        </div><button class="primary" style="margin-top:10px">Append record</button>
      </form>
    </section>
    <section class="panel"><h2>Immutable mission log</h2><div class="feed">
      ${logs.map((item) => `<article class="${item.kind}"><div class="meta">${esc(item.time)} · ${esc(item.role)} · ${item.kind}</div>${esc(item.text)}${item.lat == null ? "" : `<div class="meta">${item.lat.toFixed(5)}, ${item.lon?.toFixed(5)}</div>`}</article>`).join("") || `<div class="empty">No records.</div>`}
    </div></section>
  </div>`;
}

function teams() {
  if (!current) return noCase("teams");
  return `<div class="grid">
    <section class="panel"><h2>Assign responder</h2><form id="team-form">
      <label>Name<input name="name" required></label>
      <label>Callsign<input name="callsign" required></label>
      <label>ICS role<input name="role" required placeholder="Marine Unit Leader"></label>
      <label>Status<select name="status"><option>available</option><option selected>assigned</option><option>deployed</option><option>resting</option><option>released</option></select></label>
      <button class="primary" style="margin-top:10px">Add to case</button>
    </form></section>
    <section class="panel"><h2>Team roster</h2><table class="table"><thead><tr><th>Callsign</th><th>Name</th><th>Role</th><th>Status</th></tr></thead><tbody>
      ${current.team_members.map((member) => `<tr><td>${esc(member.callsign)}</td><td>${esc(member.name)}</td><td>${esc(member.role)}</td><td>${esc(member.status)}</td></tr>`).join("")}
    </tbody></table>${current.team_members.length ? "" : `<div class="empty">No responders assigned.</div>`}</section>
  </div>`;
}

function reportMarkup(reportType: string) {
  if (!current) return "";
  const logs = [...current.updates, ...current.clues].sort((a, b) => a.time.localeCompare(b.time));
  const title = reportType === "family" ? "Family Situation Report" : reportType === "team" ? "Operational Team Report" : "Incident Commander Report";
  return `<article><h1>${title}</h1><p><b>Case:</b> ${esc(current.mission_id)}</p><p><b>Generated:</b> ${esc(new Date().toISOString())}</p>
    <h2>Known situation</h2><p>Search for ${esc(current.subject_name)}. Priority: ${esc(current.priority)}. Last known position: ${current.lkp_lat?.toFixed(5)}, ${current.lkp_lon?.toFixed(5)}.</p>
    ${reportType === "family" ? `<p>This report includes recorded operational facts only. Unconfirmed hypotheses are excluded.</p>` : `<h2>Resources</h2><ul>${current.team_members.map((member) => `<li>${esc(member.callsign)} — ${esc(member.role)} — ${esc(member.status)}</li>`).join("")}</ul>`}
    <h2>Recorded activity</h2><ul>${logs.map((item) => `<li><b>${esc(item.time)}</b> — ${esc(item.text)} (${esc(item.role)})</li>`).join("") || "<li>No activity recorded.</li>"}</ul>
  </article>`;
}

function reports() {
  if (!current) return noCase("reports");
  return `<section class="panel"><div class="report-controls">
    <h2>Report export</h2><div class="actions">
      <select id="report-type" style="width:auto"><option value="family">Family situation</option><option value="team">SAR team operations</option><option value="ic">Incident commander</option></select>
      <button class="secondary" id="preview-report">Refresh preview</button>
      <button class="secondary" id="html-report">Export HTML</button>
      <button class="primary" id="pdf-report">Export PDF / Print</button>
      <button class="secondary" id="json-report">Case JSON</button>
    </div><p class="status">Family exports are limited to the verified case log. Review the preview before release.</p>
    </div><div class="report-preview" id="report-preview">${reportMarkup("family")}</div></section>`;
}

function noCase(noun: string) {
  return `<section class="panel empty">Select or create a case before using ${noun}.</section>`;
}

function render() {
  map?.remove(); map = undefined;
  const views: Record<Tab, () => string> = { dashboard, intake, map: mapView, updates, teams, reports };
  shell(views[tab]());
  bind();
}

function bind() {
  document.querySelector<HTMLButtonElement>("#start-intake")?.addEventListener("click", () => { tab = "intake"; render(); });
  document.querySelector<HTMLFormElement>("#intake-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      const data = new FormData(event.currentTarget as HTMLFormElement);
      const contact: Contact = { name: field(data, "contact_name"), phone: field(data, "contact_phone"), relationship: field(data, "contact_relationship") };
      const record = await createCase({
        mission_id: field(data, "mission_id"), domain: field(data, "domain"), objective: field(data, "objective"),
        priority: field(data, "priority"), subject_name: field(data, "subject_name"), subject_age: number(data, "subject_age"),
        medical_conditions: field(data, "medical_conditions").split(",").map((item) => item.trim()).filter(Boolean),
        lkp_lat: number(data, "lkp_lat"), lkp_lon: number(data, "lkp_lon"), lkp_description: field(data, "lkp_description"),
        lkp_time: field(data, "lkp_time") ? new Date(field(data, "lkp_time")).toISOString() : undefined,
        contacts: contact.name || contact.phone ? [contact] : [],
      });
      await refresh(record.id); tab = "dashboard"; setMessage(`Case ${record.mission_id} opened and saved offline.`); render();
    } catch (cause) { setMessage(String(cause), true); render(); }
  });
  document.querySelector<HTMLFormElement>("#log-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!current) return;
    try {
      const data = new FormData(event.currentTarget as HTMLFormElement);
      const kind = field(data, "kind") as "update" | "clue";
      const lat = number(data, "lat"); const lon = number(data, "lon");
      if ((lat == null) !== (lon == null)) throw new Error("Latitude and longitude must be supplied together");
      await addLog(current.id, kind, { time: new Date().toISOString(), role: field(data, "role"), text: field(data, "text"), lat, lon });
      await refresh(current.id); setMessage(`${kind === "clue" ? "Clue" : "Update"} appended to the case log.`); render();
    } catch (cause) { setMessage(String(cause), true); render(); }
  });
  document.querySelector<HTMLFormElement>("#team-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!current) return;
    try {
      const data = new FormData(event.currentTarget as HTMLFormElement);
      await saveTeamMember(current.id, { name: field(data, "name"), callsign: field(data, "callsign"), role: field(data, "role"), status: field(data, "status") });
      await refresh(current.id); setMessage("Responder assigned."); render();
    } catch (cause) { setMessage(String(cause), true); render(); }
  });
  if (tab === "map" && current) setTimeout(initMap);
  bindReports();
}

function initMap() {
  if (!current || !document.querySelector("#map")) return;
  const center: L.LatLngExpression = [current.lkp_lat ?? 42.5, current.lkp_lon ?? -82.7];
  map = L.map("map", { zoomControl: true }).setView(center, 10);
  L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", { attribution: "© OpenStreetMap", maxZoom: 19 }).addTo(map);
  if (current.lkp_lat != null && current.lkp_lon != null) {
    L.circleMarker([current.lkp_lat, current.lkp_lon], { radius: 9, color: "#f6ad2f", fillOpacity: .9 }).bindPopup("Last known position").addTo(map);
  }
  current.clues.filter((item) => item.lat != null && item.lon != null).forEach((item) => {
    L.marker([item.lat!, item.lon!]).bindPopup(`<b>Clue</b><br>${esc(item.text)}<br><small>${esc(item.time)}</small>`).addTo(map!);
  });
  overlays.forEach((item) => item.layer.addTo(map!));
  document.querySelector<HTMLFormElement>("#layer-form")!.onsubmit = async (event) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget as HTMLFormElement);
    try {
      const response = await fetch(field(data, "url"));
      if (!response.ok) throw new Error(`Layer request failed (${response.status})`);
      const geojson = await response.json();
      if (geojson?.type !== "FeatureCollection" && geojson?.type !== "Feature") throw new Error("Response is not GeoJSON");
      const layer = L.geoJSON(geojson, { style: { color: "#38bdf8", weight: 2, fillOpacity: .18 } });
      overlays.push({ name: field(data, "name"), layer });
      setMessage("GeoJSON layer loaded without modification."); render();
    } catch (cause) { setMessage(String(cause), true); render(); }
  };
  document.querySelectorAll<HTMLButtonElement>("[data-remove-layer]").forEach((button) => {
    button.onclick = () => { overlays.splice(Number(button.dataset.removeLayer), 1); render(); };
  });
}

function bindReports() {
  const preview = document.querySelector<HTMLDivElement>("#report-preview");
  const selector = document.querySelector<HTMLSelectElement>("#report-type");
  if (!preview || !selector || !current) return;
  const update = () => { preview.innerHTML = reportMarkup(selector.value); };
  document.querySelector("#preview-report")?.addEventListener("click", update);
  document.querySelector("#html-report")?.addEventListener("click", () => {
    const html = `<!doctype html><html><head><meta charset="utf-8"><title>${esc(current!.mission_id)} report</title></head><body>${reportMarkup(selector.value)}</body></html>`;
    saveFile(`${current!.mission_id}-${selector.value}.html`, html, "text/html");
  });
  document.querySelector("#pdf-report")?.addEventListener("click", async () => {
    try {
      const bytes = await exportReportPdf(current!.id, selector.value);
      if (bytes) saveFile(`${current!.mission_id}-${selector.value}.pdf`, new Uint8Array(bytes), "application/pdf");
      else window.print();
    } catch (cause) { setMessage(String(cause), true); render(); }
  });
  document.querySelector("#json-report")?.addEventListener("click", async () => {
    const json = await exportCaseJson(current!.id);
    saveFile(`${current!.mission_id}.json`, json, "application/json");
  });
}

refresh().then(render).catch((cause) => { setMessage(String(cause), true); render(); });
