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
    pub document_sha256: String,
}
pub(crate) fn document_identity(
    state: &serde_json::Value,
    board: &std::path::Path,
) -> Result<String, String> {
    use serde_json::json;
    let version = &state["kicad_version"];
    if state["evidence_source"] != "kicad_ipc"
        || version["major"] != 10
        || version["minor"] != 0
        || version["patch"] != 7
    {
        return Err("KiCad no confirmó la identidad IPC de la versión compatible.".into());
    }
    let editors = state["editors"]
        .as_array()
        .filter(|e| e.len() <= 2)
        .ok_or("Falta el contexto nativo del editor.")?;
    let pcb = editors
        .iter()
        .filter(|e| e["editor"] == "pcb")
        .collect::<Vec<_>>();
    if pcb.len() != 1 || pcb[0]["addressable"] != true {
        return Err("No hay un único editor PCB accesible en esta conexión.".into());
    }
    let documents = pcb[0]["documents"]
        .as_array()
        .filter(|d| d.len() <= 32)
        .ok_or("La lista nativa de documentos no es compatible.")?;
    let selected =
        std::fs::canonicalize(board).map_err(|_| "No se pudo identificar la PCB seleccionada.")?;
    let mut matches = Vec::new();
    for doc in documents {
        let path = doc["document_path"]
            .as_str()
            .ok_or("Falta la ruta nativa de una PCB abierta.")?;
        crate::project::valid_location(std::path::Path::new(path))
            .map_err(|_| "KiCad devolvió una identidad de documento no válida.")?;
        if doc["editor"] != "pcb" || !doc["sheet_instance_path"].is_null() {
            return Err("KiCad devolvió otro tipo de documento.".into());
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| "No se pudo comprobar una identidad de PCB abierta.")?;
        if canonical != selected {
            continue;
        }
        let project = if doc["project"].is_null() {
            serde_json::Value::Null
        } else {
            let name = doc["project"]["name"]
                .as_str()
                .ok_or("Falta el nombre del proyecto nativo.")?;
            let path = doc["project"]["path"]
                .as_str()
                .ok_or("Falta la identidad del proyecto nativo.")?;
            if name.len() > 1024 || name.chars().any(char::is_control) || path.len() > 4096 {
                return Err("La identidad del proyecto nativo no es válida.".into());
            }
            crate::project::valid_location(std::path::Path::new(path))
                .map_err(|_| "El proyecto nativo no tiene una ruta local válida.")?;
            json!({"name":name,"path":path})
        };
        matches.push(json!({"document_path":canonical,"project":project}));
    }
    if matches.len() != 1 {
        return Err(
            "La PCB seleccionada no tiene una identidad nativa única. Vuelve a conectar.".into(),
        );
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&matches[0]).map_err(|e| e.to_string())?)
    ))
}
impl Observation {
    pub fn from_native(
        info: &serde_json::Value,
        value: serde_json::Value,
        document_sha256: &str,
    ) -> Result<Self, String> {
        if !crate::kicad_project::valid_hash(document_sha256) {
            return Err("Falta la identidad nativa del documento.".into());
        }
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
            document_sha256: document_sha256.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn observe(info: &serde_json::Value, value: serde_json::Value) -> Result<Observation, String> {
        Observation::from_native(info, value, &"a".repeat(64))
    }
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
    fn document_context_requires_one_exact_native_board_and_hides_private_identity() {
        let root =
            std::env::temp_dir().join(format!("hazlo-native-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let board = root.join("selected.kicad_pcb");
        std::fs::write(&board, b"identity fixture").unwrap();
        let other = root.join("other.kicad_pcb");
        std::fs::write(&other, b"other fixture").unwrap();
        let doc = json!({"editor":"pcb","project":{"name":"owned native project","path":root},"document_path":board,"sheet_instance_path":null});
        let state = json!({"kicad_version":{"major":10,"minor":0,"patch":7,"full_version":"10.0.7"},"evidence_source":"kicad_ipc","editors":[{"editor":"schematic","addressable":false,"documents":[]},{"editor":"pcb","addressable":true,"documents":[doc]}],"active_editor":null,"active_document":null,"active_sheet_instance":null});
        let fingerprint = document_identity(&state, &board).unwrap();
        assert!(crate::kicad_project::valid_hash(&fingerprint));
        let mut changed = state.clone();
        changed["editors"][1]["documents"][0]["project"]["name"] =
            json!("different native project");
        assert_ne!(document_identity(&changed, &board).unwrap(), fingerprint);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["project"] = serde_json::Value::Null;
        assert!(
            document_identity(&changed, &board).is_ok(),
            "Standalone boards remain addressable without inventing a native project"
        );
        let mut variants = Vec::new();
        changed = state.clone();
        changed["evidence_source"] = json!("file");
        variants.push(changed);
        changed = state.clone();
        changed["kicad_version"]["patch"] = json!(6);
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["addressable"] = json!(false);
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"] = json!([]);
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"] = json!([doc, doc]);
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["document_path"] = json!(other);
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["editor"] = json!("schematic");
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["sheet_instance_path"] = json!({"kiids":[]});
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["document_path"] = json!("relative.kicad_pcb");
        variants.push(changed);
        changed = state.clone();
        changed["editors"][1]["documents"][0]["project"]["path"] = json!("relative");
        variants.push(changed);
        changed = state.clone();
        changed["editors"] = json!([state["editors"][1], state["editors"][1]]);
        variants.push(changed);
        for invalid in variants {
            assert!(document_identity(&invalid, &board).is_err());
        }
        let read = Observation::from_native(
            &json!({"source":"ipc"}),
            json!({"count":0,"components":[]}),
            &fingerprint,
        )
        .unwrap();
        let published = serde_json::to_string(&read).unwrap();
        assert_eq!(read.document_sha256, fingerprint);
        assert!(!published.contains("owned native project"));
        assert!(!published.contains("selected.kicad_pcb"));
        assert!(Observation::from_native(
            &json!({"source":"ipc"}),
            json!({"count":0,"components":[]}),
            "invalid"
        )
        .is_err());
    }
    #[test]
    fn observations_require_ipc_and_a_complete_unambiguous_native_projection() {
        let c = json!({"reference":"R1","value":"10k","footprint":"R_0603","x":25.4,"y":10.,"rotation":90.,"layer":"F.Cu"});
        let v = json!({"count":1,"components":[c]});
        let observed = observe(&json!({"source":"ipc"}), v.clone()).unwrap();
        assert_eq!(observed.components[0].x, 25.4);
        assert!(!serde_json::to_string(&observed).unwrap().contains("token"));
        for source in ["file", "saved_board", ""] {
            assert!(observe(&json!({"source":source}), v.clone()).is_err());
        }
        for count in [0, 2, 257] {
            let mut invalid = v.clone();
            invalid["count"] = json!(count);
            assert!(observe(&json!({"source":"ipc"}), invalid).is_err());
        }
        let duplicate = json!({"count":2,"components":[c,c]});
        assert!(observe(&json!({"source":"ipc"}), duplicate).is_err());
        let mut changed = v.clone();
        changed["components"][0]["x"] = json!(26.4);
        assert_ne!(
            observed.components_sha256,
            observe(&json!({"source":"ipc"}), changed)
                .unwrap()
                .components_sha256
        );
        let mut large = v;
        large["components"][0]["x"] = json!(1e100);
        assert!(observe(&json!({"source":"ipc"}), large).is_err());
    }
}
