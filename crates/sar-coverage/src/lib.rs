//! sar-coverage: how well each search segment has been covered, by drones and dogs together
//! (operator 2026-11-15: "drone and dog search together, as they share a lot of the same covered area").
//!
//! Every resource's track becomes a per-cell probability of detection (POD) on one grid. Within one
//! pass a cell counts once (the best look); separate passes combine as independent looks:
//! POD = 1 − Π(1 − PODᵢ). Segment POD is the mean over its cells; probability of area (POA) is updated
//! the standard way: POA′ᵢ = POAᵢ(1 − PODᵢ) / (1 − Σ POAⱼ·PODⱼ).
//!
//! Sweep models (all weights are knobs; the defaults are PLACEHOLDER starting values, not validated):
//! - Drone: the camera's ground footprint per telemetry record (altitude, gimbal, field of view); POD
//!   from the pixels a person would cover at that range.
//! - K9 air-scent: an area search for any human scent; detection reaches UPWIND of the dog, in a cone
//!   whose reach depends on wind speed.
//! - K9 trailing: follows one person's scent, which drifts and pools, so the person's true path is
//!   likely offset UPWIND of where the dog works; a corridor shifted upwind.
//! - K9 tracking: follows the footstep track itself; a narrow corridor on the path.
//! - Foot searcher: a corridor of the sweep width.

pub mod track;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use track::TrackPoint;

// ── geometry ───────────────────────────────────────────────

/// Local metric frame (east, north) around an origin; fine at search-area scale (tens of km).
#[derive(Debug, Clone, Copy)]
pub struct Proj {
    lat0: f64,
    lon0: f64,
    kx: f64,
    ky: f64,
}

impl Proj {
    pub fn new(lat0: f64, lon0: f64) -> Self {
        Self { lat0, lon0, kx: 111_320.0 * lat0.to_radians().cos(), ky: 110_540.0 }
    }
    pub fn fwd(&self, lat: f64, lon: f64) -> (f64, f64) {
        ((lon - self.lon0) * self.kx, (lat - self.lat0) * self.ky)
    }
    pub fn inv(&self, x: f64, y: f64) -> (f64, f64) {
        (self.lat0 + y / self.ky, self.lon0 + x / self.kx)
    }
}

fn point_in_poly(x: f64, y: f64, ring: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (xi, yi) = ring[i];
        let (xj, yj) = ring[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Compass bearing (degrees, 0 = north, clockwise) to an (east, north) unit vector.
fn bearing_vec(deg: f64) -> (f64, f64) {
    let r = deg.to_radians();
    (r.sin(), r.cos())
}

// ── inputs ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    /// Outer ring as (lat, lon).
    pub ring: Vec<(f64, f64)>,
    /// Prior probability of area; segments without one share what is left equally.
    #[serde(default)]
    pub poa: Option<f64>,
}

/// Segments from a GeoJSON FeatureCollection of Polygons (property `id` or `name`, optional `poa`).
pub fn segments_from_geojson(v: &Value) -> Vec<Segment> {
    let mut out = Vec::new();
    for (i, f) in v["features"].as_array().into_iter().flatten().enumerate() {
        if f["geometry"]["type"] != "Polygon" {
            continue;
        }
        let ring: Vec<(f64, f64)> = f["geometry"]["coordinates"][0]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| Some((c[1].as_f64()?, c[0].as_f64()?)))
            .collect();
        if ring.len() < 3 {
            continue;
        }
        let p = &f["properties"];
        let id = p["id"].as_str().or(p["name"].as_str()).map(String::from).unwrap_or_else(|| format!("S{}", i + 1));
        out.push(Segment { id, ring, poa: p["poa"].as_f64() });
    }
    out
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Wind {
    /// Direction the wind blows FROM, degrees true.
    pub from_deg: f64,
    pub speed_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Drone,
    K9AirScent,
    K9Trailing,
    K9Tracking,
    Foot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub kind: Kind,
    #[serde(default)]
    pub label: String,
    #[serde(skip)]
    pub track: Vec<TrackPoint>,
    /// Per-resource knob overrides (merged over the plan's knobs for this kind).
    #[serde(default)]
    pub knobs: Value,
}

/// Starting values for every sweep model. PLACEHOLDERS until calibrated on real tracks and finds.
pub fn default_knobs() -> Value {
    json!({
        "_note": "placeholder starting values, not validated; calibrate on real dog/drone tracks before relying on POD numbers",
        "drone": {"hfov_deg": 72.0, "aspect": 1.7778, "image_w_px": 3840.0, "target_m": 0.5,
                  "px_min": 4.0, "px_full": 12.0, "pod_max": 0.8, "max_ground_m": 300.0, "assume_nadir": true},
        "k9_air_scent": {"near_radius_m": 15.0, "cone_deg": 60.0, "pod_max": 0.7,
                         "reach_calm_m": 40.0, "reach_good_m": 100.0, "reach_strong_m": 60.0,
                         "calm_below_ms": 0.5, "strong_above_ms": 6.0, "no_wind_radius_m": 30.0},
        "k9_trailing": {"half_width_m": 15.0, "upwind_offset_m": 10.0, "pod": 0.6},
        "k9_tracking": {"half_width_m": 3.0, "pod": 0.7},
        "foot": {"half_width_m": 10.0, "pod": 0.5}
    })
}

fn kind_key(k: Kind) -> &'static str {
    match k {
        Kind::Drone => "drone",
        Kind::K9AirScent => "k9_air_scent",
        Kind::K9Trailing => "k9_trailing",
        Kind::K9Tracking => "k9_tracking",
        Kind::Foot => "foot",
    }
}

