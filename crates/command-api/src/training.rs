//! Training data set: fictitious cases loaded from a seed file (JSON).
//!
//! Everything in a seed is made up (light-hearted names, 555 numbers, example.org
//! addresses). Each loaded case carries a `training_marker` attachment holding
//! "FICTITIOUS - training data", so the UI can show it and the tester site can
//! delete exactly these cases again. Real sample sonar runs are attached as
//! `sample_run` attachments (processed elsewhere; the FCU does not run surveys),
//! and anything made up for the samples section is a `simulated` attachment that
//! the UI always draws under a "FULLY SIMULATED - under construction" banner.

use chrono::{Duration, Utc};
use command_db::{ContactInput, CreateCase, Database, SubjectInput};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const MARKER: &str = "FICTITIOUS - training data";

#[derive(Debug, Deserialize)]
pub struct Seed {
    #[serde(default)]
    pub name: String,
    #[serde(default = "marker")]
    pub marker: String,
    /// Real sample runs by key (label, source, licence, credit, links, stats).
    #[serde(default)]
    pub sample_runs: HashMap<String, Value>,
    pub cases: Vec<SeedCase>,
}
fn marker() -> String { MARKER.into() }

#[derive(Debug, Deserialize)]
pub struct SeedCase {
    pub id: String,
    pub domain: String,
    pub objective: String,
    pub priority: String,
    #[serde(default)]
    pub area: String,
    pub subject: SeedSubject,
    pub lkp: Option<SeedLkp>,
    #[serde(default)]
    pub contacts: Vec<SeedContact>,
    #[serde(default)]
    pub team: Vec<SeedTeam>,
    #[serde(default)]
    pub updates: Vec<SeedLog>,
    #[serde(default)]
    pub clues: Vec<SeedLog>,
    #[serde(default)]
    pub family_report: String,
    /// Key into `Seed::sample_runs`.
    #[serde(default)]
    pub sample_run: Option<String>,
    #[serde(default)]
    pub simulated: Vec<SeedSimulated>,
}
#[derive(Debug, Deserialize)]
pub struct SeedSubject {
    pub name: String,
    pub age: Option<u8>,
    #[serde(default)]
    pub clothing: Option<String>,
    #[serde(default)]
    pub medical: Vec<String>,
}
#[derive(Debug, Deserialize)]
pub struct SeedLkp { pub lat: f64, pub lon: f64, pub text: String, pub min_ago: i64 }
#[derive(Debug, Deserialize)]
pub struct SeedContact { pub name: String, pub phone: String, pub relationship: String }
#[derive(Debug, Deserialize)]
pub struct SeedTeam { pub callsign: String, pub name: String, pub role: String, pub status: String }
#[derive(Debug, Deserialize)]
pub struct SeedLog {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    pub min_ago: i64,
}
#[derive(Debug, Deserialize)]
pub struct SeedSimulated { pub title: String, pub text: String }

pub fn load_seed(path: &str) -> Result<Seed, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let seed: Seed = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    check(&seed).map_err(|e| format!("{path}: {e}"))?;
    Ok(seed)
}

/// Refuse a seed that breaks the operator's rules for the set.
fn check(seed: &Seed) -> Result<(), String> {
    for c in &seed.cases {
        if !c.id.starts_with("TRN-") {
            return Err(format!("case {} must start with TRN-", c.id));
        }
        for k in &c.contacts {
            if !k.phone.contains("555") {
                return Err(format!("case {}: contact phone must be a 555 number", c.id));
            }
        }
        if let Some(key) = &c.sample_run {
            let run = seed.sample_runs.get(key).ok_or(format!("case {}: unknown sample run {key}", c.id))?;
            for f in ["label", "source", "licence", "credit"] {
                if run.get(f).and_then(Value::as_str).unwrap_or("").is_empty() {
                    return Err(format!("sample run {key}: missing {f}"));
                }
            }
        }
    }
    Ok(())
}

