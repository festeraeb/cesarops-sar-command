-- Attachments: things hung on a case that are not log lines.
--   kind 'training_marker'  : the case is fictitious training data
--   kind 'sample_run'       : a real sample sonar run (processed elsewhere), with source and licence credit
--   kind 'simulated'        : a made-up placeholder, always shown under a "FULLY SIMULATED" banner
--   kind 'family_report'    : prepared plain-language family message
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_attachments_case ON attachments(case_id);
