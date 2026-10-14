//! Native PCB observations; this connection never grants mutation or saved-file fallback.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub endpoint: String,
    pub token: String,
}
impl Connection {
    pub fn validate(&self) -> Result<(), String> {
        let path = self
            .endpoint
            .strip_prefix("ipc://")
            .ok_or("Copia la dirección IPC local de las preferencias de KiCad.")?;
        // NNG maps this local Windows path into its pipe namespace. Never probe it with a raw pipe client.
        crate::project::valid_location(std::path::Path::new(path))
            .map_err(|_| "La dirección IPC debe ser una ruta local absoluta de Windows.")?;
        if path.len() > 1024
            || path.chars().any(char::is_control)
            || self.token.len() > 4096
            || self.token.chars().any(char::is_control)
        {
            return Err("Conexión IPC local no válida.".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub layer: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Observation {
    pub components: Vec<Component>,
    pub components_sha256: String,
}
impl Observation {
    pub fn from_native(info: &serde_json::Value, value: serde_json::Value) -> Result<Self, String> {
        if info["source"] != "ipc" {
            return Err("KiCad no confirmó la PCB abierta por IPC. No se sustituirá por el archivo guardado.".into());
        }
        let count = value["count"]
            .as_u64()
            .filter(|n| *n <= 256)
            .ok_or("La lectura nativa admite hasta 256 componentes completos.")?;
        let mut components: Vec<Component> = serde_json::from_value(value["components"].clone())
            .map_err(|_| "La lectura nativa de componentes no es compatible.")?;
        let mut references = BTreeSet::new();
        if components.len() as u64 != count
            || components.iter().any(|c| {
                c.reference.is_empty()
                    || !references.insert(c.reference.clone())
                    || [&c.reference, &c.value, &c.footprint, &c.layer]
                        .iter()
                        .any(|s| s.len() > 1024 || s.chars().any(char::is_control))
                    || [c.x, c.y, c.rotation]
                        .iter()
                        .any(|n| !n.is_finite() || n.abs() > 100000.)
            })
        {
            return Err(
                "La lectura nativa está incompleta o contiene identidades ambiguas.".into(),
            );
        }
        components.sort_by(|a, b| a.reference.cmp(&b.reference));
        let bytes = serde_json::to_vec(&components).map_err(|e| e.to_string())?;
        if bytes.len() > 60000 {
            return Err("La lectura nativa supera el límite del chat.".into());
        }
        Ok(Self {
            components,
            components_sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_local_nng_addresses_and_private_tokens_are_accepted() {
        let good = Connection {
            endpoint: "ipc://C:\\owned\\kicad\\api.sock".into(),
            token: "fixture-private-token".into(),
        };
        good.validate().unwrap();
        for endpoint in [
            "tcp://127.0.0.1:1234",
            "ipc://relative",
            "ipc://\\\\server\\share\\api.sock",
            "ipc://C:\\owned\\api.sock\n",
        ] {
            assert!(Connection {
                endpoint: endpoint.into(),
                ..good.clone()
            }
            .validate()
            .is_err());
        }
        assert!(serde_json::from_value::<Connection>(
            json!({"endpoint":good.endpoint,"token":good.token,"tool":"move_component"})
        )
        .is_err());
    }
    #[test]
    fn observations_require_ipc_and_a_complete_unambiguous_native_projection() {
        let c = json!({"reference":"R1","value":"10k","footprint":"R_0603","x":25.4,"y":10.,"rotation":90.,"layer":"F.Cu"});
        let v = json!({"count":1,"components":[c]});
        let observed = Observation::from_native(&json!({"source":"ipc"}), v.clone()).unwrap();
        assert_eq!(observed.components[0].x, 25.4);
        assert!(!serde_json::to_string(&observed).unwrap().contains("token"));
        for source in ["file", "saved_board", ""] {
            assert!(Observation::from_native(&json!({"source":source}), v.clone()).is_err());
        }
        for count in [0, 2, 257] {
            let mut invalid = v.clone();
            invalid["count"] = json!(count);
            assert!(Observation::from_native(&json!({"source":"ipc"}), invalid).is_err());
        }
        let duplicate = json!({"count":2,"components":[c,c]});
        assert!(Observation::from_native(&json!({"source":"ipc"}), duplicate).is_err());
        let mut changed = v.clone();
        changed["components"][0]["x"] = json!(26.4);
        assert_ne!(
            observed.components_sha256,
            Observation::from_native(&json!({"source":"ipc"}), changed)
                .unwrap()
                .components_sha256
        );
        let mut large = v;
        large["components"][0]["x"] = json!(1e100);
        assert!(Observation::from_native(&json!({"source":"ipc"}), large).is_err());
    }
}
