use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod image_input;

#[cfg(windows)]
pub mod tasks;
#[cfg(windows)]
pub mod public_document;
#[cfg(windows)]
pub mod native_journal;
mod live_dispatch;
#[cfg(windows)]
pub mod native_checks;

#[cfg(windows)]
pub mod process;

#[cfg(windows)]
pub mod freecad;

#[cfg(windows)]
pub mod arduino;

#[cfg(windows)]
pub mod cadcraft;

#[cfg(windows)]
pub mod cadcraft_live;

#[cfg(windows)]
pub mod cadpilot;

#[cfg(windows)]
pub mod cadpilot_live;

#[cfg(windows)]
pub mod kicad;

#[cfg(windows)]
pub mod kicad_live;

#[cfg(windows)]
pub mod konnect;

#[cfg(windows)]
pub mod host_tools;

#[cfg(windows)]
pub mod native_tools;

#[cfg(windows)]
pub mod native_setup;

#[cfg(windows)]
pub mod serial_diagnostics;

#[cfg(windows)]
pub mod diagnostic_downloads;

#[cfg(windows)]
mod diagnostic_cli;

#[cfg(windows)]
mod program_tools;

#[cfg(windows)]
pub mod tool_bridge;

#[cfg(windows)]
pub mod claude;

#[cfg(windows)]
pub mod codex;

#[cfg(windows)]
pub mod client_discovery;

#[cfg(windows)]
pub mod konnect_run;

#[cfg(windows)]
pub mod konnect_pcb;

#[cfg(windows)]
pub mod kicad_project;

#[cfg(windows)]
pub mod kicad_run;

#[cfg(windows)]
pub mod orca;

#[cfg(windows)]
pub mod orca_run;

#[cfg(windows)]
pub mod agent;

#[cfg(windows)]
pub mod agent_editor;
mod agent_companion;

#[cfg(windows)]
pub mod project;

#[cfg(windows)]
pub mod jobs;

pub mod ollama;

pub const MIN_DIMENSION_MM: f64 = 1.0;
pub const MAX_DIMENSION_MM: f64 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dimensions {
    pub length: f64,
    pub width: f64,
    pub height: f64,
}

impl Dimensions {
    pub fn validate(self) -> Result<Self, String> {
        for number in [self.length, self.width, self.height] {
            if !number.is_finite() || !(MIN_DIMENSION_MM..=MAX_DIMENSION_MM).contains(&number) {
                return Err("Las dimensiones deben estar entre 1 y 200 mm.".into());
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SolidInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub dimensions_mm: Dimensions,
}

impl SolidInput {
    pub fn new(dimensions: Dimensions) -> Result<Self, String> {
        Ok(Self {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            dimensions_mm: dimensions.validate()?,
        })
    }

    pub fn validate(self) -> Result<Self, String> {
        if self.schema_version != 1 {
            return Err("Versión de operación no compatible.".into());
        }
        self.dimensions_mm.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dimensions_cannot_become_an_operation() {
        for length in [0.0, 200.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(SolidInput::new(Dimensions {
                length,
                width: 20.0,
                height: 30.0
            })
            .is_err());
        }
        for length in [1.0, 12.5, 200.0] {
            assert!(SolidInput::new(Dimensions {
                length,
                width: 20.0,
                height: 30.0
            })
            .is_ok());
        }
    }

    #[test]
    fn provider_objects_cannot_smuggle_fields_or_coerce_values() {
        for input in [
            r#"{"length":10,"width":20,"height":30,"shell":"run"}"#,
            r#"{"length":"10","width":20,"height":30}"#,
            r#"{"length":true,"width":20,"height":30}"#,
            r#"{"length":10,"length":15,"width":20,"height":30}"#,
            r#"{"length":10,"width":20}"#,
        ] {
            assert!(
                serde_json::from_str::<Dimensions>(input).is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn generated_request_matches_fixed_adapter_contract() {
        let request = SolidInput::new(Dimensions {
            length: 12.5,
            width: 7.25,
            height: 3.5,
        })
        .unwrap();
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["job_id"], request.job_id.to_string());
        assert_eq!(
            value["dimensions_mm"],
            serde_json::json!({"length":12.5,"width":7.25,"height":3.5})
        );
        let mut decoded: SolidInput = serde_json::from_value(value).unwrap();
        decoded.schema_version = 2;
        assert!(decoded.validate().is_err());
    }
}
