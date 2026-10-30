# cesarops-sar-command

Frontend and command-post UI for CESAROPS SAR incident management.

**Scope:** call intake, mission map, team dispatch, ICS workspace, stakeholder reports (family / team / IC).  
**Out of scope:** satellite/mag/sonar detection pipelines (those stay in `wreckhunter2000-1` / `cesarops-server`).

## Related repos & recovery

| Path | Role |
|------|------|
| `wreckhunter2000-1/todo/sar-platform-recovery/` | Recovered Python/Rust prototypes + [D4H research](./docs/LINKS.md) |
| `wreckhunter2000-1/cesarops-platform/` | Rust Axum backend skeleton (`:9200`) — future API target |
| `laptop-code/.../cesarops-core/tauri/` | Wreck desktop pattern (Tauri 2); **not** this product |

## Docs (start here)

1. **[docs/ACTION_PLAN.md](docs/ACTION_PLAN.md)** — phased plan to return to
2. **[docs/RUST_CRATES.md](docs/RUST_CRATES.md)** — recommended crates by layer
3. **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — stack choice and boundaries

## Run the application

Web test build:

```sh
cd apps/web
npm install
npm run dev
```

Desktop build (requires the Tauri 2 prerequisites):

```sh
cd apps/tauri/src-tauri
cargo tauri dev
```

The same TypeScript UI runs at `/adminpanel/` as a web application. In a
browser it uses an offline local store; in Tauri it uses the embedded SQLite
database through typed commands.

## Implemented

- SAR intake, mission dashboard, updates/clues, team roster, and stakeholder reports
- Offline SQLite schema and case JSON export
- Live drift/detection GeoJSON overlays on the mission map
- Truth-gated HTML and PDF report generation
- Tauri 2 desktop shell plus Vite web deployment
