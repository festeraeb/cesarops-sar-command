//! Offline SQLite persistence for the command-post domain.
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

const MIGRATION_1: &str = include_str!("../migrations/001_initial.sql");

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("case {0} was not found")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubjectInput {
    pub name: String,
    pub age: Option<u8>,
    #[serde(default)]
    pub clothing_description: Option<String>,
    #[serde(default)]
    pub medical_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContactInput {
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub relationship: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateCase {
    pub id: Option<String>,
    pub domain: String,
    pub objective: String,
    pub priority: String,
    #[serde(default)]
    pub play_id: Option<String>,
    pub subject: SubjectInput,
    #[serde(default)]
    pub contacts: Vec<ContactInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Case {
    pub id: String,
    pub domain: String,
    pub objective: String,
    pub priority: String,
    pub status: String,
    pub play_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Subject {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub age: Option<u8>,
    pub clothing_description: Option<String>,
    pub medical_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Contact {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub relationship: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clue {
    pub id: String,
    pub case_id: String,
    pub text: String,
    pub recorded_by: String,
    pub recorded_at: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Update {
    pub id: String,
    pub case_id: String,
    pub text: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Assignment {
    pub id: String,
    pub case_id: String,
    pub title: String,
    pub team_id: Option<String>,
    pub status: String,
    pub geometry: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamMember {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseExport {
    pub case: Case,
    pub subjects: Vec<Subject>,
    pub contacts: Vec<Contact>,
    pub assignments: Vec<Assignment>,
    pub clues: Vec<Clue>,
    pub updates: Vec<Update>,
    pub team_members: Vec<TeamMember>,
}

pub struct Database {
    conn: Connection,
}

fn required(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(DbError::Validation(format!("{field} must not be blank")))
    } else {
        Ok(value.to_owned())
    }
}

fn optional(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn valid_time(value: &str, field: &str) -> Result<String> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| DbError::Validation(format!("{field} must be an RFC 3339 timestamp")))?;
    Ok(value.to_owned())
}

fn id() -> String {
    Uuid::new_v4().to_string()
}

fn ensure_case(tx: &Transaction<'_>, case_id: &str) -> Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM cases WHERE id = ?1)",
        [case_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(DbError::NotFound(case_id.to_owned()))
    }
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(MIGRATION_1)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, ?1)",
            [Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn create_case(&mut self, input: CreateCase) -> Result<Case> {
        let domain = required(&input.domain, "domain")?;
        if !matches!(domain.as_str(), "water" | "land") {
            return Err(DbError::Validation("domain must be water or land".into()));
        }
        let priority = required(&input.priority, "priority")?;
        if !matches!(priority.as_str(), "low" | "medium" | "high" | "critical") {
            return Err(DbError::Validation("invalid priority".into()));
        }
        let objective = required(&input.objective, "objective")?;
        if !matches!(
            objective.as_str(),
            "wreck_hunt" | "vessel_sar" | "aircraft_sar" | "vehicle_sar" | "person_search"
        ) {
            return Err(DbError::Validation("invalid objective".into()));
        }
        let subject_name = required(&input.subject.name, "subject.name")?;
        let case_id = match input.id {
            Some(value) => required(&value, "id")?,
            None => id(),
        };
        if case_id.len() > 200 {
            return Err(DbError::Validation("id is too long".into()));
        }
        for condition in &input.subject.medical_conditions {
            required(condition, "medical condition")?;
        }
        for contact in &input.contacts {
            required(&contact.name, "contact.name")?;
        }

        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO cases(id, domain, objective, priority, status, play_id, created_at)
             VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6)",
            params![
                case_id,
                domain,
                objective,
                priority,
                optional(input.play_id),
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO subjects(id, case_id, name, age, clothing_description, medical_conditions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id(),
                case_id,
                subject_name,
                input.subject.age,
                optional(input.subject.clothing_description),
                serde_json::to_string(&input.subject.medical_conditions)?
            ],
        )?;
        for contact in input.contacts {
            tx.execute(
                "INSERT INTO contacts(id, case_id, name, phone, relationship) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id(),
                    case_id,
                    contact.name.trim(),
                    optional(contact.phone),
                    optional(contact.relationship)
                ],
            )?;
        }
        tx.commit()?;
        self.get_case(&case_id)
    }

    pub fn list_cases(&self) -> Result<Vec<Case>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, domain, objective, priority, status, play_id, created_at
             FROM cases ORDER BY created_at DESC, id",
        )?;
        let rows = stmt.query_map([], map_case)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_case(&self, case_id: &str) -> Result<Case> {
        self.conn
            .query_row(
                "SELECT id, domain, objective, priority, status, play_id, created_at
                 FROM cases WHERE id = ?1",
                [case_id],
                map_case,
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(case_id.to_owned()))
    }

    pub fn add_clue(
        &mut self,
        case_id: &str,
        text: &str,
        recorded_by: &str,
        recorded_at: &str,
        position: Option<(f64, f64)>,
    ) -> Result<Clue> {
        let text = required(text, "clue.text")?;
        let recorded_by = required(recorded_by, "clue.recorded_by")?;
        let recorded_at = valid_time(recorded_at, "clue.recorded_at")?;
        let (lat, lon) = match position {
            Some((lat, lon))
                if lat.is_finite()
                    && lon.is_finite()
                    && (-90.0..=90.0).contains(&lat)
                    && (-180.0..=180.0).contains(&lon) =>
            {
                (Some(lat), Some(lon))
            }
            Some(_) => return Err(DbError::Validation("clue coordinates are invalid".into())),
            None => (None, None),
        };
        let clue = Clue {
            id: id(),
            case_id: required(case_id, "case_id")?,
            text,
            recorded_by,
            recorded_at,
            lat,
            lon,
        };
        let tx = self.conn.transaction()?;
        ensure_case(&tx, &clue.case_id)?;
        tx.execute(
            "INSERT INTO clues(id, case_id, text, recorded_by, recorded_at, lat, lon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                clue.id,
                clue.case_id,
                clue.text,
                clue.recorded_by,
                clue.recorded_at,
                clue.lat,
                clue.lon
            ],
        )?;
        tx.commit()?;
        Ok(clue)
    }

    pub fn add_update(
        &mut self,
        case_id: &str,
        text: &str,
        role: &str,
        created_at: &str,
    ) -> Result<Update> {
        let update = Update {
            id: id(),
            case_id: required(case_id, "case_id")?,
            text: required(text, "update.text")?,
            role: required(role, "update.role")?,
            created_at: valid_time(created_at, "update.created_at")?,
        };
        let tx = self.conn.transaction()?;
        ensure_case(&tx, &update.case_id)?;
        tx.execute(
            "INSERT INTO updates(id, case_id, text, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                update.id,
                update.case_id,
                update.text,
                update.role,
                update.created_at
            ],
        )?;
        tx.commit()?;
        Ok(update)
    }

    pub fn add_assignment(
        &mut self,
        case_id: &str,
        title: &str,
        team_id: Option<String>,
        status: &str,
        geometry: Option<serde_json::Value>,
    ) -> Result<Assignment> {
        if geometry.as_ref().is_some_and(|v| !v.is_object()) {
            return Err(DbError::Validation(
                "assignment geometry must be a JSON object".into(),
            ));
        }
        let assignment = Assignment {
            id: id(),
            case_id: required(case_id, "case_id")?,
            title: required(title, "assignment.title")?,
            team_id: optional(team_id),
            status: required(status, "assignment.status")?,
            geometry,
            created_at: Utc::now().to_rfc3339(),
        };
        let geometry_json = assignment
            .geometry
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let tx = self.conn.transaction()?;
        ensure_case(&tx, &assignment.case_id)?;
        tx.execute(
            "INSERT INTO assignments(id, case_id, title, team_id, status, geometry_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![assignment.id, assignment.case_id, assignment.title, assignment.team_id, assignment.status, geometry_json, assignment.created_at],
        )?;
        tx.commit()?;
        Ok(assignment)
    }

    pub fn add_team_member(
        &mut self,
        case_id: &str,
        name: &str,
        role: &str,
        status: &str,
    ) -> Result<TeamMember> {
        let member = TeamMember {
            id: id(),
            case_id: required(case_id, "case_id")?,
            name: required(name, "team_member.name")?,
            role: required(role, "team_member.role")?,
            status: required(status, "team_member.status")?,
        };
        let tx = self.conn.transaction()?;
        ensure_case(&tx, &member.case_id)?;
        tx.execute(
            "INSERT INTO team_members(id, case_id, name, role, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                member.id,
                member.case_id,
                member.name,
                member.role,
                member.status
            ],
        )?;
        tx.commit()?;
        Ok(member)
    }

    pub fn export_case(&self, case_id: &str) -> Result<CaseExport> {
        let case = self.get_case(case_id)?;
        Ok(CaseExport {
            subjects: self.subjects(case_id)?,
            contacts: self.contacts(case_id)?,
            assignments: self.assignments(case_id)?,
            clues: self.clues(case_id)?,
            updates: self.updates(case_id)?,
            team_members: self.team_members(case_id)?,
            case,
        })
    }

    pub fn export_case_json(&self, case_id: &str) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.export_case(case_id)?)?)
    }

    fn subjects(&self, case_id: &str) -> Result<Vec<Subject>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, name, age, clothing_description, medical_conditions
             FROM subjects WHERE case_id = ?1 ORDER BY rowid",
        )?;
        let raw = stmt
            .query_map([case_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<u8>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(id, case_id, name, age, clothing_description, medical)| {
                Ok(Subject {
                    id,
                    case_id,
                    name,
                    age,
                    clothing_description,
                    medical_conditions: serde_json::from_str(&medical)?,
                })
            })
            .collect()
    }

    fn contacts(&self, case_id: &str) -> Result<Vec<Contact>> {
        query_simple(
            &self.conn,
            "SELECT id, case_id, name, phone, relationship FROM contacts WHERE case_id = ?1 ORDER BY rowid",
            case_id,
            |r| Ok(Contact { id: r.get(0)?, case_id: r.get(1)?, name: r.get(2)?, phone: r.get(3)?, relationship: r.get(4)? }),
        )
    }

    fn clues(&self, case_id: &str) -> Result<Vec<Clue>> {
        query_simple(
            &self.conn,
            "SELECT id, case_id, text, recorded_by, recorded_at, lat, lon FROM clues WHERE case_id = ?1 ORDER BY recorded_at, rowid",
            case_id,
            |r| Ok(Clue { id: r.get(0)?, case_id: r.get(1)?, text: r.get(2)?, recorded_by: r.get(3)?, recorded_at: r.get(4)?, lat: r.get(5)?, lon: r.get(6)? }),
        )
    }

    fn updates(&self, case_id: &str) -> Result<Vec<Update>> {
        query_simple(
            &self.conn,
            "SELECT id, case_id, text, role, created_at FROM updates WHERE case_id = ?1 ORDER BY created_at, rowid",
            case_id,
            |r| Ok(Update { id: r.get(0)?, case_id: r.get(1)?, text: r.get(2)?, role: r.get(3)?, created_at: r.get(4)? }),
        )
    }

    fn assignments(&self, case_id: &str) -> Result<Vec<Assignment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, title, team_id, status, geometry_json, created_at
             FROM assignments WHERE case_id = ?1 ORDER BY rowid",
        )?;
        let raw = stmt
            .query_map([case_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(id, case_id, title, team_id, status, geometry, created_at)| {
                    Ok(Assignment {
                        id,
                        case_id,
                        title,
                        team_id,
                        status,
                        geometry: geometry.map(|v| serde_json::from_str(&v)).transpose()?,
                        created_at,
                    })
                },
            )
            .collect()
    }

    fn team_members(&self, case_id: &str) -> Result<Vec<TeamMember>> {
        query_simple(
            &self.conn,
            "SELECT id, case_id, name, role, status FROM team_members WHERE case_id = ?1 ORDER BY rowid",
            case_id,
            |r| Ok(TeamMember { id: r.get(0)?, case_id: r.get(1)?, name: r.get(2)?, role: r.get(3)?, status: r.get(4)? }),
        )
    }
}

