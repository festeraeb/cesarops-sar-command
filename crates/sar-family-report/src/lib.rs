//! The gate between the dream lab and anything shown to a family
//! (TRUTH_CONTRACT section 0): only MEASURED results, raw observed data and
//! stated uncertainty pass. A bad statement fails the whole build; it is never
//! dropped or repaired.
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Measured,
    Observed,
    Uncertainty,
    Hypothesis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub text: String,
    pub kind: Kind,
    pub ledger_id: Option<String>,
    pub command: Option<String>,
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub case_id: String,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedReport {
    pub case_id: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateError {
    BlankCaseId,
    NoStatements,
    Statement { index: usize, reason: String },
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankCaseId => write!(f, "case_id is blank"),
            Self::NoStatements => write!(f, "report has no statements"),
            Self::Statement { index, reason } => write!(f, "statement {index}: {reason}"),
        }
    }
}

impl std::error::Error for GateError {}

/// The field's text, only if present and not blank.
fn filled(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.trim().is_empty())
}

/// Check one statement against the gate rules; Err carries the reason.
fn check(s: &Statement) -> Result<(), &'static str> {
    if s.text.trim().is_empty() {
        return Err("text is blank");
    }
    match s.kind {
        Kind::Hypothesis => Err("hypothesis cannot appear in a family report"),
        Kind::Measured => {
            if filled(&s.ledger_id).is_none() {
                Err("measured statement lacks ledger_id")
            } else if filled(&s.command).is_none() {
                Err("measured statement lacks command")
            } else if filled(&s.uncertainty).is_none() {
                Err("measured statement lacks uncertainty")
            } else {
                Ok(())
            }
        }
        Kind::Observed => match filled(&s.ledger_id) {
            Some(_) => Ok(()),
            None => Err("observed statement lacks ledger_id"),
        },
        Kind::Uncertainty => Ok(()),
    }
}

