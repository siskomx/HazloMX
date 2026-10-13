//! Identified KiCad report contract; completed checks and design findings are separate.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const VERSION: &str = "10.0.7";
pub const REPORT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Erc,
    Drc,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
    In,
    Mils,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Error,
    Warning,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Filter {
    Error,
    Warning,
    Exclusion,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    x: f64,
    y: f64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    uuid: Uuid,
    description: String,
    pos: Position,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Violation {
    #[serde(rename = "type")]
    kind: String,
    description: String,
    severity: Severity,
    items: Vec<Item>,
    #[serde(default)]
    excluded: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IgnoredCheck {
    pub key: String,
    pub description: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sheet {
    path: String,
    uuid_path: String,
    violations: Vec<Violation>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErcReport {
    #[serde(rename = "$schema")]
    schema: String,
    source: String,
    date: String,
    kicad_version: String,
    coordinate_units: Units,
    included_severities: Vec<Filter>,
    ignored_checks: Vec<IgnoredCheck>,
    sheets: Vec<Sheet>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DrcReport {
    #[serde(rename = "$schema")]
    schema: String,
    source: String,
    date: String,
    kicad_version: String,
    coordinate_units: Units,
    included_severities: Vec<Filter>,
    ignored_checks: Vec<IgnoredCheck>,
    violations: Vec<Violation>,
    unconnected_items: Vec<Violation>,
    schematic_parity: Vec<Violation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckSummary {
    pub check: Check,
    pub units: Units,
    pub errors: u32,
    pub warnings: u32,
    pub excluded: u32,
    pub ignored_checks: Vec<String>,
}
struct Header {
    schema: String,
    source: String,
    date: String,
    version: String,
    units: Units,
    filters: Vec<Filter>,
    ignored: Vec<IgnoredCheck>,
}
fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.contains('\0')
}

/// The caller must use --severity-all and --exit-code-violations on an approved private copy.
pub fn validate_report(
    bytes: &[u8],
    check: Check,
    units: Units,
    source_name: &str,
    exit_code: u32,
) -> Result<CheckSummary, String> {
    let invalid = || "El informe de KiCad no corresponde a la revisión solicitada.".to_string();
    if bytes.is_empty()
        || bytes.len() > REPORT_LIMIT
        || !matches!(exit_code, 0 | 5)
        || !text(source_name, 255)
        || source_name.contains(['/', '\\', ':', '\0'])
    {
        return Err(invalid());
    }
    let (header, violations) = match check {
        Check::Erc => {
            let report: ErcReport = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            if report.sheets.is_empty() || report.sheets.len() > 1024 {
                return Err(invalid());
            }
            let mut violations = Vec::new();
            for sheet in report.sheets {
                if !text(&sheet.path, 4096)
                    || !text(&sheet.uuid_path, 4096)
                    || !sheet.path.starts_with('/')
                    || !sheet.uuid_path.starts_with('/')
                {
                    return Err(invalid());
                }
                if sheet.uuid_path[1..]
                    .split('/')
                    .any(|id| Uuid::parse_str(id).is_err())
                {
                    return Err(invalid());
                }
                violations.extend(sheet.violations);
            }
            (
                Header {
                    schema: report.schema,
                    source: report.source,
                    date: report.date,
                    version: report.kicad_version,
                    units: report.coordinate_units,
                    filters: report.included_severities,
                    ignored: report.ignored_checks,
                },
                violations,
            )
        }
        Check::Drc => {
            let mut report: DrcReport = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            report.violations.extend(report.unconnected_items);
            report.violations.extend(report.schematic_parity);
            (
                Header {
                    schema: report.schema,
                    source: report.source,
                    date: report.date,
                    version: report.kicad_version,
                    units: report.coordinate_units,
                    filters: report.included_severities,
                    ignored: report.ignored_checks,
                },
                report.violations,
            )
        }
    };
    let expected_schema = match check {
        Check::Erc => "https://schemas.kicad.org/erc.v1.json",
        Check::Drc => "https://schemas.kicad.org/drc.v1.json",
    };
    if header.schema != expected_schema
        || header.version != VERSION
        || header.source != source_name
        || header.units != units
        || !text(&header.date, 64)
        || header.filters.len() != 3
        || ![Filter::Error, Filter::Warning, Filter::Exclusion]
            .iter()
            .all(|value| header.filters.contains(value))
        || header.ignored.len() > 256
        || violations.len() > 100_000
    {
        return Err(invalid());
    }
    let mut summary = CheckSummary {
        check,
        units,
        errors: 0,
        warnings: 0,
        excluded: 0,
        ignored_checks: Vec::new(),
    };
    for ignored in header.ignored {
        if !text(&ignored.key, 256)
            || !text(&ignored.description, 8192)
            || summary.ignored_checks.contains(&ignored.key)
        {
            return Err(invalid());
        }
        summary.ignored_checks.push(ignored.key);
    }
    for violation in violations {
        if !text(&violation.kind, 256)
            || !text(&violation.description, 8192)
            || violation.items.len() > 1024
        {
            return Err(invalid());
        }
        for item in violation.items {
            if !text(&item.description, 8192) || !item.pos.x.is_finite() || !item.pos.y.is_finite()
            {
                return Err(invalid());
            }
            // Nil IDs can describe a design-wide finding; identity remains in the preserved raw report.
            let _ = item.uuid;
        }
        if violation.excluded {
            summary.excluded += 1;
        } else {
            match violation.severity {
                Severity::Error => summary.errors += 1,
                Severity::Warning => summary.warnings += 1,
            }
        }
    }
    if (exit_code == 0 && summary.errors + summary.warnings != 0)
        || (exit_code == 5 && summary.errors + summary.warnings + summary.excluded == 0)
    {
        return Err(invalid());
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ERC: &[u8] =
        include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/erc-mm.json");
    const DRC: &[u8] =
        include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/drc-mm.json");
    #[test]
    fn actual_reports_separate_completed_checks_from_findings() {
        let erc = validate_report(ERC, Check::Erc, Units::Mm, "basic_test.kicad_sch", 5).unwrap();
        assert_eq!((erc.errors, erc.warnings, erc.excluded), (2, 1, 0));
        assert!(erc.ignored_checks.contains(&"footprint_link_issues".into()));
        let drc = validate_report(DRC, Check::Drc, Units::Mm, "basic_test.kicad_pcb", 5).unwrap();
        assert_eq!((drc.errors, drc.warnings, drc.excluded), (0, 5, 0));
        let short = validate_report(
            include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/shorted.json"),
            Check::Drc,
            Units::Mm,
            "shorted.kicad_pcb",
            5,
        )
        .unwrap();
        assert!(short.errors > 0);
        for (bytes, kind, name) in [
            (
                include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/clean-erc.json")
                    .as_slice(),
                Check::Erc,
                "clean.kicad_sch",
            ),
            (
                include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/clean-drc.json")
                    .as_slice(),
                Check::Drc,
                "clean.kicad_pcb",
            ),
        ] {
            let result = validate_report(bytes, kind, Units::Mm, name, 0).unwrap();
            assert_eq!((result.errors, result.warnings, result.excluded), (0, 0, 0));
        }
        assert!(validate_report(
            include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/erc-in.json"),
            Check::Erc,
            Units::In,
            "basic_test.kicad_sch",
            5
        )
        .is_ok());
        assert!(validate_report(
            include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/drc-in.json"),
            Check::Drc,
            Units::In,
            "basic_test.kicad_pcb",
            5
        )
        .is_ok());
        for (bytes, check, name) in [
            (
                include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/erc-mils.json")
                    .as_slice(),
                Check::Erc,
                "basic_test.kicad_sch",
            ),
            (
                include_bytes!("../../../../docs/research/kicad-runtime-2026-10-11/drc-mils.json")
                    .as_slice(),
                Check::Drc,
                "basic_test.kicad_pcb",
            ),
        ] {
            assert!(validate_report(bytes, check, Units::Mils, name, 5).is_ok());
        }
    }
    #[test]
    fn rejects_mismatched_incomplete_and_contradictory_reports() {
        assert!(validate_report(ERC, Check::Drc, Units::Mm, "basic_test.kicad_sch", 5).is_err());
        assert!(validate_report(ERC, Check::Erc, Units::In, "basic_test.kicad_sch", 5).is_err());
        assert!(validate_report(ERC, Check::Erc, Units::Mm, "other.kicad_sch", 5).is_err());
        for exit in [0, 1, 3, 7] {
            assert!(
                validate_report(ERC, Check::Erc, Units::Mm, "basic_test.kicad_sch", exit).is_err()
            );
        }
        let base: serde_json::Value = serde_json::from_slice(ERC).unwrap();
        for (key, value) in [
            ("kicad_version", serde_json::json!("10.0.8")),
            ("included_severities", serde_json::json!(["error"])),
            ("sheets", serde_json::json!([])),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut report = base.clone();
            report[key] = value;
            assert!(validate_report(
                &serde_json::to_vec(&report).unwrap(),
                Check::Erc,
                Units::Mm,
                "basic_test.kicad_sch",
                5
            )
            .is_err());
        }
        let mut report = base;
        report["sheets"][0]["violations"][0]["items"][0]["uuid"] = serde_json::json!("not-a-uuid");
        assert!(validate_report(
            &serde_json::to_vec(&report).unwrap(),
            Check::Erc,
            Units::Mm,
            "basic_test.kicad_sch",
            5
        )
        .is_err());
    }
    #[test]
    fn excluded_findings_are_not_counted_as_active_errors() {
        let mut report: serde_json::Value = serde_json::from_slice(ERC).unwrap();
        for sheet in report["sheets"].as_array_mut().unwrap() {
            for violation in sheet["violations"].as_array_mut().unwrap() {
                violation["excluded"] = serde_json::json!(true);
            }
        }
        let summary = validate_report(
            &serde_json::to_vec(&report).unwrap(),
            Check::Erc,
            Units::Mm,
            "basic_test.kicad_sch",
            5,
        )
        .unwrap();
        assert_eq!(
            (summary.errors, summary.warnings, summary.excluded),
            (0, 0, 3)
        );
        // Synthetic classification fixture; actual excluded-marker command behavior still needs a runtime check.
    }
}
