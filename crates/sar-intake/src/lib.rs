//! Phase 1 case intake (schemas/case_intake.json). Missing or invalid input is
//! always an error; nothing is defaulted or guessed (TRUTH_CONTRACT).
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Water,
    Land,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    WreckHunt,
    VesselSar,
    AircraftSar,
    VehicleSar,
    PersonSearch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lkp {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub description: Option<String>,
    pub time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    pub name: String,
    pub age: Option<u32>,
    pub clothing_description: Option<String>,
    #[serde(default)]
    pub medical_conditions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub relationship: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseIntake {
    pub mission_id: String,
    pub domain: Domain,
    pub objective: Objective,
    pub play_id: Option<String>,
    pub priority: Priority,
    pub lkp: Option<Lkp>,
    pub bbox: Option<[f64; 4]>,
    pub subject: Subject,
    #[serde(default)]
    pub contacts: Vec<Contact>,
}

#[derive(Debug, PartialEq)]
pub enum IntakeError {
    Parse(String),
    BlankMissionId,
    BlankSubjectName,
    LatOutOfRange(f64),
    LonOutOfRange(f64),
    LkpIncomplete,
    BadTime(String),
    BadBbox(String),
    DomainObjectiveMismatch { domain: Domain, objective: Objective },
    NoLocation,
}

impl fmt::Display for IntakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "intake JSON invalid: {e}"),
            Self::BlankMissionId => write!(f, "mission_id is blank"),
            Self::BlankSubjectName => write!(f, "subject name is blank"),
            Self::LatOutOfRange(v) => write!(f, "latitude {v} is not a finite value in -90..90"),
            Self::LonOutOfRange(v) => write!(f, "longitude {v} is not a finite value in -180..180"),
            Self::LkpIncomplete => write!(f, "lkp has only one of lat/lon"),
            Self::BadTime(t) => write!(f, "lkp time {t:?} is not RFC 3339"),
            Self::BadBbox(why) => write!(f, "bbox invalid: {why}"),
            Self::DomainObjectiveMismatch { domain, objective } => {
                write!(f, "objective {objective:?} is not allowed in domain {domain:?}")
            }
            Self::NoLocation => write!(f, "case has neither lkp coordinates nor bbox"),
        }
    }
}

impl std::error::Error for IntakeError {}

fn check_lat(v: f64) -> Result<(), IntakeError> {
    if v.is_finite() && (-90.0..=90.0).contains(&v) {
        Ok(())
    } else {
        Err(IntakeError::LatOutOfRange(v))
    }
}

fn check_lon(v: f64) -> Result<(), IntakeError> {
    if v.is_finite() && (-180.0..=180.0).contains(&v) {
        Ok(())
    } else {
        Err(IntakeError::LonOutOfRange(v))
    }
}

/// Parse `n` ASCII digits at `s[at..]` as a number.
fn digits(s: &[u8], at: usize, n: usize) -> Option<u32> {
    let part = s.get(at..at + n)?;
    if !part.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(part.iter().fold(0, |a, d| a * 10 + u32::from(d - b'0')))
}

