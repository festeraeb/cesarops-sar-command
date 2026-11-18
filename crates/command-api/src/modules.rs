//! Module registry. Every capability is a module that registers itself
//! (id, name, version, what it provides) and is reached through an adapter
//! with up to two transports:
//!
//! * `local` - offline: in-process crate, Tauri invoke, or a local binary
//! * `cloud` - HTTP to a hosted service
//!
//! A team "pulls in" a module by listing its id in `modules.json`
//! (`{"enabled": ["core-cases", ...]}`); the UI shows only enabled modules.
//! A transport or function that does not work is reported as
//! `available: false` ("not available yet"). Nothing here returns canned or
//! simulated results.

use command_db::Database;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct TransportInfo {
    /// "local" or "cloud"
    pub kind: &'static str,
    pub available: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub name: &'static str,
    pub available: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    /// "core" (built in) or "external"
    pub kind: &'static str,
    pub description: &'static str,
    /// Commands callable through the adapter (empty for a module with none yet).
    pub provides: Vec<&'static str>,
    pub transports: Vec<TransportInfo>,
    pub functions: Vec<FunctionInfo>,
    /// Optional link the UI may offer (e.g. the hosted review site).
    pub link: Option<String>,
}

/// Adapter interface every module implements.
pub trait Module: Send + Sync {
    fn info(&self) -> ModuleInfo;
    /// Call one of `info().provides`. `db` is the caller's case store
    /// (external modules may ignore it).
    fn call(&self, db: &mut Database, cmd: &str, args: &Value) -> Result<Value, String>;
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SonarSnifferConfig {
    /// Path to the local `parse_cli` binary (laptop).
    pub local_binary: Option<String>,
    /// Review-site URL (cloud transport).
    pub cloud_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModulesConfig {
    pub enabled: Vec<String>,
    #[serde(default)]
    pub sonarsniffer: SonarSnifferConfig,
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            enabled: vec!["core-cases".into(), "core-reports".into()],
            sonarsniffer: SonarSnifferConfig::default(),
        }
    }
}

impl ModulesConfig {
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("bad argument '{key}': {e}"))
}
fn json<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn local_ok(note: &str) -> TransportInfo {
    TransportInfo { kind: "local", available: true, note: note.into() }
}

/// Core module: cases, intake record, updates, clues, team roster (crate command-db).
pub struct CoreCases;
impl Module for CoreCases {
    fn info(&self) -> ModuleInfo {
        ModuleInfo {
            id: "core-cases",
            name: "Cases, intake and teams",
            version: "0.1.0",
            kind: "core",
            description: "Call intake, mission log, clues, team dispatch, case export.",
            provides: vec![
                "list_cases", "get_case", "create_case", "add_update", "add_clue",
                "add_team_member", "export_case_json",
            ],
            transports: vec![local_ok("in-process case store (SQLite)")],
            functions: vec![],
            link: None,
        }
    }
    fn call(&self, db: &mut Database, cmd: &str, a: &Value) -> Result<Value, String> {
        use crate::*;
        match cmd {
            "list_cases" => json(list_cases(db)?),
            "get_case" => json(get_case(db, &parse::<String>(a, "id")?)?),
            "create_case" => json(create_case(db, parse(a, "input")?)?),
            "add_update" => json(add_update(db, &parse::<String>(a, "caseId")?, parse(a, "input")?)?),
            "add_clue" => json(add_clue(db, &parse::<String>(a, "caseId")?, parse(a, "input")?)?),
            "add_team_member" => json(add_team_member(db, &parse::<String>(a, "caseId")?, parse(a, "input")?)?),
            "export_case_json" => json(export_case_json(db, &parse::<String>(a, "caseId")?)?),
            _ => Err(format!("unknown command {cmd}")),
        }
    }
}

/// Core module: truth-gated family / team / IC reports (crates command-reports, sar-family-report).
pub struct CoreReports;
impl Module for CoreReports {
    fn info(&self) -> ModuleInfo {
        ModuleInfo {
            id: "core-reports",
            name: "Stakeholder reports",
            version: "0.1.0",
            kind: "core",
            description: "Family, team and incident-commander reports (PDF).",
            provides: vec!["export_report_pdf"],
            transports: vec![local_ok("in-process report generator")],
            functions: vec![],
            link: None,
        }
    }
    fn call(&self, db: &mut Database, cmd: &str, a: &Value) -> Result<Value, String> {
        match cmd {
            "export_report_pdf" => json(crate::export_report_pdf(
                db,
                &parse::<String>(a, "caseId")?,
                &parse::<String>(a, "reportType")?,
            )?),
            _ => Err(format!("unknown command {cmd}")),
        }
    }
}

