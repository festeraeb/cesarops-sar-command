//! Desktop (offline) shell. Commands are thin wrappers over `command-api`
//! (the same code the web host calls), plus the module list.
use command_api::modules::{ModuleInfo, ModulesConfig, Registry};
use command_api::*;
use command_db::Database;
use directories::ProjectDirs;
use std::sync::Mutex;
use tauri::State;

struct AppState {
    db: Mutex<Database>,
    registry: Registry,
}

fn lock<'a>(s: &'a State<'_, AppState>) -> Result<std::sync::MutexGuard<'a, Database>, String> {
    s.db.lock().map_err(err)
}

#[tauri::command]
fn list_modules(state: State<'_, AppState>) -> Vec<ModuleInfo> {
    state.registry.infos()
}
#[tauri::command]
fn list_cases(state: State<'_, AppState>) -> Result<Vec<WebCase>, String> {
    command_api::list_cases(&*lock(&state)?)
}
#[tauri::command]
fn get_case(id: String, state: State<'_, AppState>) -> Result<WebCase, String> {
    command_api::get_case(&*lock(&state)?, &id)
}
#[tauri::command]
fn create_case(input: WebCreateCase, state: State<'_, AppState>) -> Result<WebCase, String> {
    command_api::create_case(&mut *lock(&state)?, input)
}
#[tauri::command]
fn add_update(case_id: String, input: WebLogInput, state: State<'_, AppState>) -> Result<WebLog, String> {
    command_api::add_update(&mut *lock(&state)?, &case_id, input)
}
#[tauri::command]
fn add_clue(case_id: String, input: WebLogInput, state: State<'_, AppState>) -> Result<WebLog, String> {
    command_api::add_clue(&mut *lock(&state)?, &case_id, input)
}
#[tauri::command]
fn add_team_member(case_id: String, input: WebTeamInput, state: State<'_, AppState>) -> Result<WebTeamMember, String> {
    command_api::add_team_member(&mut *lock(&state)?, &case_id, input)
}
#[tauri::command]
fn export_case_json(case_id: String, state: State<'_, AppState>) -> Result<String, String> {
    command_api::export_case_json(&*lock(&state)?, &case_id)
}
#[tauri::command]
fn export_report_pdf(case_id: String, report_type: String, state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    command_api::export_report_pdf(&*lock(&state)?, &case_id, &report_type)
}

pub fn run() {
    let project = ProjectDirs::from("org", "CESAROPS", "SAR Command")
        .expect("operating system has no application data directory");
    std::fs::create_dir_all(project.data_local_dir())
        .expect("failed to create application data directory");
    let db = Database::open(project.data_local_dir().join("sar-command.sqlite"))
        .expect("failed to open SAR Command database");
    // Modules are enabled in modules.json next to the database; default = core modules.
    let cfg_path = project.config_dir().join("modules.json");
    let cfg = ModulesConfig::load(&cfg_path.to_string_lossy()).unwrap_or_default();
    let registry = Registry::new(&cfg).expect("invalid modules.json");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { db: Mutex::new(db), registry })
        .invoke_handler(tauri::generate_handler![
            list_modules, list_cases, get_case, create_case, add_update, add_clue,
            add_team_member, export_case_json, export_report_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running CESAROPS Field Command Unit");
}
