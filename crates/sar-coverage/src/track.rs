//! Tracks from search resources: dog collars and handler phones (GPX / KML / CSV) and drones (DJI
//! `.SRT` telemetry, one record per video frame). All readers are small and dependency-free.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackPoint {
    /// Unix seconds (fractional), when known.
    pub t: Option<f64>,
    pub lat: f64,
    pub lon: f64,
    /// Height above the take-off point (drones: `rel_alt`), metres.
    pub alt_agl: Option<f64>,
    /// Gimbal pitch in degrees (-90 = straight down) and yaw (true heading of the camera), drones.
    pub gimbal_pitch: Option<f64>,
    pub gimbal_yaw: Option<f64>,
    /// Video frame number for drone records (`FrameCnt`), so footage maps to positions.
    pub frame: Option<u64>,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let i = tag.find(&key)? + key.len();
    let rest = &tag[i..];
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let end = rest[1..].find(q)? + 1;
    Some(rest[1..end].to_string())
}

fn inner<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = s.find(&format!("<{tag}"))?;
    let after = s[open..].find('>')? + open + 1;
    let close = s[after..].find(&format!("</{tag}>"))? + after;
    Some(s[after..close].trim())
}

pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis() as f64 / 1000.0);
    }
    for f in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S,%f", "%Y.%m.%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Some(Utc.from_utc_datetime(&n).timestamp_millis() as f64 / 1000.0);
        }
    }
    None
}

/// GPX `<trkpt lat lon><ele/><time/></trkpt>` (also `<rtept>` and `<wpt>`).
pub fn parse_gpx(text: &str) -> Vec<TrackPoint> {
    let mut out = Vec::new();
    for tagname in ["trkpt", "rtept"] {
        let mut rest = text;
        while let Some(i) = rest.find(&format!("<{tagname}")) {
            let r = &rest[i..];
            let head_end = r.find('>').unwrap_or(r.len());
            let head = &r[..head_end];
            let self_closing = head.ends_with('/');
            let body_end = if self_closing { head_end } else { r.find(&format!("</{tagname}>")).unwrap_or(head_end) };
            let body = &r[..body_end];
            if let (Some(lat), Some(lon)) = (attr(head, "lat").and_then(|v| v.parse().ok()), attr(head, "lon").and_then(|v| v.parse().ok())) {
                out.push(TrackPoint {
                    t: inner(body, "time").and_then(parse_time),
                    lat,
                    lon,
                    alt_agl: None,
                    ..Default::default()
                });
            }
            rest = &r[body_end.max(1)..];
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// KML: `<gx:Track>` (`<when>` + `<gx:coord>lon lat alt`) or plain `<coordinates>lon,lat,alt ...`.
pub fn parse_kml(text: &str) -> Vec<TrackPoint> {
    let whens: Vec<Option<f64>> = text.split("<when>").skip(1).map(|s| s.split("</when>").next().and_then(parse_time)).collect();
    let coords: Vec<(f64, f64)> = text
        .split("<gx:coord>")
        .skip(1)
        .filter_map(|s| {
            let v: Vec<f64> = s.split("</gx:coord>").next()?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() >= 2).then(|| (v[0], v[1]))
        })
        .collect();
    if !coords.is_empty() {
        return coords.iter().enumerate().map(|(i, (lon, lat))| TrackPoint { t: whens.get(i).copied().flatten(), lat: *lat, lon: *lon, ..Default::default() }).collect();
    }
    let mut out = Vec::new();
    for block in text.split("<coordinates>").skip(1) {
        let Some(body) = block.split("</coordinates>").next() else { continue };
        for tok in body.split_whitespace() {
            let v: Vec<f64> = tok.split(',').filter_map(|x| x.parse().ok()).collect();
            if v.len() >= 2 {
                out.push(TrackPoint { lat: v[1], lon: v[0], ..Default::default() });
            }
        }
    }
    out
}

/// CSV with a header naming lat/latitude, lon/lng/longitude and optionally time/timestamp, alt.
pub fn parse_csv(text: &str) -> Vec<TrackPoint> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else { return Vec::new() };
    let cols: Vec<String> = header.split(',').map(|c| c.trim().to_ascii_lowercase()).collect();
    let find = |names: &[&str]| cols.iter().position(|c| names.contains(&c.as_str()));
    let (Some(ilat), Some(ilon)) = (find(&["lat", "latitude"]), find(&["lon", "lng", "long", "longitude"])) else { return Vec::new() };
    let it = find(&["time", "timestamp", "datetime", "utc"]);
    let ia = find(&["alt", "altitude", "rel_alt", "agl"]);
    lines
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(|x| x.trim()).collect();
            Some(TrackPoint {
                lat: f.get(ilat)?.parse().ok()?,
                lon: f.get(ilon)?.parse().ok()?,
                t: it.and_then(|i| f.get(i)).and_then(|s| s.parse::<f64>().ok().or_else(|| parse_time(s))),
                alt_agl: ia.and_then(|i| f.get(i)).and_then(|s| s.parse().ok()),
                ..Default::default()
            })
        })
        .collect()
}