pub fn build_family_report(
    case_id: &str,
    statements: Vec<Statement>,
) -> Result<RenderedReport, GateError> {
    if case_id.trim().is_empty() {
        return Err(GateError::BlankCaseId);
    }
    if statements.is_empty() {
        return Err(GateError::NoStatements);
    }
    for (index, s) in statements.iter().enumerate() {
        check(s).map_err(|r| GateError::Statement { index, reason: r.to_string() })?;
    }
    let mut md = format!("# Case report: {case_id}\n\n");
    let mut unknowns = String::new();
    for s in &statements {
        let ledger = filled(&s.ledger_id);
        match s.kind {
            Kind::Measured => md.push_str(&format!(
                "- {} (uncertainty: {}; ledger {})\n",
                s.text,
                filled(&s.uncertainty).unwrap_or_default(),
                ledger.unwrap_or_default()
            )),
            Kind::Observed => md.push_str(&format!(
                "- {} (observed; ledger {})\n",
                s.text,
                ledger.unwrap_or_default()
            )),
            _ => match ledger {
                Some(l) => unknowns.push_str(&format!("- {} (ledger {l})\n", s.text)),
                None => unknowns.push_str(&format!("- {}\n", s.text)),
            },
        }
    }
    if !unknowns.is_empty() {
        md.push_str("\n## What we do not know\n\n");
        md.push_str(&unknowns);
    }
    Ok(RenderedReport { case_id: case_id.to_string(), markdown: md })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(text: &str, id: &str) -> Statement {
        Statement {
            text: text.into(),
            kind: Kind::Measured,
            ledger_id: Some(id.into()),
            command: Some("sonar-scan --tile 7".into()),
            uncertainty: Some("position within 12 m".into()),
        }
    }

    fn observed(text: &str, id: &str) -> Statement {
        Statement {
            text: text.into(),
            kind: Kind::Observed,
            ledger_id: Some(id.into()),
            command: None,
            uncertainty: None,
        }
    }

    fn unknown(text: &str) -> Statement {
        Statement { text: text.into(), kind: Kind::Uncertainty, ledger_id: None, command: None, uncertainty: None }
    }

    fn err_index(r: Result<RenderedReport, GateError>) -> usize {
        match r {
            Err(GateError::Statement { index, .. }) => index,
            other => panic!("expected statement error, got {other:?}"),
        }
    }

    #[test]
    fn valid_mixed_report_renders_with_ledger_ids() {
        let r = build_family_report(
            "C-9",
            vec![measured("Debris field found", "L-1"), observed("Boat last seen at dusk", "L-2"), unknown("Whether the subject was on board")],
        )
        .unwrap();
        assert_eq!(r.case_id, "C-9");
        assert!(r.markdown.starts_with("# Case report: C-9"));
        assert!(r.markdown.contains("- Debris field found (uncertainty: position within 12 m; ledger L-1)"));
        assert!(r.markdown.contains("- Boat last seen at dusk (observed; ledger L-2)"));
        assert!(r.markdown.contains("## What we do not know"));
        let section = r.markdown.split("## What we do not know").nth(1).unwrap();
        assert!(section.contains("Whether the subject was on board"));
        assert!(!r.markdown.split("## What we do not know").next().unwrap().contains("Whether the subject"));
    }

    #[test]
    fn hypothesis_fails_whole_build() {
        let h = Statement { text: "Thermocline hints at a wreck".into(), kind: Kind::Hypothesis, ledger_id: Some("L-3".into()), command: Some("x".into()), uncertainty: Some("y".into()) };
        let r = build_family_report("C-9", vec![measured("a", "L-1"), h]);
        assert_eq!(err_index(r), 1);
    }

    #[test]
    fn measured_missing_ledger_id() {
        let mut s = measured("a", "L-1");
        s.ledger_id = None;
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn measured_blank_ledger_id() {
        let mut s = measured("a", "L-1");
        s.ledger_id = Some("  ".into());
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn measured_missing_command() {
        let mut s = measured("a", "L-1");
        s.command = None;
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn measured_blank_command() {
        let mut s = measured("a", "L-1");
        s.command = Some("".into());
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn measured_missing_uncertainty() {
        let mut s = measured("a", "L-1");
        s.uncertainty = None;
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn measured_blank_uncertainty() {
        let mut s = measured("a", "L-1");
        s.uncertainty = Some(" \t".into());
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn observed_missing_ledger_id() {
        let mut s = observed("a", "L-1");
        s.ledger_id = None;
        assert_eq!(err_index(build_family_report("C", vec![s])), 0);
    }

    #[test]
    fn blank_text_fails() {
        for kind_stmt in [measured(" ", "L-1"), observed("", "L-2"), unknown("  ")] {
            assert_eq!(err_index(build_family_report("C", vec![kind_stmt])), 0);
        }
    }

    #[test]
    fn empty_statement_list_is_err() {
        assert_eq!(build_family_report("C", vec![]), Err(GateError::NoStatements));
    }

    #[test]
    fn blank_case_id_is_err() {
        assert_eq!(build_family_report("  ", vec![observed("a", "L-1")]), Err(GateError::BlankCaseId));
    }

    #[test]
    fn ordering_preserved() {
        let r = build_family_report("C", vec![observed("first", "L-1"), measured("second", "L-2"), observed("third", "L-3")]).unwrap();
        let pos = |w: &str| r.markdown.find(w).unwrap();
        assert!(pos("first") < pos("second") && pos("second") < pos("third"));
    }

    #[test]
    fn error_message_names_offending_index() {
        let mut bad = measured("c", "L-3");
        bad.command = None;
        let e = build_family_report("C", vec![observed("a", "L-1"), measured("b", "L-2"), bad]).unwrap_err();
        assert!(e.to_string().contains("statement 2"), "{e}");
        assert!(e.to_string().contains("command"), "{e}");
    }
}
