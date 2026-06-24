# Architecture — SAR Command Frontend

## Recommended stack

| Layer | Choice | Why |
|-------|--------|-----|
| **Shell** | [Tauri 2](https://tauri.app/) | Offline desktop + optional web deploy; matches existing wreck Tauri pattern |
| **UI** | **Leptos** (Rust in WebView) *or* **Vite + TypeScript** (faster map ecosystem) | Leptos = one language; TS + Leaflet = faster maps/ICS PDF parity with SAR Command |
| **Map** | MapLibre GL or Leaflet (in WebView) | Industry standard; Rust `geo`/`geojson` for geometry only |
| **Local data** | `rusqlite` + SQL migrations | Offline-first mission DB (cases, clues, assignments) |
| **Sync queue** | `redb` or SQLite outbox table | Retry POST when link returns |
| **API** | `cesarops-platform` (Axum) or embed routes in Tauri sidecar | Reuse dispatch/admin/mapping modules from recovery |
| **Realtime** | Axum WebSocket | Mission Updates log (D4H-style audit stream) |

**Default recommendation:** Tauri 2 + **TypeScript UI** for Phase 1–3 (maps and forms move faster), Rust in `src-tauri/` for SQLite, sync, and PDF. Port hot paths to Leptos later if you want all-Rust UI.

## Crate boundaries

```
cesarops-sar-command/          ← this repo (UI + local store + Tauri)
        │
        ├─ HTTP/WS ──► cesarops-platform (fleet)  :9200
        │
        └─ webhooks ─► cesarops-forge-v2           :9100  (detection missions only)
```

Detection results arrive as **GeoJSON layers** on the map — not computed inside this repo.

## Screens (MVP)

1. **Intake** — case + subject + contacts  
2. **Mission board** — Play template, status, weather strip  
3. **Map** — LKP, assignments, clues, drift overlay (from API)  
4. **Updates** — chronological log, role tags  
5. **Teams** — resources, dispatch, GPS (when online)  
6. **Reports** — family / SAR team / IC export  

Reference UX: [D4H SAR walkthrough](https://www.d4h.com/blog/d4h-incident-management-for-search-rescue), [SAR Command](https://sarcommand.app/) offline patterns.
