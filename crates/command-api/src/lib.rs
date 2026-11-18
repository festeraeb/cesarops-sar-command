//! Shared command logic for the Field Command Unit (FCU).
//!
//! The Tauri desktop shell and the HTTP web host both call this crate, so a
//! command has one implementation and one set of types. Capabilities are
//! MODULES (see `modules`) that register themselves and are enabled by config.

pub mod modules;
pub mod training;

use command_db::{Attachment, ContactInput, CreateCase, Database, SubjectInput};
use command_reports::{export_pdf, Audience, Kind, ReportRequest, Statement};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct WebContact {
    pub name: String,
    pub phone: String,
    pub relationship: String,
}

#[derive(Debug, Deserialize)]
pub struct WebCreateCase {
    pub mission_id: String,
    pub domain: String,
    pub objective: String,
    pub priority: String,
    pub subject_name: String,
    pub subject_age: Option<u8>,
    pub medical_conditions: Vec<String>,
    pub lkp_lat: Option<f64>,
    pub lkp_lon: Option<f64>,
    pub lkp_description: Option<String>,
    pub lkp_time: Option<String>,
    pub contacts: Vec<WebContact>,
}

#[derive(Debug, Deserialize)]
pub struct WebLogInput {
    pub time: String,
    pub role: String,
    pub text: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct WebTeamInput {
    pub name: String,
    pub callsign: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct WebLog {
    pub id: String,
    pub case_id: String,
    pub time: String,
    pub role: String,
    pub text: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct WebTeamMember {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub callsign: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct WebCase {
    pub id: String,
    pub mission_id: String,
    pub domain: String,
    pub objective: String,
    pub priority: String,
    pub status: String,
    pub created_at: String,
    pub subject_name: String,
    pub subject_age: Option<u8>,
    pub medical_conditions: Vec<String>,
    pub lkp_lat: Option<f64>,
    pub lkp_lon: Option<f64>,
    pub lkp_description: Option<String>,
    pub lkp_time: Option<String>,
    pub contacts: Vec<WebContactOutput>,
    pub updates: Vec<WebLog>,
    pub clues: Vec<WebLog>,
    pub team_members: Vec<WebTeamMember>,
    /// True when the case carries the "FICTITIOUS - training data" marker.
    pub training: bool,
    /// Sample runs, simulated placeholders, prepared family message (see command-db migration 002).
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Serialize)]
pub struct WebContactOutput {
    pub name: String,
    pub phone: String,
    pub relationship: String,
}

pub fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn web_case(db: &Database, id: &str) -> Result<WebCase, String> {
    let export = db.export_case(id).map_err(err)?;
    let subject = export.subjects.first().ok_or("Case has no subject")?;
    let lkp = export
        .clues
        .iter()
        .find(|clue| clue.text.starts_with("Last known position"));
    Ok(WebCase {
        id: export.case.id.clone(),
        mission_id: export.case.id,
        domain: export.case.domain,
        objective: export.case.objective,
        priority: export.case.priority,
        status: export.case.status,
        created_at: export.case.created_at,
        subject_name: subject.name.clone(),
        subject_age: subject.age,
        medical_conditions: subject.medical_conditions.clone(),
        lkp_lat: lkp.and_then(|item| item.lat),
        lkp_lon: lkp.and_then(|item| item.lon),
        lkp_description: lkp.map(|item| {
            item.text
                .trim_start_matches("Last known position: ")
                .to_owned()
        }),
        lkp_time: lkp.map(|item| item.recorded_at.clone()),
        contacts: export
            .contacts
            .into_iter()
            .map(|item| WebContactOutput {
                name: item.name,
                phone: item.phone.unwrap_or_default(),
                relationship: item.relationship.unwrap_or_default(),
            })
            .collect(),
        updates: export
            .updates
            .into_iter()
            .map(|item| WebLog {
                id: item.id,
                case_id: item.case_id,
                time: item.created_at,
                role: item.role,
                text: item.text,
                lat: None,
                lon: None,
                kind: "update",
            })
            .collect(),
        clues: export
            .clues
            .into_iter()
            .map(|item| WebLog {
                id: item.id,
                case_id: item.case_id,
                time: item.recorded_at,
                role: item.recorded_by,
                text: item.text,
                lat: item.lat,
                lon: item.lon,
                kind: "clue",
            })
            .collect(),
        training: export.attachments.iter().any(|a| a.kind == "training_marker"),
        attachments: export.attachments,
        team_members: export
            .team_members
            .into_iter()
            .map(|item| {
                let (callsign, name) = item
                    .name
                    .split_once(" — ")
                    .map_or(("", item.name.as_str()), |(callsign, name)| {
                        (callsign, name)
                    });
                WebTeamMember {
                    id: item.id,
                    case_id: item.case_id,
                    name: name.to_owned(),
                    callsign: callsign.to_owned(),
                    role: item.role,
                    status: item.status,
                }
            })
            .collect(),
    })
}

pub fn list_cases(db: &Database) -> Result<Vec<WebCase>, String> {
    db.list_cases()
        .map_err(err)?
        .iter()
        .map(|item| web_case(db, &item.id))
        .collect()
}

pub fn get_case(db: &Database, id: &str) -> Result<WebCase, String> {
    web_case(db, id)
}

pub fn create_case(db: &mut Database, input: WebCreateCase) -> Result<WebCase, String> {
    let case = db
        .create_case(CreateCase {
            id: Some(input.mission_id),
            domain: input.domain,
            objective: input.objective,
            priority: input.priority,
            play_id: None,
            subject: SubjectInput {
                name: input.subject_name,
                age: input.subject_age,
                clothing_description: None,
                medical_conditions: input.medical_conditions,
            },
            contacts: input
                .contacts
                .into_iter()
                .map(|item| ContactInput {
                    name: item.name,
                    phone: Some(item.phone),
                    relationship: Some(item.relationship),
                })
                .collect(),
        })
        .map_err(err)?;
    if let (Some(lat), Some(lon)) = (input.lkp_lat, input.lkp_lon) {
        db.add_clue(
            &case.id,
            &format!(
                "Last known position: {}",
                input
                    .lkp_description
                    .unwrap_or_else(|| "No description recorded".into())
            ),
            "intake",
            input.lkp_time.as_deref().unwrap_or(&case.created_at),
            Some((lat, lon)),
        )
        .map_err(err)?;
    }
    web_case(db, &case.id)
}

pub fn add_update(db: &mut Database, case_id: &str, input: WebLogInput) -> Result<WebLog, String> {
    let item = db
        .add_update(&case_id, &input.text, &input.role, &input.time)
        .map_err(err)?;
    Ok(WebLog {
        id: item.id,
        case_id: item.case_id,
        time: item.created_at,
        role: item.role,
        text: item.text,
        lat: None,
        lon: None,
        kind: "update",
    })
}

pub fn add_clue(db: &mut Database, case_id: &str, input: WebLogInput) -> Result<WebLog, String> {
    let position = match (input.lat, input.lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        (None, None) => None,
        _ => return Err("Latitude and longitude must be supplied together".into()),
    };
    let item = db
        .add_clue(&case_id, &input.text, &input.role, &input.time, position)
        .map_err(err)?;
    Ok(WebLog {
        id: item.id,
        case_id: item.case_id,
        time: item.recorded_at,
        role: item.recorded_by,
        text: item.text,
        lat: item.lat,
        lon: item.lon,
        kind: "clue",
    })
}

pub fn add_team_member(db: &mut Database, case_id: &str, input: WebTeamInput) -> Result<WebTeamMember, String> {
    let stored_name = format!("{} — {}", input.callsign.trim(), input.name.trim());
    let item = db
        .add_team_member(&case_id, &stored_name, &input.role, &input.status)
        .map_err(err)?;
    Ok(WebTeamMember {
        id: item.id,
        case_id: item.case_id,
        name: input.name,
        callsign: input.callsign,
        role: item.role,
        status: item.status,
    })
}

pub fn export_case_json(db: &Database, case_id: &str) -> Result<String, String> {
    db.export_case_json(case_id).map_err(err)
}

pub fn export_report_pdf(db: &Database, case_id: &str, report_type: &str) -> Result<Vec<u8>, String> {
    let export = db.export_case(&case_id).map_err(err)?;
    let audience = match report_type {
        "family" => Audience::Family,
        "team" => Audience::Team,
        "ic" => Audience::IncidentCommander,
        _ => return Err("Unknown report type".into()),
    };
    let mut statements = Vec::new();
    for update in export.updates {
        statements.push(Statement {
            text: update.text,
            kind: Kind::Observed,
            ledger_id: Some(update.id),
            command: None,
            uncertainty: None,
        });
    }
    for clue in export.clues {
        statements.push(Statement {
            text: clue.text,
            kind: Kind::Observed,
            ledger_id: Some(clue.id),
            command: None,
            uncertainty: None,
        });
    }
    if statements.is_empty() {
        return Err("Add at least one verified update or clue before exporting a report".into());
    }
    export_pdf(&ReportRequest {
        case_id: case_id.to_owned(),
        audience,
        statements,
    })
    .map_err(err)
}