fn map_case(row: &rusqlite::Row<'_>) -> rusqlite::Result<Case> {
    Ok(Case {
        id: row.get(0)?,
        domain: row.get(1)?,
        objective: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        play_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn query_simple<T, F>(conn: &Connection, sql: &str, case_id: &str, map: F) -> Result<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([case_id], map)?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str) -> CreateCase {
        CreateCase {
            id: Some(id.into()),
            domain: "land".into(),
            objective: "person_search".into(),
            priority: "high".into(),
            play_id: Some(" land_person ".into()),
            subject: SubjectInput {
                name: "Alex".into(),
                age: Some(42),
                clothing_description: None,
                medical_conditions: vec!["diabetes".into()],
            },
            contacts: vec![ContactInput {
                name: "Sam".into(),
                phone: Some("555-0100".into()),
                relationship: Some("partner".into()),
            }],
        }
    }

    #[test]
    fn migrates_and_persists_complete_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("command.sqlite");
        {
            let mut db = Database::open(&path).unwrap();
            db.create_case(input("C-1")).unwrap();
            db.add_clue(
                "C-1",
                "boot print",
                "searcher-2",
                "2026-10-30T12:00:00Z",
                Some((42.1, -83.2)),
            )
            .unwrap();
            db.add_update("C-1", "Team deployed", "operations", "2026-10-30T12:01:00Z")
                .unwrap();
            db.add_assignment(
                "C-1",
                "Sector A",
                None,
                "active",
                Some(serde_json::json!({"type":"Polygon","coordinates":[]})),
            )
            .unwrap();
            db.add_team_member("C-1", "Lee", "searcher", "deployed")
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let exported: CaseExport =
            serde_json::from_str(&db.export_case_json("C-1").unwrap()).unwrap();
        assert_eq!(exported.case.id, "C-1");
        assert_eq!(exported.subjects[0].medical_conditions, ["diabetes"]);
        assert_eq!(exported.contacts.len(), 1);
        assert_eq!(exported.assignments.len(), 1);
        assert_eq!(exported.clues.len(), 1);
        assert_eq!(exported.updates.len(), 1);
        assert_eq!(exported.team_members.len(), 1);
    }

    #[test]
    fn create_list_get_and_duplicate_are_safe() {
        let mut db = Database::open_in_memory().unwrap();
        db.create_case(input("C-1")).unwrap();
        db.create_case(input("C-2")).unwrap();
        assert_eq!(db.list_cases().unwrap().len(), 2);
        assert_eq!(
            db.get_case("C-1").unwrap().play_id.as_deref(),
            Some("land_person")
        );
        assert!(matches!(db.create_case(input("C-1")), Err(DbError::Sql(_))));
        assert!(matches!(db.get_case("missing"), Err(DbError::NotFound(_))));
    }

    #[test]
    fn invalid_input_never_writes_partial_records() {
        let mut db = Database::open_in_memory().unwrap();
        let mut bad = input("bad");
        bad.subject.name = " ".into();
        assert!(matches!(db.create_case(bad), Err(DbError::Validation(_))));
        assert!(db.list_cases().unwrap().is_empty());

        db.create_case(input("C")).unwrap();
        for position in [Some((91.0, 0.0)), Some((0.0, f64::NAN))] {
            assert!(matches!(
                db.add_clue("C", "x", "y", "2026-10-30T12:00:00Z", position),
                Err(DbError::Validation(_))
            ));
        }
        assert!(matches!(
            db.add_update("C", "x", "ic", "not-a-time"),
            Err(DbError::Validation(_))
        ));
        assert!(db.export_case("C").unwrap().clues.is_empty());
        assert!(db.export_case("C").unwrap().updates.is_empty());
    }

    #[test]
    fn foreign_keys_and_missing_cases_are_enforced() {
        let mut db = Database::open_in_memory().unwrap();
        assert!(matches!(
            db.add_update("absent", "x", "ic", "2026-10-30T12:00:00Z"),
            Err(DbError::NotFound(_))
        ));
        assert!(matches!(
            db.export_case_json("absent"),
            Err(DbError::NotFound(_))
        ));
    }
}
