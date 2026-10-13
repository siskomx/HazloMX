//! Typed schematic edits and independent verification before publication.
use serde::{Deserialize, Serialize};

const LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueEdit {
    pub reference: String,
    pub expected_value: String,
    pub value: String,
}
impl ValueEdit {
    pub fn validate(&self) -> Result<(), String> {
        if self.reference.is_empty()
            || self.reference.len() > 64
            || !self
                .reference
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || self.value.trim().is_empty()
            || self.value.len() > 1024
            || self.expected_value.len() > 1024
            || self
                .value
                .chars()
                .chain(self.expected_value.chars())
                .any(char::is_control)
            || self.value == self.expected_value
        {
            return Err(
                "Confirma la referencia, el valor actual y el nuevo valor del componente.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Node {
    List(Vec<Node>),
    Quoted(String),
    Atom(String),
}

// Compare decimal spellings exactly without rounding large numbers through f64.
fn number(value: &str) -> Option<String> {
    let (mantissa, exponent) = value.split_once(['e', 'E']).unwrap_or((value, "0"));
    let exponent: i32 = exponent.parse().ok()?;
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.strip_prefix(['-', '+']).unwrap_or(mantissa);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some("0".into());
    }
    let trimmed = digits.trim_end_matches('0');
    let power = exponent
        .checked_sub(i32::try_from(fraction.len()).ok()?)?
        .checked_add(i32::try_from(digits.len() - trimmed.len()).ok()?)?;
    Some(format!(
        "{}{trimmed}e{power}",
        if negative { "-" } else { "" }
    ))
}

fn parse(bytes: &[u8]) -> Result<Node, String> {
    parse_document(bytes, "kicad_sch")
}
fn parse_document(bytes: &[u8], kind: &str) -> Result<Node, String> {
    if bytes.len() > LIMIT {
        return Err("El esquema supera el límite de verificación.".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "El esquema debe usar UTF-8.")?;
    let mut stack: Vec<Vec<Node>> = Vec::new();
    let mut root = None;
    let mut index = 0;
    let mut count = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        count += 1;
        if count > 500_000 {
            return Err("El esquema contiene demasiados elementos.".into());
        }
        let node = match bytes[index] {
            b'(' => {
                if stack.len() >= 128 {
                    return Err("El esquema contiene demasiados niveles.".into());
                }
                stack.push(Vec::new());
                index += 1;
                continue;
            }
            b')' => {
                index += 1;
                let list = stack.pop().ok_or("Paréntesis del esquema no válidos.")?;
                if !matches!(list.first(), Some(Node::Atom(_))) {
                    return Err("Nodo del esquema no válido.".into());
                }
                Node::List(list)
            }
            b'"' => {
                let start = index;
                index += 1;
                let mut closed = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => {
                            index += 2;
                        }
                        b'"' => {
                            index += 1;
                            closed = true;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                if !closed {
                    return Err("Cadena del esquema incompleta.".into());
                }
                Node::Quoted(
                    serde_json::from_str(&text[start..index])
                        .map_err(|_| "Cadena del esquema no válida.")?,
                )
            }
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'(' | b')' | b'"')
                {
                    index += 1;
                }
                let atom = &text[start..index];
                Node::Atom(number(atom).unwrap_or_else(|| atom.into()))
            }
        };
        if let Some(parent) = stack.last_mut() {
            parent.push(node);
        } else if root.replace(node).is_some() {
            return Err("El esquema contiene varias raíces.".into());
        }
    }
    if !stack.is_empty() {
        return Err("Estructura del esquema incompleta.".into());
    }
    let root = root.ok_or("Esquema vacío.")?;
    if !matches!(&root, Node::List(list) if list.first() == Some(&Node::Atom(kind.into()))) {
        return Err(format!("Seleccione un documento {kind}."));
    }
    Ok(root)
}

fn property(node: &Node, name: &str) -> bool {
    matches!(node, Node::List(list) if list.len() >= 3 && list[0] == Node::Atom("property".into()) && list[1] == Node::Quoted(name.into()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Component {
    pub reference: String,
    pub value: String,
    pub library: String,
}

/// Inventory placed components only; library templates are not editable instances.
pub fn components(bytes: &[u8]) -> Result<Vec<Component>, String> {
    let Node::List(root) = parse(bytes)? else {
        unreachable!()
    };
    let mut found = std::collections::BTreeMap::new();
    for node in root.iter().skip(1) {
        let Node::List(symbol) = node else { continue };
        if symbol.first() != Some(&Node::Atom("symbol".into())) {
            continue;
        }
        let text_property = |name: &str| -> Result<String, String> {
            let mut matches = symbol.iter().filter(|n| property(n, name));
            let Some(Node::List(value)) = matches.next() else {
                return Err(format!("Propiedad {name} ausente."));
            };
            let Some(Node::Quoted(text)) = value.get(2) else {
                return Err(format!("Propiedad {name} no válida."));
            };
            if matches.next().is_some() || text.len() > 1024 || text.chars().any(char::is_control) {
                return Err(format!("Propiedad {name} duplicada o no válida."));
            }
            Ok(text.clone())
        };
        let reference = text_property("Reference")?;
        let reference_body = reference.strip_prefix('#').unwrap_or(&reference);
        if reference_body.is_empty()
            || reference.len() > 64
            || !reference_body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("Referencia de componente no válida.".into());
        }
        let mut libraries = symbol.iter().filter_map(|node| match node {
            Node::List(list) if list.first() == Some(&Node::Atom("lib_id".into())) => Some(list),
            _ => None,
        });
        let library = match libraries.next() {
            Some(list) => match list.get(1) {
                Some(Node::Quoted(text))
                    if text.len() <= 1024 && !text.chars().any(char::is_control) =>
                {
                    text.clone()
                }
                _ => return Err("Biblioteca no válida.".into()),
            },
            None => String::new(),
        };
        if libraries.next().is_some() {
            return Err("Biblioteca duplicada.".into());
        }
        let component = Component {
            reference: reference.clone(),
            value: text_property("Value")?,
            library,
        };
        if let Some(previous) = found.insert(reference, component.clone()) {
            if previous != component {
                return Err("Las unidades del componente tienen datos distintos.".into());
            }
        }
        if found.len() > 256 {
            return Err("La selección admite hasta 256 componentes por hoja.".into());
        }
    }
    Ok(found.into_values().collect())
}

pub fn inspect_file(path: &std::path::Path) -> Result<(String, Vec<Component>), String> {
    use sha2::{Digest, Sha256};
    let bytes = crate::kicad_project::read_regular(path, LIMIT as u64)?;
    Ok((format!("{:x}", Sha256::digest(&bytes)), components(&bytes)?))
}

pub fn verify_value_edit(before: &[u8], after: &[u8], edit: &ValueEdit) -> Result<(), String> {
    if expected_edit(before, edit)? != parse(after)? {
        return Err("La copia no conserva el esquema fuera del valor aprobado.".into());
    }
    Ok(())
}

pub fn validate_current_value(before: &[u8], edit: &ValueEdit) -> Result<(), String> {
    expected_edit(before, edit).map(|_| ())
}

pub fn current_value(bytes: &[u8], reference: &str) -> Result<String, String> {
    if reference.is_empty()
        || reference.len() > 64
        || !reference
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Referencia de componente no válida.".into());
    }
    let Node::List(root) = parse(bytes)? else {
        unreachable!()
    };
    let mut value = None;
    for node in root.iter().skip(1) {
        let Node::List(symbol) = node else { continue };
        if symbol.first() != Some(&Node::Atom("symbol".into())) {
            continue;
        }
        let references: Vec<_> = symbol.iter().filter(|n| property(n, "Reference")).collect();
        if references.len() != 1 {
            return Err("Referencia ausente o duplicada.".into());
        }
        let Node::List(selected) = references[0] else {
            unreachable!()
        };
        if selected[2] != Node::Quoted(reference.into()) {
            continue;
        }
        let values: Vec<_> = symbol.iter().filter(|n| property(n, "Value")).collect();
        if values.len() != 1 {
            return Err("Valor ausente o duplicado.".into());
        }
        let Node::List(selected) = values[0] else {
            unreachable!()
        };
        let Node::Quoted(current) = &selected[2] else {
            return Err("Valor no válido.".into());
        };
        if value.as_ref().is_some_and(|previous| previous != current) {
            return Err("Las unidades del componente tienen valores distintos.".into());
        }
        value = Some(current.clone());
    }
    value.ok_or("No se encontró la referencia en el esquema seleccionado.".into())
}

fn expected_edit(before: &[u8], edit: &ValueEdit) -> Result<Node, String> {
    edit.validate()?;
    let mut expected = parse(before)?;
    let Node::List(root) = &mut expected else {
        unreachable!()
    };
    let mut changed = 0;
    for node in root.iter_mut().skip(1) {
        let Node::List(symbol) = node else { continue };
        if symbol.first() != Some(&Node::Atom("symbol".into())) {
            continue;
        }
        let references: Vec<_> = symbol.iter().filter(|v| property(v, "Reference")).collect();
        if references.len() != 1 {
            return Err("Referencia de componente ausente o duplicada.".into());
        }
        let Node::List(reference) = references[0] else {
            unreachable!()
        };
        if reference[2] != Node::Quoted(edit.reference.clone()) {
            continue;
        }
        let values: Vec<_> = symbol.iter_mut().filter(|v| property(v, "Value")).collect();
        if values.len() != 1 {
            return Err("Valor de componente ausente o duplicado.".into());
        }
        let Node::List(value) = values.into_iter().next().unwrap() else {
            unreachable!()
        };
        if value[2] != Node::Quoted(edit.expected_value.clone()) {
            return Err("El valor actual cambió. Inspecciona de nuevo antes de aprobar.".into());
        }
        value[2] = Node::Quoted(edit.value.clone());
        changed += 1;
    }
    if changed == 0 {
        return Err("La copia no conserva el esquema fuera del valor aprobado.".into());
    }
    Ok(expected)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintMove {
    pub reference: String,
    pub uuid: uuid::Uuid,
    pub expected_x_mm: f64,
    pub expected_y_mm: f64,
    pub x_mm: f64,
    pub y_mm: f64,
}
impl FootprintMove {
    pub fn validate(&self) -> Result<(), String> {
        // Konnect stores X/Y to six decimals; reject unreviewed rounding or a physical no-op.
        if self.uuid.is_nil()
            || self.reference.is_empty()
            || self.reference.len() > 64
            || !self
                .reference
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || [self.expected_x_mm, self.expected_y_mm, self.x_mm, self.y_mm]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 10000.0)
            || [self.x_mm, self.y_mm]
                .iter()
                .any(|v| (v * 1_000_000.0 - (v * 1_000_000.0).round()).abs() > 0.00001)
            || ((self.expected_x_mm * 1_000_000.0).round() == (self.x_mm * 1_000_000.0).round()
                && (self.expected_y_mm * 1_000_000.0).round() == (self.y_mm * 1_000_000.0).round())
        {
            return Err(
                "Confirma el componente y una posición nueva en milímetros con hasta seis decimales."
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Footprint {
    pub reference: String,
    pub uuid: uuid::Uuid,
    pub library: String,
    pub value: String,
    pub layer: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub rotation_deg: f64,
}
fn tagged(node: &Node, tag: &str) -> bool {
    matches!(node,Node::List(list) if list.first()==Some(&Node::Atom(tag.into())))
}
fn footprint_details(nodes: &[Node]) -> Result<Footprint, String> {
    let references: Vec<_> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::List(v) if property(node, "Reference") => match &v[2] {
                Node::Quoted(r) => Some(r.clone()),
                _ => None,
            },
            Node::List(v)
                if v.len() >= 3
                    && v[0] == Node::Atom("fp_text".into())
                    && v[1] == Node::Atom("reference".into()) =>
            {
                match &v[2] {
                    Node::Quoted(r) => Some(r.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    let ids: Vec<_> = nodes
        .iter()
        .filter(|n| tagged(n, "uuid") || tagged(n, "tstamp"))
        .collect();
    let positions: Vec<_> = nodes.iter().filter(|n| tagged(n, "at")).collect();
    if references.len() != 1 || ids.len() != 1 || positions.len() != 1 {
        return Err("La PCB tiene un componente ambiguo o sin posición/identidad.".into());
    }
    let Node::List(id) = ids[0] else {
        unreachable!()
    };
    let uuid = match id.as_slice() {
        [_, Node::Atom(value) | Node::Quoted(value)] => value
            .parse::<uuid::Uuid>()
            .map_err(|_| "Identidad de componente no válida.")?,
        _ => return Err("Identidad de componente no válida.".into()),
    };
    let Node::List(at) = positions[0] else {
        unreachable!()
    };
    if !matches!(at.len(), 3 | 4) {
        return Err("Posición de componente no válida.".into());
    }
    let coordinate = |index: usize| -> Result<f64, String> {
        let Some(Node::Atom(v)) = at.get(index) else {
            return Err("Coordenada de componente no válida.".into());
        };
        let value = v
            .parse::<f64>()
            .map_err(|_| "Coordenada de componente no válida.")?;
        if !value.is_finite() {
            return Err("Coordenada de componente no válida.".into());
        }
        Ok(value)
    };
    let library = match nodes.get(1) {
        Some(Node::Quoted(v)) => v.clone(),
        _ => return Err("Huella de componente no válida.".into()),
    };
    let values: Vec<_> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::List(v) if property(node, "Value") => match &v[2] {
                Node::Quoted(value) => Some(value.clone()),
                _ => None,
            },
            Node::List(v)
                if v.len() >= 3
                    && v[0] == Node::Atom("fp_text".into())
                    && v[1] == Node::Atom("value".into()) =>
            {
                match &v[2] {
                    Node::Quoted(value) => Some(value.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    if values.len() > 1 {
        return Err("La PCB tiene un valor de componente ambiguo.".into());
    }
    let layers: Vec<_> = nodes.iter().filter(|node| tagged(node, "layer")).collect();
    let layer = match layers.as_slice() {
        [Node::List(v)] => match v.as_slice() {
            [_, Node::Quoted(layer)] => layer.clone(),
            _ => return Err("Capa de componente no válida.".into()),
        },
        [] => String::new(),
        _ => return Err("Capa de componente ambigua.".into()),
    };
    if uuid.is_nil() || references[0].is_empty() || references[0].len() > 64 {
        return Err("Referencia de componente no válida.".into());
    }
    Ok(Footprint {
        reference: references[0].clone(),
        uuid,
        library,
        value: values.into_iter().next().unwrap_or_default(),
        layer,
        x_mm: coordinate(1)?,
        y_mm: coordinate(2)?,
        rotation_deg: if at.len() == 4 { coordinate(3)? } else { 0.0 },
    })
}
pub fn footprints(bytes: &[u8]) -> Result<Vec<Footprint>, String> {
    let Node::List(root) = parse_document(bytes, "kicad_pcb")? else {
        unreachable!()
    };
    if root.iter().any(|node| tagged(node, "module")) {
        return Err(
            "Actualiza la PCB antigua en KiCad antes de seleccionar sus componentes.".into(),
        );
    }
    let mut result = Vec::new();
    let mut references = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for node in root.iter().filter(|n| tagged(n, "footprint")) {
        let Node::List(nodes) = node else {
            unreachable!()
        };
        let footprint = footprint_details(nodes)?;
        if !references.insert(footprint.reference.clone()) || !ids.insert(footprint.uuid) {
            return Err("La PCB repite una referencia o identidad de componente.".into());
        }
        result.push(footprint);
    }
    Ok(result)
}
/// Verify the whole board: only the chosen footprint's root X/Y may change.
pub fn verify_footprint_move(
    before: &[u8],
    after: &[u8],
    edit: &FootprintMove,
) -> Result<(), String> {
    validate_current_position(before, edit)?;
    let mut expected = parse_document(before, "kicad_pcb")?;
    let Node::List(root) = &mut expected else {
        unreachable!()
    };
    for node in root.iter_mut().filter(|n| tagged(n, "footprint")) {
        let Node::List(nodes) = node else {
            unreachable!()
        };
        if footprint_details(nodes)?.uuid != edit.uuid {
            continue;
        }
        let Node::List(at) = nodes.iter_mut().find(|n| tagged(n, "at")).unwrap() else {
            unreachable!()
        };
        at[1] = Node::Atom(number(&format!("{:.6}", edit.x_mm)).ok_or("Coordenada no válida.")?);
        at[2] = Node::Atom(number(&format!("{:.6}", edit.y_mm)).ok_or("Coordenada no válida.")?);
        if at.len() == 4 && at[3] == Node::Atom("0".into()) {
            at.pop();
        }
    }
    footprints(after)?;
    if parse_document(after, "kicad_pcb")? != expected {
        return Err("La PCB cambió fuera de la posición aprobada.".into());
    }
    Ok(())
}
pub fn validate_current_position(before: &[u8], edit: &FootprintMove) -> Result<(), String> {
    edit.validate()?;
    let inventory = footprints(before)?;
    let selected = inventory
        .iter()
        .find(|fp| fp.reference == edit.reference && fp.uuid == edit.uuid)
        .ok_or("El componente seleccionado cambió.")?;
    if selected.x_mm != edit.expected_x_mm || selected.y_mm != edit.expected_y_mm {
        return Err("La posición actual cambió. Inspecciona de nuevo antes de aprobar.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const BOARD: &[u8] = include_bytes!(
        "../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_pcb"
    );
    fn board_move() -> FootprintMove {
        let selected = footprints(BOARD)
            .unwrap()
            .into_iter()
            .find(|fp| fp.reference == "J1")
            .unwrap();
        FootprintMove {
            reference: selected.reference,
            uuid: selected.uuid,
            expected_x_mm: selected.x_mm,
            expected_y_mm: selected.y_mm,
            x_mm: 203.34,
            y_mm: 155.33,
        }
    }
    #[test]
    fn board_move_verifies_whole_board_and_rejects_unrelated_changes() {
        let before = std::str::from_utf8(BOARD).unwrap();
        let after = before.replacen("(at 200.8 156.6)", "(at 203.34 155.33)", 1);
        let edit = board_move();
        verify_footprint_move(BOARD, after.as_bytes(), &edit).unwrap();
        for (old, new) in [
            ("(size 3 3)", "(size 4 3)"),
            ("(at 201.9 145.3 180)", "(at 202 145.3 180)"),
            ("(net 6 \"/OUT\")", "(net 6 \"/OTHER\")"),
        ] {
            let tampered = after.replacen(old, new, 1);
            assert_ne!(tampered, after);
            assert!(verify_footprint_move(BOARD, tampered.as_bytes(), &edit).is_err());
        }
        let mut stale = edit.clone();
        stale.expected_x_mm += 1.0;
        assert!(verify_footprint_move(BOARD, after.as_bytes(), &stale).is_err());
        stale = edit.clone();
        stale.uuid = uuid::Uuid::new_v4();
        assert!(verify_footprint_move(BOARD, after.as_bytes(), &stale).is_err());
        stale = edit;
        stale.x_mm = f64::NAN;
        assert!(stale.validate().is_err());
        stale = board_move();
        stale.x_mm = 203.3400004;
        assert!(stale.validate().is_err());
        let converted = FootprintMove {
            x_mm: 200.8 + 2.54,
            y_mm: 156.6 - 1.27,
            ..board_move()
        };
        verify_footprint_move(BOARD, after.as_bytes(), &converted).unwrap();
        let no_op = FootprintMove {
            x_mm: 200.8 + 0.000000000001,
            y_mm: 156.6,
            ..board_move()
        };
        assert!(no_op.validate().is_err());
    }
    #[test]
    fn board_inventory_rejects_ambiguous_missing_and_invalid_identity() {
        let footprint = r#"(footprint "Device:R" (uuid "1390739f-0b91-4d50-b8ae-92acd725ec51") (at 1 2 90) (property "Reference" "R1"))"#;
        let board = format!("(kicad_pcb {footprint})");
        let found = footprints(board.as_bytes()).unwrap();
        assert_eq!(found[0].rotation_deg, 90.0);
        assert!(footprints(format!("(kicad_pcb {footprint} {footprint})").as_bytes()).is_err());
        for (old, new) in [
            ("(at 1 2 90)", ""),
            ("(at 1 2 90)", "(at 1e999 2)"),
            ("1390739f-0b91-4d50-b8ae-92acd725ec51", "not-a-uuid"),
            (
                "(property \"Reference\" \"R1\")",
                "(property \"Reference\" \"R1\") (fp_text reference \"R1\")",
            ),
        ] {
            assert!(footprints(board.replace(old, new).as_bytes()).is_err());
        }
        assert!(footprints(b"(kicad_sch)").is_err());
        assert!(footprints(b"(kicad_pcb (module legacy))").is_err());
    }
    fn edit() -> ValueEdit {
        ValueEdit {
            reference: "R1".into(),
            expected_value: "10k".into(),
            value: "12.34k".into(),
        }
    }
    const BEFORE: &[u8] = br#"(kicad_sch (symbol (property "Reference" "R1") (property "Value" "10k") (at 1.0 2)) (wire (pts (xy 0 0) (xy 1 2))))"#;
    #[test]
    fn discovery_lists_instances_and_refuses_ambiguous_units() {
        let source = br#"(kicad_sch (lib_symbols (symbol "template" (property "Reference" "R") (property "Value" "R"))) (symbol (lib_id "Device:R") (property "Reference" "R1") (property "Value" "10k")) (symbol (lib_id "Device:R") (property "Reference" "R1") (property "Value" "10k")))"#;
        assert_eq!(
            components(source).unwrap(),
            vec![Component {
                reference: "R1".into(),
                value: "10k".into(),
                library: "Device:R".into()
            }]
        );
        let changed = String::from_utf8(source.to_vec())
            .unwrap()
            .replacen("\"10k\"", "\"20k\"", 1);
        assert!(components(changed.as_bytes()).is_err());
        let actual = include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_sch");
        let inventory = components(actual).unwrap();
        assert_eq!(
            inventory
                .iter()
                .find(|c| c.reference == "R1")
                .unwrap()
                .value,
            "10k"
        );
        assert!(components(
            br#"(kicad_sch (symbol (property "Reference" "R1") (property "Value" "<img>")))"#
        )
        .is_ok());
        assert!(components(br#"(kicad_sch (symbol (property "Reference" "R1") (property "Value" "10k") (property "Value" "20k")))"#).is_err());
    }
    #[test]
    fn value_only_and_number_normalization() {
        assert_eq!(current_value(BEFORE, "R1").unwrap(), "10k");
        assert!(current_value(BEFORE, "missing").is_err());
        assert!(current_value(BEFORE, "R1/..").is_err());
        let after = String::from_utf8(BEFORE.to_vec())
            .unwrap()
            .replace("10k", "12.34k")
            .replace("1.0", "1e0");
        verify_value_edit(BEFORE, after.as_bytes(), &edit()).unwrap();
        for changed in [
            after.replace("(xy 0 0)", "(xy 0 1)"),
            after.replace("(at 1e0 2)", "(at 3 2)"),
            after.replace("R1", "R2"),
        ] {
            assert!(verify_value_edit(BEFORE, changed.as_bytes(), &edit()).is_err());
        }
        assert_ne!(number("9007199254740992"), number("9007199254740993"));
        assert_eq!(number("-0.000"), number("0"));
        assert_eq!(number("12.3400e-2"), number("0.1234"));
    }
    #[test]
    fn malformed_and_stale_inputs_refused() {
        for bytes in [
            b"".as_slice(),
            b"(kicad_sch",
            b"(kicad_sch))",
            b"(other)",
            b"(kicad_sch) (kicad_sch)",
            br#"(kicad_sch "unterminated)"#,
        ] {
            assert!(parse(bytes).is_err());
        }
        let mut stale = edit();
        stale.expected_value = "20k".into();
        assert!(verify_value_edit(BEFORE, BEFORE, &stale).is_err());
        for value in ["", "10k", "x\n", "x\0"] {
            let mut invalid = edit();
            invalid.value = value.into();
            assert!(invalid.validate().is_err());
        }
    }
    #[test]
    fn retained_actual_edit_passes_and_placement_tamper_fails() {
        let before=include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_sch");
        let after = include_bytes!(
            "../../../../docs/research/konnect-runtime-2026-10-11/raw/Edited/basic_test.kicad_sch"
        );
        verify_value_edit(before, after, &edit()).unwrap();
        let changed = String::from_utf8(after.to_vec())
            .unwrap()
            .replacen("127", "128", 1);
        assert!(verify_value_edit(before, changed.as_bytes(), &edit()).is_err());
    }

    #[test]
    fn duplicate_properties_and_resource_limits_are_refused() {
        let duplicate = String::from_utf8(BEFORE.to_vec()).unwrap().replace(
            "(property \"Value\" \"10k\")",
            "(property \"Value\" \"10k\") (property \"Value\" \"10k\")",
        );
        assert!(verify_value_edit(duplicate.as_bytes(), duplicate.as_bytes(), &edit()).is_err());
        assert!(parse(&vec![b' '; LIMIT + 1]).is_err());
        let deep = format!("{}{}", "(kicad_sch ".repeat(129), ")".repeat(129));
        assert!(parse(deep.as_bytes()).is_err());
        let mut invalid = edit();
        invalid.reference = "R1/../../".into();
        assert!(invalid.validate().is_err());
    }
}