/// Load every seed case into `db`; cases whose id already exists are skipped.
/// Times are relative to now so the set always looks current.
/// Returns (loaded, skipped).
pub fn apply(db: &mut Database, seed: &Seed) -> Result<(usize, usize), String> {
    let now = Utc::now();
    let ts = |m: i64| (now - Duration::minutes(m)).to_rfc3339();
    let e = |x: command_db::DbError| x.to_string();
    let (mut loaded, mut skipped) = (0, 0);
    for c in &seed.cases {
        if db.get_case(&c.id).is_ok() {
            skipped += 1;
            continue;
        }
        db.create_case(CreateCase {
            id: Some(c.id.clone()),
            domain: c.domain.clone(),
            objective: c.objective.clone(),
            priority: c.priority.clone(),
            play_id: None,
            subject: SubjectInput {
                name: c.subject.name.clone(),
                age: c.subject.age,
                clothing_description: c.subject.clothing.clone(),
                medical_conditions: c.subject.medical.clone(),
            },
            contacts: c
                .contacts
                .iter()
                .map(|k| ContactInput { name: k.name.clone(), phone: Some(k.phone.clone()), relationship: Some(k.relationship.clone()) })
                .collect(),
        })
        .map_err(e)?;
        db.add_attachment(&c.id, "training_marker", &seed.marker, &json!({ "text": seed.marker, "area": c.area })).map_err(e)?;
        if let Some(l) = &c.lkp {
            db.add_clue(&c.id, &format!("Last known position: {} [{}]", l.text, seed.marker), "intake", &ts(l.min_ago), Some((l.lat, l.lon))).map_err(e)?;
        }
        for u in &c.updates {
            db.add_update(&c.id, &u.text, &u.role, &ts(u.min_ago)).map_err(e)?;
        }
        for k in &c.clues {
            db.add_clue(&c.id, &k.text, &k.role, &ts(k.min_ago), k.lat.zip(k.lon)).map_err(e)?;
        }
        for t in &c.team {
            db.add_team_member(&c.id, &format!("{} — {}", t.callsign, t.name), &t.role, &t.status).map_err(e)?;
        }
        if !c.family_report.is_empty() {
            db.add_attachment(&c.id, "family_report", "Message to the family", &json!({ "text": c.family_report, "marker": seed.marker })).map_err(e)?;
        }
        if let Some(key) = &c.sample_run {
            let run = &seed.sample_runs[key];
            let title = run.get("title").and_then(Value::as_str).unwrap_or(key.as_str()).to_owned();
            db.add_attachment(&c.id, "sample_run", &title, run).map_err(e)?;
        }
        for s in &c.simulated {
            db.add_attachment(&c.id, "simulated", &s.title, &json!({ "banner": "FULLY SIMULATED - under construction", "text": s.text })).map_err(e)?;
        }
        loaded += 1;
    }
    Ok((loaded, skipped))
}

/// Delete every case carrying the training marker (nothing else). Returns how many.
pub fn delete_training(db: &mut Database) -> Result<usize, String> {
    let ids: Vec<String> = db
        .list_cases()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|c| db.attachments(&c.id).map(|a| a.iter().any(|x| x.kind == "training_marker")).unwrap_or(false))
        .map(|c| c.id)
        .collect();
    for id in &ids {
        db.delete_case(id).map_err(|e| e.to_string())?;
    }
    Ok(ids.len())
}

pub fn count_training(db: &Database) -> usize {
    db.list_cases()
        .map(|l| l.iter().filter(|c| db.attachments(&c.id).map(|a| a.iter().any(|x| x.kind == "training_marker")).unwrap_or(false)).count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Seed {
        serde_json::from_value(json!({
            "sample_runs": {"r": {"label":"Real sample run - proxy people and data","source":"s","licence":"MIT","credit":"c"}},
            "cases": [{"id":"TRN-1","domain":"water","objective":"vessel_sar","priority":"high",
              "subject":{"name":"Ima Lostboater","age":40},
              "lkp":{"lat":43.0,"lon":-87.0,"text":"x","min_ago":60},
              "contacts":[{"name":"A","phone":"555-0100","relationship":"r"}],
              "sample_run":"r","simulated":[{"title":"t","text":"x"}],"family_report":"hello"}]
        }))
        .unwrap()
    }

    #[test]
    fn load_marks_and_delete_removes_only_training() {
        let mut db = Database::open_in_memory().unwrap();
        db.create_case(CreateCase {
            id: Some("MINE".into()), domain: "land".into(), objective: "person_search".into(), priority: "low".into(),
            play_id: None, subject: SubjectInput { name: "Mine".into(), age: None, clothing_description: None, medical_conditions: vec![] },
            contacts: vec![],
        }).unwrap();
        let s = seed();
        check(&s).unwrap();
        assert_eq!(apply(&mut db, &s).unwrap(), (1, 0));
        assert_eq!(apply(&mut db, &s).unwrap(), (0, 1));
        let w = crate::web_case(&db, "TRN-1").unwrap();
        assert!(w.training);
        assert_eq!(w.attachments.iter().filter(|a| a.kind == "sample_run").count(), 1);
        assert_eq!(count_training(&db), 1);
        assert_eq!(delete_training(&mut db).unwrap(), 1);
        assert_eq!(db.list_cases().unwrap().len(), 1);
        assert!(db.get_case("MINE").is_ok());
    }

    #[test]
    fn seed_without_555_or_licence_is_refused() {
        let mut s = seed();
        s.cases[0].contacts[0].phone = "612-555".into();
        assert!(check(&s).is_ok());
        s.cases[0].contacts[0].phone = "612-123-4567".into();
        assert!(check(&s).is_err());
        let mut s = seed();
        s.sample_runs.insert("r".into(), json!({"label":"x"}));
        assert!(check(&s).is_err());
    }
}
