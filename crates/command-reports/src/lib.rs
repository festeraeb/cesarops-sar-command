//! Truth-gated stakeholder reports rendered as self-contained HTML or PDF.
use chrono::Utc;
use printpdf::{BuiltinFont, Mm, PdfDocument};
use serde::{Deserialize, Serialize};
use std::io::BufWriter;
use tera::{Context, Tera};
use thiserror::Error;

pub use sar_family_report::{Kind, Statement};

const HTML_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{{ title }}</title>
  <style>body{font:16px/1.5 system-ui,sans-serif;max-width:54rem;margin:2rem auto;padding:0 1rem;color:#17202a}h1{border-bottom:3px solid #d88a00;padding-bottom:.4rem}.meta{color:#52606d}.statement{margin:.8rem 0;padding:.7rem;border-left:4px solid #d88a00;background:#f7f8fa}.uncertainty{border-color:#64748b}.evidence{font-size:.85rem;color:#52606d}</style>
</head>
<body>
  <h1>{{ title }}</h1>
  <p class="meta">Case {{ case_id }} · Generated {{ generated_at }}</p>
  {% for item in statements %}
  <section class="statement{% if item.kind == "Uncertainty" %} uncertainty{% endif %}">
    <div>{{ item.text }}</div>
    <div class="evidence">{{ item.label }}{% if item.ledger_id %} · ledger {{ item.ledger_id }}{% endif %}{% if item.uncertainty %} · uncertainty: {{ item.uncertainty }}{% endif %}</div>
  </section>
  {% endfor %}
</body>
</html>
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Family,
    Team,
    IncidentCommander,
}

impl Audience {
    fn title(self) -> &'static str {
        match self {
            Self::Family => "Family Situation Report",
            Self::Team => "SAR Team Operational Report",
            Self::IncidentCommander => "Incident Commander Report",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportRequest {
    pub case_id: String,
    pub audience: Audience,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HtmlReport {
    pub case_id: String,
    pub audience: Audience,
    pub html: String,
}

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("truth gate rejected report: {0}")]
    TruthGate(String),
    #[error("template rendering failed: {0}")]
    Template(#[from] tera::Error),
    #[error("PDF generation failed: {0}")]
    Pdf(String),
}

#[derive(Serialize)]
struct ViewStatement<'a> {
    text: &'a str,
    kind: &'static str,
    label: &'static str,
    ledger_id: Option<&'a str>,
    uncertainty: Option<&'a str>,
}

fn gate(request: &ReportRequest) -> Result<(), ReportError> {
    sar_family_report::build_family_report(&request.case_id, request.statements.clone())
        .map(|_| ())
        .map_err(|e| ReportError::TruthGate(e.to_string()))
}

fn view(statement: &Statement) -> ViewStatement<'_> {
    let (kind, label) = match statement.kind {
        Kind::Measured => ("Measured", "Measured result"),
        Kind::Observed => ("Observed", "Direct observation"),
        Kind::Uncertainty => ("Uncertainty", "What is not yet known"),
        Kind::Hypothesis => ("Hypothesis", "Hypothesis"),
    };
    ViewStatement {
        text: statement.text.trim(),
        kind,
        label,
        ledger_id: statement.ledger_id.as_deref().map(str::trim),
        uncertainty: statement.uncertainty.as_deref().map(str::trim),
    }
}

pub fn render_html(request: &ReportRequest) -> Result<HtmlReport, ReportError> {
    gate(request)?;
    let mut context = Context::new();
    context.insert("title", request.audience.title());
    context.insert("case_id", request.case_id.trim());
    context.insert("generated_at", &Utc::now().to_rfc3339());
    context.insert(
        "statements",
        &request.statements.iter().map(view).collect::<Vec<_>>(),
    );
    let html = Tera::one_off(HTML_TEMPLATE, &context, true)?;
    Ok(HtmlReport {
        case_id: request.case_id.trim().to_owned(),
        audience: request.audience,
        html,
    })
}

pub fn export_html(request: &ReportRequest) -> Result<String, ReportError> {
    Ok(render_html(request)?.html)
}

