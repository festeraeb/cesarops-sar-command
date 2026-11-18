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

## Field Command Unit web host (2026-11-18)

`apps/server` (axum, crate `fcu-server`) exposes every Tauri command as `POST api/call/<cmd>` (same args object
as `invoke`) and serves the built front end. Shared command logic lives in `crates/command-api`, called by both
the Tauri shell and the web host. Capabilities are modules (`crates/command-api/src/modules.rs`): each registers
id, name, version and what it provides, and is reached through an adapter with a local (offline) or cloud (HTTP)
transport. A team enables modules in the config file (`"modules": {"enabled": [...]}`); the UI shows only
enabled modules. Anything not working is shown as "not available yet" (SonarSniffer today).

```sh
cd apps/web && npm install && npm run build           # front end (relative base, works under any path)
cargo build --release -p fcu-server
fcu-server deploy/demo.config.json                    # public demo: in-memory sessions, made-up incidents
fcu-server deploy/tester.config.json                  # testers: per-user SQLite stores, user from X-Sar-User
```

`deploy/` holds the configs, systemd units and nginx snippet used at cesarops.com/sar-demo/ and /sar-test/.
