//! FCU web host: every Tauri command as an HTTP JSON endpoint, same crates,
//! plus the built front end. One binary, two configs:
//!
//! * `tester`: per-user persistent case stores. The user name arrives in a
//!   header set by nginx after the login gate; the server refuses requests
//!   without it (or not on `allowed_users`). Stores are separate SQLite files.
//! * `demo`: no login, throwaway in-memory sessions seeded with fictional
//!   incidents, nothing written to disk, no outbound calls, rate limited.
//!
//! API (paths relative to the site root):
//!   GET  api/config        mode, banner, product name, user
//!   GET  api/modules       enabled modules (id, name, version, provides, transports)
//!   POST api/call/<cmd>    body = the Tauri invoke args object; reply = JSON result
//!   POST api/reset         demo only: discard this visitor's session

mod demo;

use command_api::training;

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use command_api::modules::{ModulesConfig, Registry};
use command_db::Database;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Deserialize, Clone)]
struct Config {
    mode: String,
    listen: String,
    static_dir: String,
    #[serde(default)]
    data_dir: Option<String>,
    #[serde(default = "default_header")]
    user_header: String,
    #[serde(default)]
    allowed_users: Vec<String>,
    #[serde(default)]
    banner: String,
    /// URL path the site is mounted at (cookie path), e.g. "/sar-demo/".
    #[serde(default = "default_base")]
    base_path: String,
    #[serde(default = "default_rate")]
    rate_per_min: u32,
    #[serde(default = "default_sessions")]
    max_sessions: usize,
    #[serde(default = "default_ttl")]
    session_ttl_min: u64,
    #[serde(default = "default_cases")]
    max_cases_per_session: usize,
    #[serde(default)]
    modules: Option<ModulesConfig>,
    /// Demo: seed file (JSON) every new visitor session starts with. Falls back to the built-in incidents.
    #[serde(default)]
    demo_seed: Option<String>,
    /// Tester: seed file behind the "Load training cases (fictitious)" action. Never loaded automatically.
    #[serde(default)]
    training_seed: Option<String>,
    /// Demo: walkthrough film shown on the start screen {url, poster, caption}.
    #[serde(default)]
    start_film: Option<Value>,
}
fn default_header() -> String { "x-sar-user".into() }
fn default_base() -> String { "/".into() }
fn default_rate() -> u32 { 240 }
fn default_sessions() -> usize { 200 }
fn default_ttl() -> u64 { 45 }
fn default_cases() -> usize { 10 }

type Db = Arc<Mutex<Database>>;