/// Produce a real, parseable PDF document. The truth gate runs before any
/// bytes are emitted, so a rejected report cannot leave a partial export.
pub fn render_pdf(request: &ReportRequest) -> Result<Vec<u8>, ReportError> {
    gate(request)?;
    let title = request.audience.title();
    let (doc, page, layer) = PdfDocument::new(title, Mm(210.0), Mm(297.0), "Report");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| ReportError::Pdf(e.to_string()))?;
    let bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| ReportError::Pdf(e.to_string()))?;
    let mut current = doc.get_page(page).get_layer(layer);
    let mut y = 278.0_f32;
    current.use_text(title, 18.0, Mm(15.0), Mm(y), &bold);
    y -= 10.0;
    current.use_text(
        format!("Case {}", request.case_id.trim()),
        11.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 10.0;

    for statement in &request.statements {
        let prefix = match statement.kind {
            Kind::Measured => "MEASURED",
            Kind::Observed => "OBSERVED",
            Kind::Uncertainty => "UNCERTAINTY",
            Kind::Hypothesis => unreachable!("hypotheses are rejected by the truth gate"),
        };
        let mut text = format!("[{prefix}] {}", single_line(&statement.text));
        if let Some(ledger) = statement.ledger_id.as_deref() {
            text.push_str(&format!(" (ledger {})", single_line(ledger)));
        }
        if let Some(uncertainty) = statement.uncertainty.as_deref() {
            text.push_str(&format!("; uncertainty: {}", single_line(uncertainty)));
        }
        for line in wrap_text(&text, 88) {
            if y < 20.0 {
                let (page, layer) = doc.add_page(Mm(210.0), Mm(297.0), "Report");
                current = doc.get_page(page).get_layer(layer);
                y = 278.0;
                current.use_text(format!("{title} - continued"), 14.0, Mm(15.0), Mm(y), &bold);
                y -= 10.0;
            }
            current.use_text(line, 10.0, Mm(15.0), Mm(y), &font);
            y -= 5.5;
        }
        y -= 3.0;
    }

    let mut bytes = Vec::new();
    doc.save(&mut BufWriter::new(&mut bytes))
        .map_err(|e| ReportError::Pdf(e.to_string()))?;
    Ok(bytes)
}

pub fn export_pdf(request: &ReportRequest) -> Result<Vec<u8>, ReportError> {
    render_pdf(request)
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wrap_text(value: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        if !line.is_empty() && line.len() + word.len() + 1 > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> Statement {
        Statement {
            text: "Search team measured the position".into(),
            kind: Kind::Measured,
            ledger_id: Some("L-17".into()),
            command: Some("gps-read".into()),
            uncertainty: Some("within 8 m".into()),
        }
    }

    fn request(audience: Audience) -> ReportRequest {
        ReportRequest {
            case_id: "C-42".into(),
            audience,
            statements: vec![
                measured(),
                Statement {
                    text: "Subject was last seen near the trail".into(),
                    kind: Kind::Observed,
                    ledger_id: Some("L-18".into()),
                    command: None,
                    uncertainty: None,
                },
                Statement {
                    text: "Current direction of travel is unknown".into(),
                    kind: Kind::Uncertainty,
                    ledger_id: None,
                    command: None,
                    uncertainty: None,
                },
            ],
        }
    }

    #[test]
    fn renders_family_team_and_ic_html_with_escaping() {
        for audience in [
            Audience::Family,
            Audience::Team,
            Audience::IncidentCommander,
        ] {
            let mut request = request(audience);
            request.statements[1].text = "<script>alert(1)</script>".into();
            let report = render_html(&request).unwrap();
            assert!(report.html.starts_with("<!doctype html>"));
            assert!(report.html.contains(audience.title()));
            assert!(report.html.contains("&lt;script&gt;"));
            assert!(!report.html.contains("<script>"));
            assert!(report.html.contains("L-17"));
        }
    }

    #[test]
    fn emits_actual_pdf_bytes_for_each_report_kind() {
        for audience in [
            Audience::Family,
            Audience::Team,
            Audience::IncidentCommander,
        ] {
            let pdf = render_pdf(&request(audience)).unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            assert!(pdf.windows(5).any(|bytes| bytes == b"%%EOF"));
            assert!(pdf.len() > 500);
        }
    }

    #[test]
    fn truth_gate_blocks_hypotheses_and_incomplete_evidence() {
        let mut hypothesis = request(Audience::Family);
        hypothesis.statements.push(Statement {
            text: "Maybe the subject went north".into(),
            kind: Kind::Hypothesis,
            ledger_id: Some("L-19".into()),
            command: None,
            uncertainty: None,
        });
        assert!(matches!(
            render_html(&hypothesis),
            Err(ReportError::TruthGate(_))
        ));
        assert!(matches!(
            render_pdf(&hypothesis),
            Err(ReportError::TruthGate(_))
        ));

        let mut incomplete = request(Audience::Team);
        incomplete.statements[0].command = None;
        assert!(matches!(
            render_html(&incomplete),
            Err(ReportError::TruthGate(_))
        ));
    }

    #[test]
    fn blank_case_and_empty_report_are_rejected() {
        let mut blank = request(Audience::IncidentCommander);
        blank.case_id = " ".into();
        assert!(matches!(
            render_html(&blank),
            Err(ReportError::TruthGate(_))
        ));
        blank.case_id = "C".into();
        blank.statements.clear();
        assert!(matches!(render_pdf(&blank), Err(ReportError::TruthGate(_))));
    }
}