/// `[key: value key2: value2]` pairs as DJI writes them in `.SRT` subtitle records.
fn bracket_pairs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for group in s.split('[').skip(1) {
        let g = group.split(']').next().unwrap_or("");
        let toks: Vec<&str> = g.split_whitespace().collect();
        let mut i = 0;
        while i < toks.len() {
            if let Some(k) = toks[i].strip_suffix(':') {
                if let Some(v) = toks.get(i + 1) {
                    out.push((k.to_ascii_lowercase(), v.to_string()));
                    i += 2;
                    continue;
                }
            } else if let Some((k, v)) = toks[i].split_once(':') {
                if !v.is_empty() {
                    out.push((k.to_ascii_lowercase(), v.to_string()));
                }
            }
            i += 1;
        }
    }
    out
}

/// DJI `.SRT` telemetry: one subtitle block per frame with `FrameCnt`, a timestamp line, and
/// `[latitude: ..] [longitude: ..] [rel_alt: .. abs_alt: ..]` and, on newer drones,
/// `[gb_yaw: .. gb_pitch: .. gb_roll: ..]`. Older formats write `GPS(lon,lat,alt)` instead.
pub fn parse_dji_srt(text: &str) -> Vec<TrackPoint> {
    let mut out = Vec::new();
    for block in text.replace("\r\n", "\n").split("\n\n") {
        let mut p = TrackPoint::default();
        let mut have = (false, false);
        for line in block.lines() {
            let l = line.trim();
            if p.t.is_none() && l.len() >= 19 && l.as_bytes()[4] == b'-' && l.as_bytes()[7] == b'-' {
                p.t = parse_time(l);
            }
            if let Some(i) = l.find("FrameCnt") {
                p.frame = l[i + 8..].trim_start_matches([':', ' ']).split(|c: char| !c.is_ascii_digit()).next().and_then(|s| s.parse().ok());
            }
            if let Some(i) = l.find("GPS(") {
                let v: Vec<f64> = l[i + 4..].split(')').next().unwrap_or("").split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() >= 2 {
                    p.lon = v[0];
                    p.lat = v[1];
                    have = (true, true);
                }
            }
            for (k, v) in bracket_pairs(l) {
                let n: Option<f64> = v.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok();
                match (k.as_str(), n) {
                    ("latitude", Some(x)) => { p.lat = x; have.0 = true; }
                    ("longitude" | "longtitude", Some(x)) => { p.lon = x; have.1 = true; }
                    ("rel_alt", Some(x)) => p.alt_agl = Some(x),
                    ("gb_pitch", Some(x)) => p.gimbal_pitch = Some(x),
                    ("gb_yaw", Some(x)) => p.gimbal_yaw = Some(x),
                    _ => {}
                }
            }
        }
        if have.0 && have.1 && (p.lat != 0.0 || p.lon != 0.0) {
            out.push(p);
        }
    }
    out
}

/// Pick a reader by file extension.
pub fn read_track(path: &std::path::Path) -> std::io::Result<Vec<TrackPoint>> {
    let text = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    Ok(match ext.as_str() {
        "gpx" => parse_gpx(&text),
        "kml" => parse_kml(&text),
        "srt" => parse_dji_srt(&text),
        _ => parse_csv(&text),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpx_points_with_time() {
        let g = r#"<gpx><trk><trkseg>
            <trkpt lat="42.1" lon="-86.2"><ele>180</ele><time>2026-11-15T14:00:00Z</time></trkpt>
            <trkpt lat="42.1005" lon="-86.2"><time>2026-11-15T14:00:10Z</time></trkpt>
        </trkseg></trk></gpx>"#;
        let p = parse_gpx(g);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].lat, 42.1);
        assert_eq!(p[1].t.unwrap() - p[0].t.unwrap(), 10.0);
    }

    #[test]
    fn dji_srt_new_and_old_formats() {
        let new = "1\n00:00:00,000 --> 00:00:00,033\n<font size=\"28\">FrameCnt: 1, DiffTime: 33ms\n2026-11-15 14:00:00.123\n[iso: 100] [latitude: 42.123456] [longitude: -86.234567] [rel_alt: 60.200 abs_alt: 240.1] [gb_yaw: 15.0 gb_pitch: -90.0 gb_roll: 0.0] </font>\n\n2\n00:00:00,033 --> 00:00:00,066\nFrameCnt: 2\n2026-11-15 14:00:00.156\n[latitude: 42.123460] [longitude: -86.234567] [rel_alt: 60.3 abs_alt: 240.2]\n";
        let p = parse_dji_srt(new);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].frame, Some(1));
        assert_eq!(p[0].alt_agl, Some(60.2));
        assert_eq!(p[0].gimbal_pitch, Some(-90.0));
        assert!((p[1].t.unwrap() - p[0].t.unwrap() - 0.033).abs() < 1e-6);
        let old = "1\n00:00:00,000 --> 00:00:01,000\nHOME(-86.2,42.1) 2026.11.15 14:00:00\nGPS(-86.2345,42.1234,18) BAROMETER:59.8\n";
        let o = parse_dji_srt(old);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].lat, 42.1234);
    }

    #[test]
    fn kml_and_csv() {
        let k = "<kml><LineString><coordinates>-86.2,42.1,0 -86.21,42.11,0</coordinates></LineString></kml>";
        assert_eq!(parse_kml(k).len(), 2);
        let c = "time,latitude,longitude\n2026-11-15T14:00:00Z,42.1,-86.2\n2026-11-15T14:00:05Z,42.1001,-86.2\n";
        let p = parse_csv(c);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].t.unwrap() - p[0].t.unwrap(), 5.0);
    }
}