struct App {
    cfg: Config,
    seed: Option<training::Seed>,
    registry: Registry,
    /// tester: user -> store. demo: session id -> (store, last used)
    users: Mutex<HashMap<String, Db>>,
    sessions: Mutex<HashMap<String, (Db, Instant)>>,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

fn fail(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}

fn valid_user(u: &str) -> bool {
    !u.is_empty() && u.len() <= 40 && u.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn client_ip(h: &HeaderMap) -> String {
    h.get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("local")
        .chars()
        .take(64)
        .collect()
}

impl App {
    fn rate_ok(&self, ip: &str) -> bool {
        let mut hits = self.hits.lock().unwrap();
        let now = Instant::now();
        if hits.len() > 5000 {
            hits.retain(|_, (t, _)| now.duration_since(*t) < Duration::from_secs(60));
        }
        let e = hits.entry(ip.to_owned()).or_insert((now, 0));
        if now.duration_since(e.0) >= Duration::from_secs(60) {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= self.cfg.rate_per_min
    }

    /// The tester's user name from the gate header, or an error response.
    fn tester(&self, h: &HeaderMap) -> Result<String, Response> {
        let u = h
            .get(self.cfg.user_header.as_str())
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if !valid_user(&u) {
            return Err(fail(StatusCode::UNAUTHORIZED, "sign in required"));
        }
        if !self.cfg.allowed_users.is_empty() && !self.cfg.allowed_users.contains(&u) {
            return Err(fail(StatusCode::FORBIDDEN, "this account has no access to the Field Command Unit"));
        }
        Ok(u)
    }

    fn user_db(&self, user: &str) -> Result<Db, String> {
        let mut m = self.users.lock().unwrap();
        if let Some(d) = m.get(user) {
            return Ok(d.clone());
        }
        let dir = PathBuf::from(self.cfg.data_dir.as_deref().ok_or("data_dir not set")?)
            .join("users")
            .join(user);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let db = Arc::new(Mutex::new(Database::open(dir.join("sar-command.sqlite")).map_err(|e| e.to_string())?));
        m.insert(user.to_owned(), db.clone());
        Ok(db)
    }

    /// Demo: find or create the visitor's session. Returns (db, new cookie id).
    fn demo_db(&self, h: &HeaderMap, fresh: bool) -> Result<(Db, Option<String>), Response> {
        let cookie_id = h
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').filter_map(|p| p.trim().strip_prefix("fcu_demo=")).next())
            .map(str::to_owned);
        let mut s = self.sessions.lock().unwrap();
        let now = Instant::now();
        let ttl = Duration::from_secs(self.cfg.session_ttl_min * 60);
        s.retain(|_, (_, t)| now.duration_since(*t) < ttl);
        if !fresh {
            if let Some(id) = &cookie_id {
                if let Some((db, t)) = s.get_mut(id) {
                    *t = now;
                    return Ok((db.clone(), None));
                }
            }
        }
        if let Some(id) = &cookie_id {
            s.remove(id);
        }
        if s.len() >= self.cfg.max_sessions {
            return Err(fail(StatusCode::SERVICE_UNAVAILABLE, "demo is busy, try again in a few minutes"));
        }
        let db = match &self.seed {
            Some(seed) => (|| {
                let mut d = Database::open_in_memory().map_err(|e| e.to_string())?;
                training::apply(&mut d, seed)?;
                Ok::<_, String>(d)
            })(),
            None => demo::seeded(),
        }
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
        let db = Arc::new(Mutex::new(db));
        let id = uuid::Uuid::new_v4().to_string();
        s.insert(id.clone(), (db.clone(), now));
        Ok((db, Some(id)))
    }

    fn cookie(&self, id: &str) -> HeaderValue {
        HeaderValue::from_str(&format!(
            "fcu_demo={id}; Path={}; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
            self.cfg.base_path,
            self.cfg.session_ttl_min * 60
        ))
        .unwrap()
    }
}

async fn config(State(app): State<Arc<App>>, h: HeaderMap) -> Response {
    let user = if app.cfg.mode == "tester" {
        match app.tester(&h) {
            Ok(u) => Some(u),
            Err(r) => return r,
        }
    } else {
        None
    };
    Json(json!({
        "product": "Field Command Unit",
        "mode": app.cfg.mode,
        "banner": app.cfg.banner,
        "user": user,
        "training": app.cfg.mode == "tester" && app.seed.is_some(),
        "start_film": app.cfg.start_film,
    }))
    .into_response()
}

async fn modules(State(app): State<Arc<App>>, h: HeaderMap) -> Response {
    if app.cfg.mode == "tester" {
        if let Err(r) = app.tester(&h) {
            return r;
        }
    }
    Json(app.registry.infos()).into_response()
}

async fn call(State(app): State<Arc<App>>, Path(cmd): Path<String>, h: HeaderMap, body: Option<Json<Value>>) -> Response {
    if !app.rate_ok(&client_ip(&h)) {
        return fail(StatusCode::TOO_MANY_REQUESTS, "too many requests, slow down");
    }
    let args = body.map(|b| b.0).unwrap_or(Value::Null);
    let (db, new_cookie) = if app.cfg.mode == "tester" {
        let user = match app.tester(&h) {
            Ok(u) => u,
            Err(r) => return r,
        };
        match app.user_db(&user) {
            Ok(d) => (d, None),
            Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    } else {
        match app.demo_db(&h, false) {
            Ok(x) => x,
            Err(r) => return r,
        }
    };
    let result = {
        let mut guard = db.lock().unwrap();
        if app.cfg.mode == "demo" && cmd == "create_case" {
            match guard.list_cases() {
                Ok(l) if l.len() >= app.cfg.max_cases_per_session => {
                    return fail(StatusCode::BAD_REQUEST, "demo limit reached: reset the demo to start over");
                }
                _ => {}
            }
        }
        app.registry.call(&mut guard, &cmd, &args)
    };
    let mut resp = match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::BAD_REQUEST, &e),
    };
    if let Some(id) = new_cookie {
        resp.headers_mut().append(header::SET_COOKIE, app.cookie(&id));
    }
    resp
}

/// Tester only: status / load / delete of the fictitious training cases in the caller's own store.
async fn training_action(State(app): State<Arc<App>>, Path(action): Path<String>, h: HeaderMap) -> Response {
    if app.cfg.mode != "tester" {
        return fail(StatusCode::NOT_FOUND, "not found");
    }
    let user = match app.tester(&h) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if !app.rate_ok(&client_ip(&h)) {
        return fail(StatusCode::TOO_MANY_REQUESTS, "too many requests, slow down");
    }
    let Some(seed) = &app.seed else {
        return fail(StatusCode::NOT_FOUND, "no training set is configured");
    };
    let db = match app.user_db(&user) {
        Ok(d) => d,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    let mut guard = db.lock().unwrap();
    let r = match action.as_str() {
        "status" => Ok(json!({})),
        "load" => training::apply(&mut guard, seed).map(|(l, s)| json!({ "loaded": l, "skipped": s })),
        "delete" => training::delete_training(&mut guard).map(|n| json!({ "deleted": n })),
        _ => return fail(StatusCode::NOT_FOUND, "not found"),
    };
    match r {
        Ok(mut v) => {
            v["in_store"] = json!(training::count_training(&guard));
            v["in_set"] = json!(seed.cases.len());
            Json(v).into_response()
        }
        Err(e) => fail(StatusCode::BAD_REQUEST, &e),
    }
}

async fn reset(State(app): State<Arc<App>>, h: HeaderMap) -> Response {
    if app.cfg.mode != "demo" {
        return fail(StatusCode::NOT_FOUND, "not found");
    }
    if !app.rate_ok(&client_ip(&h)) {
        return fail(StatusCode::TOO_MANY_REQUESTS, "too many requests, slow down");
    }
    match app.demo_db(&h, true) {
        Ok((_, id)) => {
            let mut r = Json(json!({ "ok": true })).into_response();
            if let Some(id) = id {
                r.headers_mut().append(header::SET_COOKIE, app.cookie(&id));
            }
            r
        }
        Err(r) => r,
    }
}

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("--build-training") {
        // fcu-server --build-training SEED.json OUT.sqlite : a test database built from the training seed
        let (seed_path, out) = (argv.get(2).expect("seed path"), argv.get(3).expect("output path"));
        let seed = training::load_seed(seed_path).expect("seed");
        let _ = std::fs::remove_file(out);
        let mut db = Database::open(out).expect("open output");
        let (l, _) = training::apply(&mut db, &seed).expect("apply");
        eprintln!("built {out}: {l} fictitious training cases");
        return;
    }
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: fcu-server CONFIG.json");
        std::process::exit(2)
    });
    let cfg: Config = serde_json::from_str(&std::fs::read_to_string(&path).expect("read config")).expect("parse config");
    assert!(cfg.mode == "tester" || cfg.mode == "demo", "mode must be tester or demo");
    if cfg.mode == "tester" {
        assert!(cfg.data_dir.is_some(), "tester mode needs data_dir");
    }
    let mcfg = cfg.modules.clone().unwrap_or_default();
    let registry = Registry::new(&mcfg).expect("modules config");
    let seed_path = if cfg.mode == "demo" { cfg.demo_seed.clone() } else { cfg.training_seed.clone() };
    let seed = seed_path.map(|p| training::load_seed(&p).expect("training seed"));
    let listen = cfg.listen.clone();
    let static_dir = cfg.static_dir.clone();
    let app = Arc::new(App {
        cfg,
        seed,
        registry,
        users: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
        hits: Mutex::new(HashMap::new()),
    });
    let index = format!("{static_dir}/index.html");
    let router = Router::new()
        .route("/api/config", get(config))
        .route("/api/modules", get(modules))
        .route("/api/call/:cmd", post(call))
        .route("/api/reset", post(reset))
        .route("/api/training/:action", post(training_action))
        .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(index)))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind");
    eprintln!("fcu-server {} on {}", path, listen);
    axum::serve(listener, router).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }).await.unwrap();
}
