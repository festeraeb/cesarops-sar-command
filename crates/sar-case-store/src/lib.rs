//! Append-only case log (updates and clues) stored as JSON lines in one file.
//! A corrupt file is reported, never repaired or truncated (TRUTH_CONTRACT).
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntryKind {
    Update,
    Clue,
    Assignment,
    Note,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub time: String,
    pub role: String,
    pub kind: EntryKind,
    pub text: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    /// 1-based line number of the first bad line.
    Corrupt { line: usize },
    BlankRole,
    BlankText,
    BadTime(String),
    BadCoordinate,
    PositionIncomplete,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "case store I/O error: {e}"),
            Self::Corrupt { line } => write!(f, "case log corrupt at line {line}"),
            Self::BlankRole => write!(f, "role is blank"),
            Self::BlankText => write!(f, "text is blank"),
            Self::BadTime(t) => write!(f, "time {t:?} is not YYYY-MM-DDTHH:MM:SS[.fff](Z|+hh:mm|-hh:mm)"),
            Self::BadCoordinate => write!(f, "coordinate is non-finite or out of range"),
            Self::PositionIncomplete => write!(f, "lat and lon must both be present or both absent"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug)]
pub struct CaseStore {
    path: PathBuf,
    next_seq: u64,
}

fn digits(b: &[u8], at: usize, n: usize) -> Option<u32> {
    let part = b.get(at..at + n)?;
    if !part.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(part.iter().fold(0, |a, d| a * 10 + u32::from(d - b'0')))
}

