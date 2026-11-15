//! Search patterns as tracks, for resources that carry no GPS (divers on a rope) and for planning
//! (drone lawn-mower / expanding square). A pattern becomes ordinary track points, so coverage treats
//! a dive team's circular search exactly like a recorded track.

use crate::track::TrackPoint;
use crate::Proj;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pattern {
    /// Diver circling an anchor on a rope, let out `step_m` each lap until `radius_max_m`.
    Circular { center: (f64, f64), #[serde(default = "d_step")] step_m: f64, radius_max_m: f64, #[serde(default)] radius_start_m: f64 },
    /// Jackstay: a line from `from` to `to`, moved sideways `move_m` each sweep, `sweeps` times
    /// (to the right of the from->to direction; negative `move_m` moves left).
    Jackstay { from: (f64, f64), to: (f64, f64), move_m: f64, sweeps: u32 },
    /// Parallel legs (lawn-mower) over a box: `origin` is one corner, legs run along `heading_deg`.
    Lawnmower { origin: (f64, f64), heading_deg: f64, leg_m: f64, spacing_m: f64, legs: u32 },
    /// Expanding square from `center`, first leg `step_m`, growing every two legs.
    ExpandingSquare { center: (f64, f64), step_m: f64, legs: u32, #[serde(default)] heading_deg: f64 },
}
fn d_step() -> f64 { 3.0 }

fn pt(proj: &Proj, x: f64, y: f64, alt: Option<f64>) -> TrackPoint {
    let (lat, lon) = proj.inv(x, y);
    TrackPoint { lat, lon, alt_agl: alt, ..Default::default() }
}

fn dir(deg: f64) -> (f64, f64) {
    let r = deg.to_radians();
    (r.sin(), r.cos())
}

/// Track points for a pattern (`alt` set on every point, e.g. drone height for planning).
pub fn to_track(p: &Pattern, alt: Option<f64>) -> Vec<TrackPoint> {
    match p {
        Pattern::Circular { center, step_m, radius_max_m, radius_start_m } => {
            let proj = Proj::new(center.0, center.1);
            let mut out = Vec::new();
            let mut r = if *radius_start_m > 0.0 { *radius_start_m } else { *step_m };
            while r <= radius_max_m + 1e-9 {
                let n = ((2.0 * std::f64::consts::PI * r / 1.0).ceil() as usize).clamp(12, 720);
                for i in 0..=n {
                    let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                    out.push(pt(&proj, r * a.sin(), r * a.cos(), alt));
                }
                r += step_m.max(0.5);
            }
            out
        }
        Pattern::Jackstay { from, to, move_m, sweeps } => {
            let proj = Proj::new(from.0, from.1);
            let (bx, by) = proj.fwd(to.0, to.1);
            let len = (bx * bx + by * by).sqrt().max(1e-9);
            let (ux, uy) = (bx / len, by / len);
            let (rx, ry) = (uy, -ux);
            let mut out = Vec::new();
            for s in 0..=*sweeps {
                let o = s as f64 * move_m;
                let (a, b) = ((rx * o, ry * o), (bx + rx * o, by + ry * o));
                let (p0, p1) = if s % 2 == 0 { (a, b) } else { (b, a) };
                out.push(pt(&proj, p0.0, p0.1, alt));
                out.push(pt(&proj, p1.0, p1.1, alt));
            }
            out
        }
        Pattern::Lawnmower { origin, heading_deg, leg_m, spacing_m, legs } => {
            let proj = Proj::new(origin.0, origin.1);
            let (fx, fy) = dir(*heading_deg);
            let (rx, ry) = (fy, -fx);
            let mut out = Vec::new();
            for l in 0..*legs {
                let o = l as f64 * spacing_m;
                let a = (rx * o, ry * o);
                let b = (a.0 + fx * leg_m, a.1 + fy * leg_m);
                let (p0, p1) = if l % 2 == 0 { (a, b) } else { (b, a) };
                out.push(pt(&proj, p0.0, p0.1, alt));
                out.push(pt(&proj, p1.0, p1.1, alt));
            }
            out
        }
        Pattern::ExpandingSquare { center, step_m, legs, heading_deg } => {
            let proj = Proj::new(center.0, center.1);
            let (mut x, mut y) = (0.0, 0.0);
            let mut out = vec![pt(&proj, 0.0, 0.0, alt)];
            for i in 0..*legs {
                let len = step_m * ((i / 2) + 1) as f64;
                let (dx, dy) = dir(heading_deg + 90.0 * i as f64);
                x += dx * len;
                y += dy * len;
                out.push(pt(&proj, x, y, alt));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_rings_reach_the_max_radius() {
        let t = to_track(&Pattern::Circular { center: (43.0, -87.0), step_m: 3.0, radius_max_m: 15.0, radius_start_m: 0.0 }, None);
        let proj = Proj::new(43.0, -87.0);
        let maxr = t.iter().map(|p| { let (x, y) = proj.fwd(p.lat, p.lon); (x * x + y * y).sqrt() }).fold(0.0, f64::max);
        assert!((maxr - 15.0).abs() < 0.5, "{maxr}");
    }

    #[test]
    fn jackstay_and_expanding_square_shapes() {
        let j = to_track(&Pattern::Jackstay { from: (43.0, -87.0), to: (43.0003, -87.0), move_m: 4.0, sweeps: 5 }, None);
        assert_eq!(j.len(), 12);
        let e = to_track(&Pattern::ExpandingSquare { center: (43.0, -87.0), step_m: 10.0, legs: 8, heading_deg: 0.0 }, Some(50.0));
        assert_eq!(e.len(), 9);
        assert_eq!(e[1].alt_agl, Some(50.0));
    }
}