/// External module: SonarSniffer. Adapter interface with a local transport
/// (the `parse_cli` binary on the laptop) and a cloud transport (the review
/// site). Today nothing here can process a recording from the FCU, so no
/// function is callable: the UI shows each as "not available yet". The probes
/// below report only facts (file present, URL configured).
pub struct SonarSniffer(pub SonarSnifferConfig);
impl Module for SonarSniffer {
    fn info(&self) -> ModuleInfo {
        let local = match &self.0.local_binary {
            None => TransportInfo { kind: "local", available: false, note: "no parse_cli path configured".into() },
            Some(p) => {
                let ok = std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false);
                TransportInfo {
                    kind: "local",
                    available: false,
                    note: if ok {
                        "parse_cli found; running it from the FCU is not available yet".into()
                    } else {
                        "configured parse_cli not found on this machine".into()
                    },
                }
            }
        };
        let cloud = match &self.0.cloud_url {
            None => TransportInfo { kind: "cloud", available: false, note: "no review-site URL configured".into() },
            Some(_) => TransportInfo {
                kind: "cloud",
                available: false,
                note: "the review site is a separate login; sending a recording from the FCU is not available yet".into(),
            },
        };
        let f = |name| FunctionInfo { name, available: false, note: "not available yet".into() };
        ModuleInfo {
            id: "sonarsniffer",
            name: "SonarSniffer",
            version: "0.0.1",
            kind: "external",
            description: "Side-scan and magnetometer survey processing.",
            provides: vec![],
            transports: vec![local, cloud],
            functions: vec![f("process_recording"), f("attach_report_to_case")],
            link: self.0.cloud_url.clone(),
        }
    }
    fn call(&self, _db: &mut Database, cmd: &str, _a: &Value) -> Result<Value, String> {
        Err(format!("{cmd}: not available yet"))
    }
}

pub struct Registry {
    modules: Vec<Box<dyn Module>>,
}

impl Registry {
    pub fn new(cfg: &ModulesConfig) -> Result<Self, String> {
        let mut modules: Vec<Box<dyn Module>> = Vec::new();
        for id in &cfg.enabled {
            modules.push(match id.as_str() {
                "core-cases" => Box::new(CoreCases),
                "core-reports" => Box::new(CoreReports),
                "sonarsniffer" => Box::new(SonarSniffer(cfg.sonarsniffer.clone())),
                other => return Err(format!("unknown module '{other}' in config")),
            });
        }
        Ok(Self { modules })
    }
    pub fn infos(&self) -> Vec<ModuleInfo> {
        self.modules.iter().map(|m| m.info()).collect()
    }
    pub fn call(&self, db: &mut Database, cmd: &str, args: &Value) -> Result<Value, String> {
        for m in &self.modules {
            if m.info().provides.contains(&cmd) {
                return m.call(db, cmd, args);
            }
        }
        Err(format!("no enabled module provides '{cmd}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(ids: &[&str]) -> ModulesConfig {
        ModulesConfig { enabled: ids.iter().map(|s| s.to_string()).collect(), sonarsniffer: Default::default() }
    }

    #[test]
    fn disabled_module_is_not_callable() {
        let r = Registry::new(&cfg(&["core-cases"])).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        assert!(r.call(&mut db, "list_cases", &json!({})).is_ok());
        assert!(r.call(&mut db, "export_report_pdf", &json!({"caseId":"x","reportType":"family"})).is_err());
        assert_eq!(r.infos().len(), 1);
    }

    #[test]
    fn unknown_module_in_config_is_an_error() {
        assert!(Registry::new(&cfg(&["nope"])).is_err());
    }

    #[test]
    fn sonarsniffer_reports_not_available_and_never_fakes() {
        let r = Registry::new(&cfg(&["sonarsniffer"])).unwrap();
        let info = &r.infos()[0];
        assert!(info.transports.iter().all(|t| !t.available));
        assert!(info.functions.iter().all(|f| !f.available));
        let mut db = Database::open_in_memory().unwrap();
        assert!(r.call(&mut db, "process_recording", &json!({})).is_err());
    }

    #[test]
    fn create_and_read_case_through_the_registry() {
        let r = Registry::new(&ModulesConfig::default()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        let input = json!({"input":{"mission_id":"T-1","domain":"water","objective":"vessel_sar","priority":"high",
            "subject_name":"Test","medical_conditions":[],"lkp_lat":43.0,"lkp_lon":-87.0,"contacts":[]}});
        r.call(&mut db, "create_case", &input).unwrap();
        let got = r.call(&mut db, "get_case", &json!({"id":"T-1"})).unwrap();
        assert_eq!(got["subject_name"], "Test");
    }
}