fn time_ok(t: &str) -> bool {
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
        i += 1;
        let start = i;
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

fn in_range(v: f64, limit: f64) -> bool {
    v.is_finite() && v.abs() <= limit
}

/// Read and verify the whole log: every line parses, seq runs 1,2,3... and the
/// file ends with a newline. Nothing is modified.
fn read_log(path: &Path) -> Result<Vec<LogEntry>, StoreError> {
    let bytes = fs::read(path)?;
    let mut entries = Vec::new();
    if bytes.is_empty() {
        return Ok(entries);
    }
    let complete = bytes.ends_with(b"\n");
    let body = if complete { &bytes[..bytes.len() - 1] } else { &bytes[..] };
    let lines: Vec<&[u8]> = body.split(|&c| c == b'\n').collect();
    for (i, raw) in lines.iter().enumerate() {
        let line = i + 1;
        let entry: LogEntry =
            serde_json::from_slice(raw).map_err(|_| StoreError::Corrupt { line })?;
        if entry.seq != line as u64 || (!complete && line == lines.len()) {
            return Err(StoreError::Corrupt { line });
        }
        entries.push(entry);
    }
    Ok(entries)
}

impl CaseStore {
    pub fn open(path: &Path) -> Result<CaseStore, StoreError> {
        if !path.exists() {
            OpenOptions::new().write(true).create_new(true).open(path)?;
        }
        let entries = read_log(path)?;
        Ok(CaseStore { path: path.to_path_buf(), next_seq: entries.len() as u64 + 1 })
    }

    pub fn append(
        &mut self,
        time: &str,
        role: &str,
        kind: EntryKind,
        text: &str,
        lat: Option<f64>,
        lon: Option<f64>,
    ) -> Result<LogEntry, StoreError> {
        if role.trim().is_empty() {
            return Err(StoreError::BlankRole);
        }
        if text.trim().is_empty() {
            return Err(StoreError::BlankText);
        }
        if !time_ok(time) {
            return Err(StoreError::BadTime(time.to_string()));
        }
        match (lat, lon) {
            (Some(la), Some(lo)) if !(in_range(la, 90.0) && in_range(lo, 180.0)) => {
                return Err(StoreError::BadCoordinate)
            }
            (Some(_), None) | (None, Some(_)) => return Err(StoreError::PositionIncomplete),
            _ => {}
        }
        let entry = LogEntry {
            seq: self.next_seq,
            time: time.to_string(),
            role: role.to_string(),
            kind,
            text: text.to_string(),
            lat,
            lon,
        };
        let mut line = serde_json::to_string(&entry).map_err(|e| std::io::Error::other(e))?;
        line.push('\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        self.next_seq += 1;
        Ok(entry)
    }

    pub fn entries(&self) -> Result<Vec<LogEntry>, StoreError> {
        read_log(&self.path)
    }

    pub fn clues_with_position(&self) -> Result<Vec<LogEntry>, StoreError> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| e.kind == EntryKind::Clue && e.lat.is_some() && e.lon.is_some())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const T: &str = "2026-10-27T14:30:00Z";

    /// Unique per-test directory under the system temp dir; returns the log path.
    fn log_path() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sar-case-store-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("case.jsonl")
    }

    fn add(s: &mut CaseStore, text: &str) -> LogEntry {
        s.append(T, "ic", EntryKind::Update, text, None, None).unwrap()
    }

    #[test]
    fn append_then_reopen_keeps_order_and_seq() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        add(&mut s, "two");
        add(&mut s, "three");
        let e = CaseStore::open(&p).unwrap().entries().unwrap();
        let texts: Vec<_> = e.iter().map(|x| (x.seq, x.text.as_str())).collect();
        assert_eq!(texts, vec![(1, "one"), (2, "two"), (3, "three")]);
    }

    #[test]
    fn reopen_continues_numbering() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        let mut s2 = CaseStore::open(&p).unwrap();
        assert_eq!(add(&mut s2, "two").seq, 2);
    }

    #[test]
    fn open_creates_missing_file() {
        let p = log_path();
        assert!(!p.exists());
        let s = CaseStore::open(&p).unwrap();
        assert!(p.exists());
        assert!(s.entries().unwrap().is_empty());
    }

    #[test]
    fn corrupt_line_is_reported_and_file_untouched() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{not json\n").unwrap();
        drop(f);
        let before = fs::read(&p).unwrap();
        assert!(matches!(CaseStore::open(&p), Err(StoreError::Corrupt { line: 2 })));
        assert_eq!(fs::read(&p).unwrap(), before);
    }

    #[test]
    fn missing_final_newline_is_corrupt() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        let mut bytes = fs::read(&p).unwrap();
        bytes.pop();
        fs::write(&p, &bytes).unwrap();
        assert!(matches!(CaseStore::open(&p), Err(StoreError::Corrupt { line: 1 })));
        assert_eq!(fs::read(&p).unwrap(), bytes);
    }

    #[test]
    fn seq_gap_is_corrupt() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        add(&mut s, "two");
        add(&mut s, "three");
        let text = fs::read_to_string(&p).unwrap();
        let kept: Vec<&str> = text.lines().filter(|l| !l.contains("\"seq\":2")).collect();
        fs::write(&p, kept.join("\n") + "\n").unwrap();
        let before = fs::read(&p).unwrap();
        assert!(matches!(CaseStore::open(&p), Err(StoreError::Corrupt { line: 2 })));
        assert_eq!(fs::read(&p).unwrap(), before);
    }

    fn rejects(s: &mut CaseStore, p: &Path, time: &str, role: &str, text: &str, lat: Option<f64>, lon: Option<f64>) -> StoreError {
        let before = fs::read(p).unwrap();
        let e = s.append(time, role, EntryKind::Clue, text, lat, lon).unwrap_err();
        assert_eq!(fs::read(p).unwrap(), before, "failed append modified the file");
        e
    }

    #[test]
    fn blank_role_rejected() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        assert!(matches!(rejects(&mut s, &p, T, "  ", "x", None, None), StoreError::BlankRole));
    }

    #[test]
    fn blank_text_rejected() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        assert!(matches!(rejects(&mut s, &p, T, "ic", "", None, None), StoreError::BlankText));
    }

    #[test]
    fn bad_time_rejected() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        for t in ["", "now", "2026-10-27T14:30:00", "2026-10-27 14:30:00Z", "2026-02-30T00:00:00Z", "2026-10-27T25:00:00Z", "2026-10-27T14:30:00.Z", "2026-10-27T14:30:00+0100"] {
            assert!(matches!(rejects(&mut s, &p, t, "ic", "x", None, None), StoreError::BadTime(_)), "{t}");
        }
        for t in ["2026-10-27T14:30:00.123Z", "2026-10-27T14:30:00+05:30", "2026-10-27T14:30:00-04:00"] {
            s.append(t, "ic", EntryKind::Note, "ok", None, None).unwrap();
        }
    }

    #[test]
    fn bad_coordinates_rejected() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        for (la, lo) in [(91.0, 0.0), (0.0, 181.0), (-90.5, 0.0), (f64::NAN, 0.0), (0.0, f64::INFINITY)] {
            assert!(matches!(rejects(&mut s, &p, T, "ic", "x", Some(la), Some(lo)), StoreError::BadCoordinate), "{la} {lo}");
        }
    }

    #[test]
    fn half_position_rejected() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        assert!(matches!(rejects(&mut s, &p, T, "ic", "x", Some(1.0), None), StoreError::PositionIncomplete));
        assert!(matches!(rejects(&mut s, &p, T, "ic", "x", None, Some(1.0)), StoreError::PositionIncomplete));
    }

    #[test]
    fn failed_append_does_not_consume_seq() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        let _ = s.append("bad", "ic", EntryKind::Note, "x", None, None);
        assert_eq!(add(&mut s, "ok").seq, 1);
    }

    #[test]
    fn clues_with_position_filters() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        s.append(T, "ic", EntryKind::Clue, "no position", None, None).unwrap();
        s.append(T, "ic", EntryKind::Update, "update with position", Some(42.5), Some(-82.7)).unwrap();
        let c = s.append(T, "ic", EntryKind::Clue, "debris", Some(42.5), Some(-82.7)).unwrap();
        assert_eq!(s.clues_with_position().unwrap(), vec![c]);
    }

    #[test]
    fn existing_lines_never_rewritten() {
        let p = log_path();
        let mut s = CaseStore::open(&p).unwrap();
        add(&mut s, "one");
        let first = fs::read(&p).unwrap();
        add(&mut s, "two");
        assert!(fs::read(&p).unwrap().starts_with(&first));
    }
}
