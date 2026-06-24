# SAR Command Frontend — Action Plan

Return-to document for `cesarops-sar-command`. Benchmark: [D4H SAR](https://www.d4h.com/sectors/search-rescue-sar). Recovery reference: `wreckhunter2000-1/todo/sar-platform-recovery/`.

---

## Goal

Ship an **offline-capable command post UI** that covers intake → map/assignments → clues/updates → family & team reports, talking to fleet detection via APIs — not reimplementing satellite/mag/sonar in this repo.

---

## Phase 0 — Repo & decisions (you are here)

- [x] Create `cesarops-sar-command` planning repo
- [x] Document architecture + Rust crates ([ARCHITECTURE.md](./ARCHITECTURE.md), [RUST_CRATES.md](./RUST_CRATES.md))
- [ ] **Decision:** Tauri + TypeScript UI *vs* Tauri + Leptos (recommend TS for Phase 1 maps)
- [ ] **Decision:** Embedded SQLite only *vs* SQLite + optional LAN sidecar (`cesarops-platform`)
- [ ] Copy/link recovery assets: `schemas/` from `sar_platform_core` README + `unified_pipeline_profiles.json` Plays
- [ ] `git init` + remote `festeraeb/cesarops-sar-command` when ready to push

**Exit criteria:** Stack written down; no code required yet.

---

## Phase 1 — Static shell (2–3 weeks)

Build navigable UI with mock data — no backend.

| Screen | D4H analog | Notes |
|--------|------------|-------|
| Intake form | Situation / case record | Subject, LKP, contacts, medical |
| Mission dashboard | Dashboard + Play picker | `water_vessel_sar`, `land_aircraft_sar`, etc. |
| Map view | Maps | Leaflet/MapLibre; mock LKP + one assignment polygon |
| Updates feed | Updates tab | Chronological list, role tags |
| Teams panel | Roles | Table of responders + status |
| Reports preview | Situation share | Read-only family + team text blocks |

**Tasks**

- [ ] Scaffold `apps/tauri` (copy pattern from `cesarops-core/tauri`, rename crate)
- [ ] Scaffold `apps/web` (Vite + TS) or `apps/ui` (Leptos)
- [ ] Wire Tauri ↔ WebView dev loop
- [ ] Port visual language from `todo/.../web-mocks/sar_portal.html` (amber SAR theme)
- [ ] Define `command-types` JSON schemas in `schemas/`

**Exit criteria:** Demo runs locally; all screens reachable with fixtures.

---

## Phase 2 — Local persistence (2 weeks)

Offline-first data layer in Rust (`src-tauri` or `crates/command-db`).

- [ ] SQLite schema: `cases`, `subjects`, `contacts`, `assignments`, `clues`, `updates`, `team_members`
- [ ] Tauri commands: `create_case`, `list_cases`, `add_clue`, `add_update`, `export_case_json`
- [ ] Migrations (`refinery` or `sqlx migrate`)
- [ ] UI reads/writes via Tauri IPC (no network)

**Exit criteria:** Create case offline, restart app, data still there.

---

## Phase 3 — Map & assignments (2–3 weeks)

- [ ] GeoJSON layer API in Rust (`geo` + `geojson` crates)
- [ ] Draw/edit assignment polygons on map
- [ ] Clue pin drops sync to `clues` table + map layer
- [ ] Import GPX track (`gpx` crate) for team breadcrumb
- [ ] Scenario likelihood labels (Very Likely → Very Unlikely) per D4H

**Exit criteria:** Full mock mission on map without server.

---

## Phase 4 — Backend hookup (2 weeks)

Connect to fleet — minimal scope.

- [ ] Point API client at `cesarops-platform` (`:9200`) or embed `cesarops-platform` as `apps/sidecar`
- [ ] Implement real `dispatch` + `admin` routes (replace stubs in recovery `rust-platform`)
- [ ] WebSocket `/ws/updates` for multi-operator log (optional LAN)
- [ ] Sync outbox: queue local writes → POST when online
- [ ] Drift / detection overlays: `GET` GeoJSON from forge or wrecks API — display only

**Exit criteria:** One machine runs UI + sidecar; second browser tab sees live updates.

---

## Phase 5 — Reports & stakeholders (2 weeks)

- [ ] `command-reports` crate: Tera templates from recovery Python reports
- [ ] Family situation report (plain language) + export HTML/PDF
- [ ] SAR team operational report + debrief notes field
- [ ] IC comprehensive report
- [ ] Share link stub (static export or future public read-only route)

Port from: `todo/sar-platform-recovery/python-prototype/reports/`.

**Exit criteria:** PDF/HTML export for closed mock mission.

---

## Phase 6 — Discipline plugins UI (ongoing)

Surface backend plugin flags as UI modules — no heavy logic in frontend.

| Plugin | UI surface |
|--------|------------|
| `water` | Boat sectors, sonar layer toggle |
| `k9` | Scent corridor drawing, handler assign |
| `drone` | Flight pattern picker (port `drone_module.py` patterns) |
| `maritime` | Vessel SAR Play defaults |

**Exit criteria:** Play template enables plugin panels; drone pattern sends mission spec to forge webhook.

---

## Phase 7 — Field hardening

- [ ] Offline sync conflict rules (timestamp + operator id)
- [ ] SMS SOS intake (SAR Command pattern) — separate microservice or forge tool
- [ ] ICS-201 PDF templates
- [ ] Installer / CI build for Tauri (Linux + Windows)
- [ ] Security pass: local PIN, no secrets in repo

---

## What not to build here

- Satellite SAR clustering, mag pipeline, SonarSniffer parsing
- Forge agent chat (link out to Mission Control / Forge UI)
- Full D4H Personnel/Equipment SKUs (Phase 7+ or separate repo)

---

## Suggested first sprint when you return

1. Read [RUST_CRATES.md](./RUST_CRATES.md) §1–4  
2. `cargo create-tauri-app` → `apps/tauri`  
3. One screen: **Intake** bound to SQLite via two Tauri commands  
4. Copy `schemas/case_intake.json` from sar_platform_core README  

---

## Links

| Resource | URL |
|----------|-----|
| D4H SAR sector | https://www.d4h.com/sectors/search-rescue-sar |
| D4H SAR feature walkthrough | https://www.d4h.com/blog/d4h-incident-management-for-search-rescue |
| FIND | https://www.findsar.com/ |
| SAR Command (offline UX) | https://sarcommand.app/ |
| Recovery + research | `../wreckhunter2000-1/todo/sar-platform-recovery/RESEARCH_NOTE.md` |
