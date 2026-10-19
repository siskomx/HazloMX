//! Approved observable facts, independent of whether a native call acknowledged success.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Identity,
    Outcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub role: Role,
    pub label: String,
    pub pointer: String,
    pub expected: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
}
pub fn validate(checks: &[Check], steps: usize) -> Result<(), String> {
    if checks.is_empty() {
        return Ok(());
    }
    if checks.len() > 64
        || !checks.iter().any(|c| {
            c.role == Role::Identity
                && c.expected
                    .as_str()
                    .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|id| !id.is_nil()))
        })
        || (0..steps).any(|i| {
            !checks
                .iter()
                .any(|c| c.role == Role::Outcome && c.step == Some(i))
        })
    {
        return Err("Incluye un UUID nativo estable de destino y una comprobación por paso; un hash de contenido solo no identifica el documento.".into());
    }
    let mut final_values = std::collections::BTreeMap::new();
    for c in checks {
        let valid_pointer = c.pointer.starts_with('/')
            && c.pointer.len() <= 256
            && !c.pointer.chars().any(char::is_control)
            && !c
                .pointer
                .split('/')
                .skip(1)
                .any(|part| part.replace("~1", "").replace("~0", "").contains('~'));
        if c.label.trim().is_empty()
            || c.label.len() > 160
            || c.label.chars().any(char::is_control)
            || !valid_pointer
            || c.expected.is_array()
            || c.expected.is_object()
            || serde_json::to_vec(&c.expected)
                .map_err(|_| "Comprobación no válida.")?
                .len()
                > 512
        {
            return Err("Cada comprobación necesita una descripción, un puntero JSON y un valor escalar acotado.".into());
        }
        match c.role {
            Role::Identity => {
                let value = c
                    .expected
                    .as_str()
                    .ok_or("La identidad debe ser un UUID nativo o un SHA-256.")?;
                if c.step.is_some()
                    || !(uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
                        || crate::kicad_project::valid_hash(value))
                {
                    return Err(
                        "La identidad debe ser un UUID nativo o un SHA-256, no un nombre ambiguo."
                            .into(),
                    );
                }
            }
            Role::Outcome => {
                if final_values
                    .insert(c.pointer.as_str(), &c.expected)
                    .is_some_and(|expected| expected != &c.expected)
                {
                    return Err("Las comprobaciones finales del mismo campo se contradicen. Usa un resultado final coherente o etapas nativas verificables distintas.".into());
                }
                if c.step.is_none_or(|i| i >= steps) {
                    return Err("Vincula el resultado a un paso del lote.".into());
                }
            }
        }
    }
    Ok(())
}
pub fn observed(value: &Value) -> Value {
    if let Some(v) = value.get("structuredContent") {
        return v.clone();
    }
    if let Some(content) = value
        .get("content")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
    {
        if content[0]["type"] == "text" {
            if let Some(v) = content[0]["text"]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
            {
                return v;
            }
        }
    }
    value.clone()
}
pub fn matches(check: &Check, observed: &Value) -> bool {
    observed.pointer(&check.pointer) == Some(&check.expected)
}
pub fn identities(checks: &[Check], value: &Value) -> bool {
    let value = observed(value);
    checks
        .iter()
        .filter(|c| c.role == Role::Identity)
        .all(|c| matches(c, &value))
}
pub fn schema() -> Value {
    json!({"type":"array","maxItems":64,"items":{"type":"object","properties":{"role":{"enum":["identity","outcome"]},"label":{"type":"string","minLength":1,"maxLength":160},"pointer":{"type":"string","minLength":1,"maxLength":256},"expected":{},"step":{"type":"integer","minimum":0,"maximum":15}},"required":["role","label","pointer","expected"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_bind_native_identity_and_step_results_without_exposing_values() {
        let id = uuid::Uuid::new_v4();
        let checks = vec![
            Check {
                role: Role::Identity,
                label: "Documento".into(),
                pointer: "/id".into(),
                expected: json!(id),
                step: None,
            },
            Check {
                role: Role::Outcome,
                label: "Ancho acordado en mm".into(),
                pointer: "/width_mm".into(),
                expected: json!(42),
                step: Some(0),
            },
        ];
        validate(&checks, 1).unwrap();
        let actual = observed(
            &json!({"content":[{"type":"text","text":json!({"id":id,"width_mm":42,"manual_label":"private"}).to_string()}]}),
        );
        assert!(identities(&checks, &actual));
        assert!(matches(&checks[1], &actual));
        assert!(!identities(
            &checks,
            &json!({"id":uuid::Uuid::new_v4(),"width_mm":42})
        ));
        assert!(!matches(&checks[1], &json!({"id":id})));
        assert!(!matches(&checks[1], &json!({"width_mm":40})));
        let mut bad = checks.clone();
        bad[0].expected = json!("Untitled");
        assert!(validate(&bad, 1).is_err());
        bad[0].expected = json!("a".repeat(64));
        assert!(validate(&bad, 1).is_err());
        bad = checks.clone();
        bad[1].pointer = "/bad~2".into();
        assert!(validate(&bad, 1).is_err());
        assert!(validate(&checks, 2).is_err());
        let mut contradictory = checks.clone();
        let mut later = checks[1].clone();
        later.step = Some(1);
        later.expected = json!(43);
        contradictory.push(later);
        assert!(validate(&contradictory, 2).is_err());
    }
}