fn knob(k: &Value, name: &str, dflt: f64) -> f64 {
    k[name].as_f64().unwrap_or(dflt)
}

// ── grid ───────────────────────────────────────────────────

pub struct Grid {
    pub proj: Proj,
    pub cell: f64,
    pub x0: f64,
    pub y0: f64,
    pub nx: usize,
    pub ny: usize,
    /// Probability every look so far missed, per cell (1 = untouched).
    miss: Vec<f64>,
    /// Best single-pass POD per resource, per cell (for attribution).
    by_res: BTreeMap<String, Vec<f32>>,
}

impl Grid {
    fn center(&self, ix: usize, iy: usize) -> (f64, f64) {
        (self.x0 + (ix as f64 + 0.5) * self.cell, self.y0 + (iy as f64 + 0.5) * self.cell)
    }
    pub fn pod(&self, ix: usize, iy: usize) -> f64 {
        1.0 - self.miss[iy * self.nx + ix]
    }
    /// Cells whose centres fall in an axis box (local metres).
    fn cells_in(&self, minx: f64, miny: f64, maxx: f64, maxy: f64) -> impl Iterator<Item = (usize, usize)> + '_ {
        let ix0 = (((minx - self.x0) / self.cell).floor().max(0.0)) as usize;
        let iy0 = (((miny - self.y0) / self.cell).floor().max(0.0)) as usize;
        let ix1 = ((((maxx - self.x0) / self.cell).ceil()) as isize).clamp(0, self.nx as isize) as usize;
        let iy1 = ((((maxy - self.y0) / self.cell).ceil()) as isize).clamp(0, self.ny as isize) as usize;
        (iy0..iy1).flat_map(move |iy| (ix0..ix1).map(move |ix| (ix, iy)))
    }
}

/// Track points in local metres, densified so consecutive points are at most `step` apart.
fn densify(proj: &Proj, pts: &[TrackPoint], step: f64) -> Vec<(f64, f64, TrackPoint)> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let (ax, ay) = proj.fwd(a.lat, a.lon);
        let (bx, by) = proj.fwd(b.lat, b.lon);
        let d = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let n = ((d / step).ceil() as usize).max(1);
        for k in 0..n {
            let f = k as f64 / n as f64;
            let mut p = a.clone();
            if let (Some(ta), Some(tb)) = (a.t, b.t) {
                p.t = Some(ta + (tb - ta) * f);
            }
            out.push((ax + (bx - ax) * f, ay + (by - ay) * f, p));
        }
    }
    if let Some(l) = pts.last() {
        let (x, y) = proj.fwd(l.lat, l.lon);
        out.push((x, y, l.clone()));
    }
    out
}