fn is_rfc3339(t: &str) -> bool {
    let b = t.as_bytes();
    let sep = |i: usize, c: u8| b.get(i) == Some(&c);
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) = (
        digits(b, 0, 4),
        digits(b, 5, 2),
        digits(b, 8, 2),
        digits(b, 11, 2),
        digits(b, 14, 2),
        digits(b, 17, 2),
    ) else {
        return false;
    };
    if !(sep(4, b'-') && sep(7, b'-') && sep(10, b'T') && sep(13, b':') && sep(16, b':')) {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = match mo {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if d < 1 || d > dim || h > 23 || mi > 59 || s > 59 {
        return false;
    }
    let mut i = 19;
    if sep(i, b'.') {
        let start = i + 1;
        i = start;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match b.get(i) {
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            i + 6 == b.len()
                && sep(i + 3, b':')
                && digits(b, i + 1, 2).is_some_and(|h| h <= 23)
                && digits(b, i + 4, 2).is_some_and(|m| m <= 59)
        }
        _ => false,
    }
}

impl CaseIntake {
    pub fn validate(&self) -> Result<(), IntakeError> {
        if self.mission_id.trim().is_empty() {
            return Err(IntakeError::BlankMissionId);
        }
        if self.subject.name.trim().is_empty() {
            return Err(IntakeError::BlankSubjectName);
        }
        let mut has_position = false;
        if let Some(lkp) = &self.lkp {
            match (lkp.lat, lkp.lon) {
                (Some(lat), Some(lon)) => {
                    check_lat(lat)?;
                    check_lon(lon)?;
                    has_position = true;
                }
                (None, None) => {}
                _ => return Err(IntakeError::LkpIncomplete),
            }
            if let Some(t) = &lkp.time {
                if !is_rfc3339(t) {
                    return Err(IntakeError::BadTime(t.clone()));
                }
            }
        }
        if let Some([min_lon, min_lat, max_lon, max_lat]) = self.bbox {
            check_lon(min_lon).map_err(|e| IntakeError::BadBbox(e.to_string()))?;
            check_lon(max_lon).map_err(|e| IntakeError::BadBbox(e.to_string()))?;
            check_lat(min_lat).map_err(|e| IntakeError::BadBbox(e.to_string()))?;
            check_lat(max_lat).map_err(|e| IntakeError::BadBbox(e.to_string()))?;
            if min_lon >= max_lon || min_lat >= max_lat {
                return Err(IntakeError::BadBbox(
                    "expected [min_lon,min_lat,max_lon,max_lat] with min < max".into(),
                ));
            }
            has_position = true;
        }
        let bad_combo = match self.domain {
            Domain::Land => matches!(self.objective, Objective::VesselSar | Objective::WreckHunt),
            Domain::Water => self.objective == Objective::VehicleSar,
        };
        if bad_combo {
            return Err(IntakeError::DomainObjectiveMismatch {
                domain: self.domain.clone(),
                objective: self.objective.clone(),
            });
        }
        if !has_position {
            return Err(IntakeError::NoLocation);
        }
        Ok(())
    }
}

pub fn parse_intake(json: &str) -> Result<CaseIntake, IntakeError> {
    let intake: CaseIntake =
        serde_json::from_str(json).map_err(|e| IntakeError::Parse(e.to_string()))?;
    intake.validate()?;
    Ok(intake)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn base() -> Value {
        json!({
            "mission_id": "M-1", "domain": "water", "objective": "vessel_sar",
            "priority": "high",
            "lkp": {"lat": 42.5, "lon": -82.7, "time": "2026-10-27T14:30:00Z"},
            "subject": {"name": "Skipper"}
        })
    }

    fn parse(v: &Value) -> Result<CaseIntake, IntakeError> {
        parse_intake(&v.to_string())
    }

    #[test]
    fn valid_vessel_case_parses() {
        let c = parse(&base()).unwrap();
        assert_eq!(c.mission_id, "M-1");
        assert_eq!(c.objective, Objective::VesselSar);
        assert!(c.contacts.is_empty() && c.subject.medical_conditions.is_empty());
    }

    #[test]
    fn blank_mission_id_rejected() {
        let mut v = base();
        v["mission_id"] = json!("  ");
        assert_eq!(parse(&v), Err(IntakeError::BlankMissionId));
    }

    #[test]
    fn blank_subject_name_rejected() {
        let mut v = base();
        v["subject"]["name"] = json!("");
        assert_eq!(parse(&v), Err(IntakeError::BlankSubjectName));
    }

    #[test]
    fn lat_out_of_range_rejected() {
        let mut v = base();
        v["lkp"]["lat"] = json!(90.5);
        assert!(matches!(parse(&v), Err(IntakeError::LatOutOfRange(_))));
    }

    #[test]
    fn lon_out_of_range_rejected() {
        let mut v = base();
        v["lkp"]["lon"] = json!(-181.0);
        assert!(matches!(parse(&v), Err(IntakeError::LonOutOfRange(_))));
    }

    #[test]
    fn non_finite_coordinate_rejected() {
        let c = parse_intake(
            r#"{"mission_id":"M","domain":"water","objective":"vessel_sar","priority":"low",
            "lkp":{"lat":1e999,"lon":0},"subject":{"name":"A"}}"#,
        );
        // serde_json refuses an overflowing number, or validate rejects it: Err either way.
        assert!(c.is_err());
        let mut intake = parse(&base()).unwrap();
        intake.lkp.as_mut().unwrap().lat = Some(f64::NAN);
        assert!(matches!(intake.validate(), Err(IntakeError::LatOutOfRange(_))));
    }

    #[test]
    fn lkp_with_only_one_coordinate_rejected() {
        let mut v = base();
        v["lkp"].as_object_mut().unwrap().remove("lon");
        assert_eq!(parse(&v), Err(IntakeError::LkpIncomplete));
    }

    #[test]
    fn bad_time_rejected() {
        for t in [
            "yesterday", "2026-10-27 14:30:00Z", "2026-10-27T14:30:00", "2026-13-01T00:00:00Z",
            "2026-02-30T00:00:00Z", "2026-10-27T24:00:00Z", "2026-10-27T14:30:00.Z",
            "2026-10-27T14:30:00+0100", "2026-10-27T14:30:00Zjunk",
        ] {
            let mut v = base();
            v["lkp"]["time"] = json!(t);
            assert!(matches!(parse(&v), Err(IntakeError::BadTime(_))), "{t}");
        }
    }

    #[test]
    fn good_times_accepted() {
        for t in ["2026-10-27T14:30:00Z", "2024-02-29T00:00:00.123+05:30", "2026-10-27T14:30:00-04:00"] {
            let mut v = base();
            v["lkp"]["time"] = json!(t);
            assert!(parse(&v).is_ok(), "{t}");
        }
    }

    #[test]
    fn bbox_rejections() {
        for bb in [
            json!([-83.0, 42.0, -82.0, 41.0]),
            json!([-82.0, 42.0, -83.0, 43.0]),
            json!([-83.0, 42.0, -83.0, 43.0]),
            json!([-183.0, 42.0, -82.0, 43.0]),
            json!([-83.0, 42.0, -82.0, 91.0]),
        ] {
            let mut v = base();
            v.as_object_mut().unwrap().remove("lkp");
            v["bbox"] = bb.clone();
            assert!(matches!(parse(&v), Err(IntakeError::BadBbox(_))), "{bb}");
        }
    }

    #[test]
    fn bbox_wrong_length_rejected() {
        let mut v = base();
        v["bbox"] = json!([1.0, 2.0, 3.0]);
        assert!(matches!(parse(&v), Err(IntakeError::Parse(_))));
    }

    #[test]
    fn valid_bbox_alone_is_enough() {
        let mut v = base();
        v.as_object_mut().unwrap().remove("lkp");
        v["bbox"] = json!([-83.0, 42.0, -82.0, 43.0]);
        assert!(parse(&v).is_ok());
    }

    #[test]
    fn land_with_vessel_or_wreck_rejected() {
        for o in ["vessel_sar", "wreck_hunt"] {
            let mut v = base();
            v["domain"] = json!("land");
            v["objective"] = json!(o);
            assert!(matches!(parse(&v), Err(IntakeError::DomainObjectiveMismatch { .. })), "{o}");
        }
    }

    #[test]
    fn water_with_vehicle_rejected_but_person_search_allowed() {
        let mut v = base();
        v["objective"] = json!("vehicle_sar");
        assert!(matches!(parse(&v), Err(IntakeError::DomainObjectiveMismatch { .. })));
        v["objective"] = json!("person_search");
        assert!(parse(&v).is_ok());
    }

    #[test]
    fn no_location_rejected() {
        let mut v = base();
        v["lkp"] = json!({"description": "somewhere near the point", "time": "2026-10-27T14:30:00Z"});
        assert_eq!(parse(&v), Err(IntakeError::NoLocation));
        v.as_object_mut().unwrap().remove("lkp");
        assert_eq!(parse(&v), Err(IntakeError::NoLocation));
    }

    #[test]
    fn missing_required_field_is_err() {
        let mut v = base();
        v.as_object_mut().unwrap().remove("subject");
        assert!(matches!(parse(&v), Err(IntakeError::Parse(_))));
        for k in ["mission_id", "domain", "objective", "priority"] {
            let mut v = base();
            v.as_object_mut().unwrap().remove(k);
            assert!(matches!(parse(&v), Err(IntakeError::Parse(_))), "{k}");
        }
    }

    #[test]
    fn unknown_objective_is_err() {
        let mut v = base();
        v["objective"] = json!("treasure_hunt");
        assert!(matches!(parse(&v), Err(IntakeError::Parse(_))));
    }

    #[test]
    fn round_trip_equals_original() {
        let mut v = base();
        v["play_id"] = json!("water_vessel_sar");
        v["bbox"] = json!([-83.0, 42.0, -82.0, 43.0]);
        v["subject"] = json!({"name": "Skipper", "age": 54, "medical_conditions": ["asthma"]});
        v["contacts"] = json!([{"name": "Sam", "phone": "555-0100", "relationship": "spouse"}]);
        let c = parse(&v).unwrap();
        let again = parse_intake(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, again);
    }
}
