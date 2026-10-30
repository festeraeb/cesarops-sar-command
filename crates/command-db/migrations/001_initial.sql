PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cases (
    id TEXT PRIMARY KEY,
    domain TEXT NOT NULL CHECK(domain IN ('water', 'land')),
    objective TEXT NOT NULL,
    priority TEXT NOT NULL CHECK(priority IN ('low', 'medium', 'high', 'critical')),
    status TEXT NOT NULL DEFAULT 'open',
    play_id TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS subjects (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    age INTEGER CHECK(age IS NULL OR (age >= 0 AND age <= 130)),
    clothing_description TEXT,
    medical_conditions TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS contacts (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    phone TEXT,
    relationship TEXT
);
CREATE TABLE IF NOT EXISTS assignments (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    team_id TEXT,
    status TEXT NOT NULL,
    geometry_json TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS clues (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    recorded_by TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    lat REAL,
    lon REAL,
    CHECK (
        (lat IS NULL AND lon IS NULL)
        OR
        (lat IS NOT NULL AND lon IS NOT NULL AND lat BETWEEN -90 AND 90 AND lon BETWEEN -180 AND 180)
    )
);
CREATE TABLE IF NOT EXISTS updates (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS team_members (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_subjects_case ON subjects(case_id);
CREATE INDEX IF NOT EXISTS idx_contacts_case ON contacts(case_id);
CREATE INDEX IF NOT EXISTS idx_assignments_case ON assignments(case_id);
CREATE INDEX IF NOT EXISTS idx_clues_case ON clues(case_id);
CREATE INDEX IF NOT EXISTS idx_updates_case ON updates(case_id);
CREATE INDEX IF NOT EXISTS idx_team_members_case ON team_members(case_id);
