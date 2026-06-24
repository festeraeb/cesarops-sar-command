# Rust crates for SAR Command UI

Curated for **offline command post + map + ICS workflow**. Use Rust for shell, storage, sync, geometry, and PDF; use JS map libs inside the WebView unless you adopt Leptos full-stack.

---

## 1. Application shell (desktop)

| Crate | Use |
|-------|-----|
| [`tauri`](https://docs.rs/tauri) v2 | Desktop shell, system tray, deep links |
| [`tauri-plugin-dialog`](https://docs.rs/tauri-plugin-dialog) | File pickers (attachments, GeoTIFF export) |
| [`tauri-plugin-shell`](https://docs.rs/tauri-plugin-shell) | Open PDF / external browser (family share link) |
| [`tauri-plugin-http`](https://docs.rs/tauri-plugin-http) | Controlled HTTP from UI when not using Rust IPC |
| [`tauri-plugin-notification`](https://docs.rs/tauri-plugin-notification) | New clue / SOS location alerts |
| [`tauri-plugin-store`](https://docs.rs/tauri-plugin-store) | Small settings (API base URL, operator prefs) |

Existing fleet reference: `cesarops-core/tauri` uses Tauri 2 + `serde` + `chrono`.

---

## 2. UI in Rust (optional — all-in-Rust WebView)

| Crate | Use |
|-------|-----|
| [`leptos`](https://docs.rs/leptos) | Reactive UI; SSR or CSR in Tauri |
| [`leptos_router`](https://docs.rs/leptos_router) | Intake / map / reports routes |
| [`leptos_meta`](https://docs.rs/leptos_meta) | Document title per mission |
| [`dioxus`](https://docs.rs/dioxus) | Alternative to Leptos (desktop + web) |
| [`icondata`](https://docs.rs/icondata) | Icons without npm |

**Maps in pure Rust UI:** still embed Leaflet/MapLibre via `web-sys` or an `<iframe>` — no mature native Rust map widget for ICS-grade GIS.

---

## 3. Embedded server (LAN team mode / sidecar)

| Crate | Use |
|-------|-----|
| [`axum`](https://docs.rs/axum) | REST + WebSocket (already in `cesarops-platform`) |
| [`tower`](https://docs.rs/tower) + [`tower-http`](https://docs.rs/tower-http) | CORS, compression, request tracing |
| [`tokio`](https://docs.rs/tokio) | Async runtime |
| [`hyper`](https://docs.rs/hyper) | HTTP (via axum) |

Reuse modules from `wreckhunter2000-1/cesarops-platform`: `admin`, `dispatch`, `mapping`, `tracking`, `reporting`, `plugins`.

---

## 4. Offline storage & sync

| Crate | Use |
|-------|-----|
| [`rusqlite`](https://docs.rs/rusqlite) | Mission DB on disk (`cases`, `clues`, `assignments`, `updates`) |
| [`sqlx`](https://docs.rs/sqlx) | Same schema if you prefer async + migrations |
| [`refinery`](https://docs.rs/refinery) or [`sqlx migrate`](https://docs.rs/sqlx) | Schema versioning |
| [`redb`](https://docs.rs/redb) | Embedded KV for sync outbox / conflict metadata |
| [`serde`](https://docs.rs/serde) + [`serde_json`](https://docs.rs/serde_json) | API + Play templates |
| [`uuid`](https://docs.rs/uuid) | Case / clue IDs |
| [`chrono`](https://docs.rs/chrono) | Timestamps (ICS logs) |

**Later (multi-operator sync):** [`yrs`](https://docs.rs/yrs) CRDT for collaborative Updates log; or server-authoritative merge like SAR Command’s IndexedDB sync.

---

## 5. Geospatial (Rust side — map rendering stays JS)

| Crate | Use |
|-------|-----|
| [`geo`](https://docs.rs/geo) | Points, polygons, boolean ops (sector overlap) |
| [`geojson`](https://docs.rs/geojson) | Clue/assignment layers to/from map |
| [`geo-types`](https://docs.rs/geo-types) | Lightweight geometry (often re-exported by `geo`) |
| [`h3o`](https://docs.rs/h3o) | Hex search sectors (optional) |
| [`gpx`](https://docs.rs/gpx) | Import team GPS tracks |
| [`proj`](https://docs.rs/proj) | Reproject if you ingest survey grids |
| [`haversine`](https://docs.rs/haversine) or `geo::HaversineDistance` | Quick range checks |

**LPB probability rings:** no standard crate — port Koester tables or call FIND-style service; draw rings as GeoJSON polygons from Rust.

**Drift overlay:** consume GeoJSON from `cesarops` drift engine API; don’t reimplement in UI repo.

---

## 6. Search theory (POD / allocation)

| Crate | Use |
|-------|-----|
| [`nalgebra`](https://docs.rs/nalgebra) | POD matrices, segment scoring |
| Custom module | Port `cesarops-platform/src/search_theory.rs` (already has POD helpers) |

---

## 7. Reports & export

| Crate | Use |
|-------|-----|
| [`tera`](https://docs.rs/tera) | HTML templates (family / team / IC reports) |
| [`printpdf`](https://docs.rs/printpdf) or [`genpdf`](https://docs.rs/genpdf) | PDF export (ICS forms) |
| [`typst`](https://docs.rs/typst) | Alternative: compile `.typ` templates via CLI |
| [`zip`](https://docs.rs/zip) | Incident package bundle (maps + logs + photos) |

Port logic from recovery: `python-prototype/reports/incident_report.py` → Rust templates.

---

## 8. Auth & security

| Crate | Use |
|-------|-----|
| [`tower-sessions`](https://docs.rs/tower-sessions) | Session cookies (LAN server mode) |
| [`jsonwebtoken`](https://docs.rs/jsonwebtoken) | Token auth if fronting forge |
| [`secrecy`](https://docs.rs/secrecy) | API keys in Tauri secure store |
| [`argon2`](https://docs.rs/argon2) | Local operator PIN (command post kiosk) |

---

## 9. Observability

| Crate | Use |
|-------|-----|
| [`tracing`](https://docs.rs/tracing) | Structured logs |
| [`tracing-subscriber`](https://docs.rs/tracing-subscriber) | Env filter |

---

## 10. Workspace layout (when you scaffold)

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
  "crates/command-types",   # SarCase, Clue, Assignment, Play — shared serde types
  "crates/command-db",      # rusqlite + migrations
  "crates/command-geo",     # geojson helpers, ring generation
  "crates/command-reports", # tera + pdf
  "apps/tauri",             # Tauri binary
  "apps/sidecar",           # optional axum LAN server (thin wrap of cesarops-platform)
]
```

**Dependency rule:** `command-types` has no I/O; UI and Tauri depend on `command-db` + `command-geo`; detection crates stay in other repos.

---

## Crates to skip (for this product)

| Crate | Why skip |
|-------|----------|
| `egui` / `iced` | Poor map/ICS form story vs WebView + Leaflet |
| `gtk4` | Heavier than Tauri for same outcome |
| GDAL bindings in UI repo | Keep raster work in `cesarops-satellite` |
| Full `bevy` | Not a game |

---

## Quick copy-paste starter `Cargo.toml` (Tauri app)

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
rusqlite = { version = "0.32", features = ["bundled"] }
geo = "0.28"
geojson = "0.24"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
tracing = "0.1"
anyhow = "1"
```

Add `axum` + `reqwest` when wiring to `cesarops-platform`.
