//! sar-coverage PLAN.json OUT_DIR
//!
//! PLAN: {"segments": "segments.geojson", "cell_m": 10, "wind": {"from_deg": 270, "speed_ms": 3},
//!        "knobs": {...overrides per kind...},
//!        "resources": [{"id": "k9-mack", "kind": "k9_air_scent", "label": "Mack (air-scent)",
//!                        "track": "mack.gpx", "knobs": {}}, {"id": "drone-1", "kind": "drone", "track": "DJI_0001.SRT"}]}
//! Kinds: drone, k9_air_scent, k9_trailing, k9_tracking, foot, line_search, side_scan, diver.
//! Each resource has "track" (GPX/KML/CSV/DJI .SRT; SonarSniffer's track.kml for side_scan) or
//! "pattern" (circular | jackstay | lawnmower | expanding_square; see pattern.rs). Paths are relative to the plan.
//! OUT: coverage_cells.geojson, segments.geojson, summary.json (per-segment POD/POA, trail evidence,
//!      the knobs used). Default knobs are placeholders until calibrated: summary says so.

use sar_coverage::{compute, default_knobs, segments_from_geojson, trail_evidence, track, Kind, Resource, Wind};
use serde_json::{json, Value};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: sar-coverage PLAN.json OUT_DIR   (or: sar-coverage knobs  to print default knobs)");
        if a.get(1).map(String::as_str) == Some("knobs") {
            println!("{}", serde_json::to_string_pretty(&default_knobs())?);
        }
        std::process::exit(if a.get(1).map(String::as_str) == Some("knobs") { 0 } else { 2 });
    }
    let plan_path = Path::new(&a[1]);
    let base = plan_path.parent().unwrap_or(Path::new("."));
    let plan: Value = serde_json::from_slice(&std::fs::read(plan_path)?)?;
    let segs_v: Value = serde_json::from_slice(&std::fs::read(base.join(plan["segments"].as_str().ok_or("plan needs segments")?))?)?;
    let segments = segments_from_geojson(&segs_v);
    let wind: Option<Wind> = serde_json::from_value(plan["wind"].clone()).ok();
    let mut resources = Vec::new();
    for r in plan["resources"].as_array().ok_or("plan needs resources")? {
        let kind: Kind = serde_json::from_value(r["kind"].clone())?;
        // a recorded track, or a search pattern (dive teams on a rope, planned drone legs)
        let pts = if let Some(t) = r["track"].as_str() {
            let tp = base.join(t);
            let pts = track::read_track(&tp)?;
            eprintln!("{}: {} points from {}", r["id"].as_str().unwrap_or("?"), pts.len(), tp.display());
            pts
        } else if !r["pattern"].is_null() {
            let pat: sar_coverage::pattern::Pattern = serde_json::from_value(r["pattern"].clone())?;
            let pts = sar_coverage::pattern::to_track(&pat, r["alt_agl"].as_f64());
            eprintln!("{}: {} points from a {} pattern", r["id"].as_str().unwrap_or("?"), pts.len(), r["pattern"]["type"].as_str().unwrap_or("?"));
            pts
        } else {
            return Err("resource needs track or pattern".into());
        };
        resources.push(Resource {
            id: r["id"].as_str().unwrap_or("resource").to_string(),
            kind,
            label: r["label"].as_str().unwrap_or("").to_string(),
            track: pts,
            knobs: r["knobs"].clone(),
        });
    }
    let cell = plan["cell_m"].as_f64().unwrap_or(10.0);
    let cov = compute(&segments, &resources, &plan["knobs"], wind, cell);
    let out = Path::new(&a[2]);
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("coverage_cells.geojson"), serde_json::to_vec(&cov.cells_geojson(0.01))?)?;
    std::fs::write(out.join("segments.geojson"), serde_json::to_vec(&cov.segments_geojson())?)?;
    let evidence: Vec<Value> = resources.iter().filter_map(trail_evidence).collect();
    let summary = json!({
        "segments": cov.segments, "trail_evidence": evidence, "wind": plan["wind"], "cell_m": cell,
        "knobs_defaults": default_knobs(), "knobs_overrides": plan["knobs"],
        "status": "PLANNING AID: POD uses placeholder sweep knobs until calibrated on real dog/drone tracks; not a measured result",
    });
    std::fs::write(out.join("summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("{:<12} {:>6} {:>8} {:>9} {:>10} {:>10}", "segment", "cells", "POD", "covered", "POA before", "POA after");
    for s in &cov.segments {
        println!("{:<12} {:>6} {:>7.0}% {:>8.0}% {:>9.0}% {:>9.0}%", s.id, s.cells, 100.0 * s.pod, s.covered_pct, 100.0 * s.poa_before, 100.0 * s.poa_after);
    }
    Ok(())
}
