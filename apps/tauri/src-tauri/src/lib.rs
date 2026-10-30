use command_db::{ContactInput, CreateCase, Database, SubjectInput};
use command_reports::{export_pdf, Audience, Kind, ReportRequest, Statement};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

struct AppState(Mutex<Database>);

#[derive(Debug, Deserialize)]
struct WebContact {
    name: String,
    phone: String,
    relationship: String,
}

#[derive(Debug, Deserialize)]
struct WebCreateCase {
    mission_id: String,
    domain: String,
    objective: String,
    priority: String,
    subject_name: String,
    subject_age: Option<u8>,
    medical_conditions: Vec<String>,
    lkp_lat: Option<f64>,
    lkp_lon: Option<f64>,
    lkp_description: Option<String>,
    lkp_time: Option<String>,
    contacts: Vec<WebContact>,
}

#[derive(Debug, Deserialize)]
struct WebLogInput {
    time: String,
    role: String,
    text: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WebTeamInput {
    name: String,
    callsign: String,
    role: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct WebLog {
    id: String,
    case_id: String,
    time: String,
    role: String,
    text: String,
    lat: Option<f64>,
    lon: Option<f64>,
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct WebTeamMember {
    id: String,
    case_id: String,
    name: String,
    callsign: String,
    role: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct WebCase {
    id: String,
    mission_id: String,
    domain: String,
    objective: String,
    priority: String,
    status: String,
    created_at: String,
    subject_name: String,
    subject_age: Option<u8>,
    medical_conditions: Vec<String>,
    lkp_lat: Option<f64>,
    lkp_lon: Option<f64>,
    lkp_description: Option<String>,
    lkp_time: Option<String>,
    contacts: Vec<WebContactOutput>,
    updates: Vec<WebLog>,
    clues: Vec<WebLog>,
    team_members: Vec<WebTeamMember>,
}

#[derive(Debug, Serialize)]
struct WebContactOutput {
    name: String,
    phone: String,
    relationship: String,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn web_case(db: &Database, id: &str) -> Result<WebCase, String> {
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

#[tauri::command]
fn list_cases(state: State<'_, AppState>) -> Result<Vec<WebCase>, String> {
    let db = state.0.lock().map_err(err)?;
    db.list_cases()
        .map_err(err)?
        .iter()
        .map(|item| web_case(&db, &item.id))
        .collect()
}

#[tauri::command]
fn get_case(id: String, state: State<'_, AppState>) -> Result<WebCase, String> {
    let db = state.0.lock().map_err(err)?;
    web_case(&db, &id)
}

#[tauri::command]
fn create_case(input: WebCreateCase, state: State<'_, AppState>) -> Result<WebCase, String> {
    let mut db = state.0.lock().map_err(err)?;
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
    web_case(&db, &case.id)
}

#[tauri::command]
fn add_update(
    case_id: String,
    input: WebLogInput,
    state: State<'_, AppState>,
) -> Result<WebLog, String> {
    let item = state
        .0
        .lock()
        .map_err(err)?
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

#[tauri::command]
fn add_clue(
    case_id: String,
    input: WebLogInput,
    state: State<'_, AppState>,
) -> Result<WebLog, String> {
    let position = match (input.lat, input.lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        (None, None) => None,
        _ => return Err("Latitude and longitude must be supplied together".into()),
    };
    let item = state
        .0
        .lock()
        .map_err(err)?
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

#[tauri::command]
fn add_team_member(
    case_id: String,
    input: WebTeamInput,
    state: State<'_, AppState>,
) -> Result<WebTeamMember, String> {
    let stored_name = format!("{} — {}", input.callsign.trim(), input.name.trim());
    let item = state
        .0
        .lock()
        .map_err(err)?
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

#[tauri::command]
fn export_case_json(case_id: String, state: State<'_, AppState>) -> Result<String, String> {
    state
        .0
        .lock()
        .map_err(err)?
        .export_case_json(&case_id)
        .map_err(err)
}

#[tauri::command]
fn export_report_pdf(
    case_id: String,
    report_type: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let db = state.0.lock().map_err(err)?;
    let export = db.export_case(&case_id).map_err(err)?;
    let audience = match report_type.as_str() {
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
        case_id,
        audience,
        statements,
    })
    .map_err(err)
}

pub fn run() {
    let project = ProjectDirs::from("org", "CESAROPS", "SAR Command")
        .expect("operating system has no application data directory");
    std::fs::create_dir_all(project.data_local_dir())
        .expect("failed to create application data directory");
    let db = Database::open(project.data_local_dir().join("sar-command.sqlite"))
        .expect("failed to open SAR Command database");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState(Mutex::new(db)))
        .invoke_handler(tauri::generate_handler![
            list_cases,
            get_case,
            create_case,
            add_update,
            add_clue,
            add_team_member,
            export_case_json,
            export_report_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running CESAROPS SAR Command");
}
