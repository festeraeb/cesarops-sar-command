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

## Status

Planning repo only — no application code yet. Scaffold `apps/` and `crates/` when Phase 1 starts.