fn pass_set(pass: &mut [f32], nx: usize, ix: usize, iy: usize, pod: f64) {
    let c = &mut pass[iy * nx + ix];
    if (pod as f32) > *c {
        *c = pod as f32;
    }
}

/// One resource's pass over the grid: best POD per cell for this pass.
fn sweep(g: &Grid, r: &Resource, k: &Value, wind: Option<Wind>) -> Vec<f32> {
    let mut pass = vec![0f32; g.nx * g.ny];
    let step = (g.cell / 2.0).max(1.0);
    let pts = densify(&g.proj, &r.track, step);
    match r.kind {
        Kind::Drone => {
            let hfov = knob(k, "hfov_deg", 72.0).to_radians();
            let vfov = 2.0 * ((hfov / 2.0).tan() / knob(k, "aspect", 16.0 / 9.0)).atan();
            let (imgw, target) = (knob(k, "image_w_px", 3840.0), knob(k, "target_m", 0.5));
            let (pxmin, pxfull, podmax, maxg) = (knob(k, "px_min", 4.0), knob(k, "px_full", 12.0), knob(k, "pod_max", 0.8), knob(k, "max_ground_m", 300.0));
            for (x, y, p) in &pts {
                let Some(h) = p.alt_agl.filter(|h| *h > 1.0) else { continue };
                let pitch = p.gimbal_pitch.unwrap_or(-90.0); // nadir unless told otherwise
                let tilt = (90.0 + pitch).clamp(0.0, 80.0).to_radians(); // 0 = straight down
                let yaw = p.gimbal_yaw.unwrap_or(0.0);
                // ground distance (along the camera heading) of the near and far image edges
                let near = if tilt < vfov / 2.0 { -h * (vfov / 2.0 - tilt).tan() } else { h * (tilt - vfov / 2.0).tan() };
                let far = if tilt + vfov / 2.0 >= std::f64::consts::FRAC_PI_2 * 0.99 { maxg } else { (h * (tilt + vfov / 2.0).tan()).min(maxg) };
                let slant = h / tilt.cos();
                let half_w = slant * (hfov / 2.0).tan();
                let gsd = 2.0 * half_w / imgw;
                let px = target / gsd;
                let pod = podmax * ((px - pxmin) / (pxfull - pxmin)).clamp(0.0, 1.0);
                if pod <= 0.0 {
                    continue;
                }
                let (fx, fy) = bearing_vec(yaw); // forward
                let (rx, ry) = (fy, -fx); // right
                let r = far.abs().max(near.abs()).max(half_w) + g.cell;
                for (ix, iy) in g.cells_in(x - r, y - r, x + r, y + r) {
                    let (cx, cy) = g.center(ix, iy);
                    let (dx, dy) = (cx - x, cy - y);
                    let along = dx * fx + dy * fy;
                    let across = dx * rx + dy * ry;
                    if along >= near && along <= far && across.abs() <= half_w {
                        pass_set(&mut pass, g.nx, ix, iy, pod);
                    }
                }
            }
        }
        Kind::K9AirScent => {
            let podmax = knob(k, "pod_max", 0.7);
            let near_r = knob(k, "near_radius_m", 15.0);
            let half_cone = (knob(k, "cone_deg", 60.0) / 2.0).to_radians();
            let reach = match wind {
                Some(w) if w.speed_ms < knob(k, "calm_below_ms", 0.5) => knob(k, "reach_calm_m", 40.0),
                Some(w) if w.speed_ms > knob(k, "strong_above_ms", 6.0) => knob(k, "reach_strong_m", 60.0),
                Some(_) => knob(k, "reach_good_m", 100.0),
                None => knob(k, "no_wind_radius_m", 30.0),
            };
            let up = wind.map(|w| bearing_vec(w.from_deg)); // toward where the wind comes from
            for (x, y, _) in &pts {
                let r = reach.max(near_r) + g.cell;
                for (ix, iy) in g.cells_in(x - r, y - r, x + r, y + r) {
                    let (cx, cy) = g.center(ix, iy);
                    let (dx, dy) = (cx - x, cy - y);
                    let dist = (dx * dx + dy * dy).sqrt();
                    let pod = if dist <= near_r {
                        podmax
                    } else if let Some((ux, uy)) = up {
                        let along = dx * ux + dy * uy;
                        let across = (dx * uy - dy * ux).abs();
                        if along > 0.0 && along <= reach && across <= near_r + along * half_cone.tan() {
                            podmax * (1.0 - along / reach)
                        } else {
                            0.0
                        }
                    } else if dist <= reach {
                        podmax * (1.0 - dist / reach)
                    } else {
                        0.0
                    };
                    if pod > 0.0 {
                        pass_set(&mut pass, g.nx, ix, iy, pod);
                    }
                }
            }
        }
        Kind::K9Trailing | Kind::K9Tracking | Kind::Foot => {
            let (hw, pod, offset) = match r.kind {
                Kind::K9Trailing => (knob(k, "half_width_m", 15.0), knob(k, "pod", 0.6), knob(k, "upwind_offset_m", 10.0)),
                Kind::K9Tracking => (knob(k, "half_width_m", 3.0), knob(k, "pod", 0.7), 0.0),
                _ => (knob(k, "half_width_m", 10.0), knob(k, "pod", 0.5), 0.0),
            };
            // trailing: the person's path is likely upwind of where the dog works the drifted scent
            let (ox, oy) = match (r.kind, wind) {
                (Kind::K9Trailing, Some(w)) => {
                    let (ux, uy) = bearing_vec(w.from_deg);
                    (ux * offset, uy * offset)
                }
                _ => (0.0, 0.0),
            };
            for (x, y, _) in &pts {
                let (x, y) = (x + ox, y + oy);
                let r = hw + g.cell;
                for (ix, iy) in g.cells_in(x - r, y - r, x + r, y + r) {
                    let (cx, cy) = g.center(ix, iy);
                    if ((cx - x).powi(2) + (cy - y).powi(2)).sqrt() <= hw {
                        pass_set(&mut pass, g.nx, ix, iy, pod);
                    }
                }
            }
        }
    }
    pass
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentResult {
    pub id: String,
    pub cells: usize,
    pub pod: f64,
    pub covered_pct: f64,
    pub poa_before: f64,
    pub poa_after: f64,
    /// Mean single-pass POD contributed by each resource inside this segment.
    pub by_resource: BTreeMap<String, f64>,
}

pub struct Coverage {
    pub grid: Grid,
    pub segments: Vec<SegmentResult>,
    seg_rings: Vec<(String, Vec<(f64, f64)>)>,
}

/// Run every resource over the segments.
pub fn compute(segments: &[Segment], resources: &[Resource], knobs: &Value, wind: Option<Wind>, cell_m: f64) -> Coverage {
    // grid over the segments (and tracks, so out-of-segment coverage still shows)
    let all: Vec<(f64, f64)> = segments.iter().flat_map(|s| s.ring.iter().copied()).chain(resources.iter().flat_map(|r| r.track.iter().map(|p| (p.lat, p.lon)))).collect();
    let (lat0, lon0) = if all.is_empty() { (0.0, 0.0) } else { (all.iter().map(|p| p.0).sum::<f64>() / all.len() as f64, all.iter().map(|p| p.1).sum::<f64>() / all.len() as f64) };
    let proj = Proj::new(lat0, lon0);
    let xy: Vec<(f64, f64)> = segments.iter().flat_map(|s| s.ring.iter().map(|(la, lo)| proj.fwd(*la, *lo))).collect();
    let pad = 50.0;
    let (minx, maxx) = xy.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (miny, maxy) = xy.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    let (minx, miny, maxx, maxy) = if xy.is_empty() { (-500.0, -500.0, 500.0, 500.0) } else { (minx - pad, miny - pad, maxx + pad, maxy + pad) };
    let nx = (((maxx - minx) / cell_m).ceil() as usize).clamp(1, 4000);
    let ny = (((maxy - miny) / cell_m).ceil() as usize).clamp(1, 4000);
    let mut g = Grid { proj, cell: cell_m, x0: minx, y0: miny, nx, ny, miss: vec![1.0; nx * ny], by_res: BTreeMap::new() };
    let defaults = default_knobs();
    for r in resources {
        let key = kind_key(r.kind);
        let mut k = defaults[key].clone();
        for src in [&knobs[key], &r.knobs] {
            if let (Some(dst), Some(o)) = (k.as_object_mut(), src.as_object()) {
                for (n, v) in o {
                    dst.insert(n.clone(), v.clone());
                }
            }
        }
        let pass = sweep(&g, r, &k, wind);
        for (m, p) in g.miss.iter_mut().zip(&pass) {
            *m *= 1.0 - *p as f64;
        }
        g.by_res.insert(r.id.clone(), pass);
    }
    // segments
    let seg_rings: Vec<(String, Vec<(f64, f64)>)> = segments.iter().map(|s| (s.id.clone(), s.ring.iter().map(|(la, lo)| g.proj.fwd(*la, *lo)).collect())).collect();
    let given: f64 = segments.iter().filter_map(|s| s.poa).sum();
    let unspecified = segments.iter().filter(|s| s.poa.is_none()).count();
    let share = if unspecified > 0 { ((1.0 - given).max(0.0)) / unspecified as f64 } else { 0.0 };
    let mut results = Vec::new();
    for (s, (_, ring)) in segments.iter().zip(&seg_rings) {
        let (mut n, mut sum, mut covered) = (0usize, 0.0, 0usize);
        let mut by: BTreeMap<String, f64> = BTreeMap::new();
        for iy in 0..g.ny {
            for ix in 0..g.nx {
                let (cx, cy) = g.center(ix, iy);
                if !point_in_poly(cx, cy, ring) {
                    continue;
                }
                n += 1;
                let p = g.pod(ix, iy);
                sum += p;
                if p >= 0.05 {
                    covered += 1;
                }
                for (rid, pass) in &g.by_res {
                    *by.entry(rid.clone()).or_default() += pass[iy * g.nx + ix] as f64;
                }
            }
        }
        let pod = if n > 0 { sum / n as f64 } else { 0.0 };
        for v in by.values_mut() {
            *v = if n > 0 { *v / n as f64 } else { 0.0 };
        }
        results.push(SegmentResult { id: s.id.clone(), cells: n, pod, covered_pct: if n > 0 { 100.0 * covered as f64 / n as f64 } else { 0.0 }, poa_before: s.poa.unwrap_or(share), poa_after: 0.0, by_resource: by });
    }
    let pos_total: f64 = results.iter().map(|r| r.poa_before * r.pod).sum();
    for r in &mut results {
        r.poa_after = if pos_total < 1.0 { r.poa_before * (1.0 - r.pod) / (1.0 - pos_total) } else { 0.0 };
    }
    Coverage { grid: g, segments: results, seg_rings }
}

impl Coverage {
    /// Grid cells with any coverage as GeoJSON squares (property `pod`, plus the top resource).
    pub fn cells_geojson(&self, min_pod: f64) -> Value {
        let g = &self.grid;
        let mut feats = Vec::new();
        for iy in 0..g.ny {
            for ix in 0..g.nx {
                let p = g.pod(ix, iy);
                if p < min_pod {
                    continue;
                }
                let (x, y) = (g.x0 + ix as f64 * g.cell, g.y0 + iy as f64 * g.cell);
                let corners = [(x, y), (x + g.cell, y), (x + g.cell, y + g.cell), (x, y + g.cell), (x, y)];
                let ring: Vec<Value> = corners.iter().map(|(cx, cy)| { let (la, lo) = g.proj.inv(*cx, *cy); json!([lo, la]) }).collect();
                let top = g.by_res.iter().max_by(|a, b| a.1[iy * g.nx + ix].partial_cmp(&b.1[iy * g.nx + ix]).unwrap_or(std::cmp::Ordering::Equal)).map(|(k, _)| k.clone());
                feats.push(json!({"type": "Feature", "properties": {"pod": (p * 1000.0).round() / 1000.0, "top_resource": top},
                                  "geometry": {"type": "Polygon", "coordinates": [ring]}}));
            }
        }
        json!({"type": "FeatureCollection", "features": feats})
    }

    /// Segments with their results as GeoJSON.
    pub fn segments_geojson(&self) -> Value {
        let feats: Vec<Value> = self
            .segments
            .iter()
            .zip(&self.seg_rings)
            .map(|(r, (_, ring))| {
                let coords: Vec<Value> = ring.iter().map(|(x, y)| { let (la, lo) = self.grid.proj.inv(*x, *y); json!([lo, la]) }).collect();
                json!({"type": "Feature", "properties": serde_json::to_value(r).unwrap_or_default(), "geometry": {"type": "Polygon", "coordinates": [coords]}})
            })
            .collect();
        json!({"type": "FeatureCollection", "features": feats})
    }
}

/// Trailing/tracking evidence: where the dog's line went and which way it was heading at the end.
pub fn trail_evidence(r: &Resource) -> Option<Value> {
    if !matches!(r.kind, Kind::K9Trailing | Kind::K9Tracking) || r.track.len() < 2 {
        return None;
    }
    let (a, b) = (r.track.first()?, r.track.last()?);
    // heading over the last ~10 points
    let tail = &r.track[r.track.len().saturating_sub(10)..];
    let proj = Proj::new(b.lat, b.lon);
    let (x0, y0) = proj.fwd(tail[0].lat, tail[0].lon);
    let (x1, y1) = proj.fwd(b.lat, b.lon);
    let heading = (x1 - x0).atan2(y1 - y0).to_degrees().rem_euclid(360.0);
    Some(json!({"resource": r.id, "kind": kind_key(r.kind), "start": [a.lat, a.lon], "end": [b.lat, b.lon],
                "end_heading_deg": (heading * 10.0).round() / 10.0, "start_time": a.t, "end_time": b.t,
                "note": "where the dog's line ended and the direction it was travelling; a handler marks whether scent was lost or the dog was pulled"}))
}

/// The track position at a time (linear between samples), to place a camera frame on the map.
pub fn position_at(track: &[TrackPoint], t: f64) -> Option<(f64, f64)> {
    let w = track.windows(2).find(|w| matches!((w[0].t, w[1].t), (Some(a), Some(b)) if a <= t && t <= b))?;
    let (ta, tb) = (w[0].t?, w[1].t?);
    let f = if tb > ta { (t - ta) / (tb - ta) } else { 0.0 };
    Some((w[0].lat + (w[1].lat - w[0].lat) * f, w[0].lon + (w[1].lon - w[0].lon) * f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(id: &str, lat: f64, lon: f64, half_m: f64) -> Segment {
        let dlat = half_m / 110_540.0;
        let dlon = half_m / (111_320.0 * lat.to_radians().cos());
        Segment { id: id.into(), ring: vec![(lat - dlat, lon - dlon), (lat - dlat, lon + dlon), (lat + dlat, lon + dlon), (lat + dlat, lon - dlon)], poa: None }
    }
    fn line(lat: f64, lon0: f64, lon1: f64, n: usize) -> Vec<TrackPoint> {
        (0..n).map(|i| TrackPoint { lat, lon: lon0 + (lon1 - lon0) * i as f64 / (n - 1) as f64, t: Some(i as f64), ..Default::default() }).collect()
    }

    #[test]
    fn independent_passes_combine() {
        let seg = vec![square("A", 42.0, -86.0, 100.0)];
        let t = line(42.0, -86.002, -85.998, 20);
        let r1 = Resource { id: "f1".into(), kind: Kind::Foot, label: String::new(), track: t.clone(), knobs: json!({"half_width_m": 300.0, "pod": 0.5}) };
        let r2 = Resource { id: "f2".into(), kind: Kind::Foot, label: String::new(), track: t, knobs: json!({"half_width_m": 300.0, "pod": 0.5}) };
        let one = compute(&seg, &[r1.clone()], &json!({}), None, 10.0);
        let two = compute(&seg, &[r1, r2], &json!({}), None, 10.0);
        assert!((one.segments[0].pod - 0.5).abs() < 1e-6);
        assert!((two.segments[0].pod - 0.75).abs() < 1e-6, "1-(1-.5)^2");
    }

    #[test]
    fn air_scent_reaches_upwind_not_downwind() {
        // dog walks east-west along the centre line; wind FROM the north
        let up = square("north", 42.0006, -86.0, 25.0); // ~66 m north of the line
        let down = square("south", 41.9994, -86.0, 25.0);
        let dog = Resource { id: "k9".into(), kind: Kind::K9AirScent, label: String::new(), track: line(42.0, -86.002, -85.998, 30), knobs: json!({}) };
        let c = compute(&[up, down], &[dog], &json!({}), Some(Wind { from_deg: 0.0, speed_ms: 3.0 }), 5.0);
        let (n, s) = (c.segments[0].pod, c.segments[1].pod);
        assert!(n > 0.05 && s < 1e-9, "upwind {n}, downwind {s}");
    }

    #[test]
    fn trailing_corridor_shifts_upwind_and_tracking_is_narrow() {
        let seg = vec![square("A", 42.0, -86.0, 200.0)];
        let t = line(42.0, -86.002, -85.998, 40);
        let trail = Resource { id: "tr".into(), kind: Kind::K9Trailing, label: String::new(), track: t.clone(), knobs: json!({}) };
        let trk = Resource { id: "tk".into(), kind: Kind::K9Tracking, label: String::new(), track: t, knobs: json!({}) };
        let a = compute(&seg, &[trail.clone()], &json!({}), Some(Wind { from_deg: 0.0, speed_ms: 3.0 }), 2.0);
        let b = compute(&seg, &[trk], &json!({}), None, 2.0);
        assert!(a.segments[0].covered_pct > b.segments[0].covered_pct * 3.0, "trailing corridor wider than tracking");
        // the trailing corridor's centre is north (upwind) of the dog's line
        let g = &a.grid;
        let (_, y_line) = g.proj.fwd(42.0, -86.0);
        let iy_line = ((y_line - g.y0) / g.cell) as usize;
        let ix = g.nx / 2;
        let north = g.pod(ix, iy_line + 12); // ~24 m north
        let south = g.pod(ix, iy_line.saturating_sub(12)); // ~24 m south
        assert!(north > 0.0 && south == 0.0, "north {north}, south {south}");
        assert!(trail_evidence(&trail).unwrap()["end_heading_deg"].as_f64().unwrap() > 80.0);
    }

    #[test]
    fn drone_nadir_footprint_and_resolution() {
        let seg = vec![square("A", 42.0, -86.0, 150.0)];
        let mut low = line(42.0, -86.0005, -85.9995, 5);
        for p in &mut low {
            p.alt_agl = Some(40.0);
        }
        let mut high = low.clone();
        for p in &mut high {
            p.alt_agl = Some(400.0);
        }
        let dl = Resource { id: "d40".into(), kind: Kind::Drone, label: String::new(), track: low, knobs: json!({}) };
        let dh = Resource { id: "d400".into(), kind: Kind::Drone, label: String::new(), track: high, knobs: json!({}) };
        let lo = compute(&seg, &[dl], &json!({}), None, 5.0);
        let hi = compute(&seg, &[dh], &json!({}), None, 5.0);
        assert!(lo.segments[0].pod > 0.0, "40 m resolves a person");
        assert_eq!(hi.segments[0].pod, 0.0, "400 m: too few pixels on a person");
    }

    #[test]
    fn poa_update_moves_probability_to_unsearched() {
        let a = square("A", 42.0, -86.0, 100.0);
        let b = square("B", 42.0, -85.99, 100.0);
        let r = Resource { id: "f".into(), kind: Kind::Foot, label: String::new(), track: line(42.0, -86.001, -85.999, 10), knobs: json!({"half_width_m": 300.0, "pod": 0.8}) };
        let c = compute(&[a, b], &[r], &json!({}), None, 10.0);
        let (sa, sb) = (&c.segments[0], &c.segments[1]);
        assert!((sa.poa_before - 0.5).abs() < 1e-9);
        assert!(sa.poa_after < sa.poa_before && sb.poa_after > sb.poa_before);
        assert!((sa.poa_after + sb.poa_after - 1.0).abs() < 1e-9);
    }

    #[test]
    fn frame_position_interpolates() {
        let t = line(42.0, -86.0, -85.99, 2);
        let (_, lon) = position_at(&t, 0.5).unwrap();
        assert!((lon - -85.995).abs() < 1e-9);
    }
}
