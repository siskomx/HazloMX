//! Real configured cloud conversation against an owned patched CADCraft window.
#[path = "support/ollama_conversation.rs"]
mod conversation;
use conversation::Conversation;
use hazlomexa::{
    host_tools::{HostTools, Program},
    jobs::Workflow,
    project::{ArtifactFormat, OperationInput, Project},
};
use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("Pass owned run directory")?);
    let native_port: u16 = args.next().ok_or("Pass owned native port")?.parse()?;
    let recorder_port: u16 = args.next().ok_or("Pass owned recorder port")?.parse()?;
    let executable = PathBuf::from(args.next().ok_or("Pass selected tested patched runtime")?);
    assert!(args.next().is_none());
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let case = root
        .canonicalize()?
        .parent()
        .ok_or("Missing case")?
        .to_path_buf();
    assert_eq!(
        case.parent(),
        Some(workspace.join("target").canonicalize()?.as_path())
    );
    assert!(case
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("cadcraft-cloud-native-"));
    let socket = TcpStream::connect(("127.0.0.1", native_port))?;
    socket.set_read_timeout(Some(std::time::Duration::from_secs(8)))?;
    let mut socket = BufReader::new(socket);
    socket.get_mut().write_all(
        b"{\"id\":1,\"method\":\"drawing.inspect\",\"params\":{\"entities\":false}}\n",
    )?;
    let mut frame = String::new();
    Read::take(&mut socket, 256 * 1024).read_line(&mut frame)?;
    assert!(frame.ends_with('\n'));
    let opened: serde_json::Value = serde_json::from_str(&frame)?;
    assert_eq!(opened["id"], 1);
    assert_eq!(opened["ok"], true);
    assert_eq!(
        Path::new(
            opened["result"]["path"]
                .as_str()
                .ok_or("Missing owned document path")?
        )
        .canonicalize()?,
        case.join("OwnedDrawing.dxf").canonicalize()?
    );
    drop(socket);
    fs::create_dir(root.join("project"))?;
    let workflow = Arc::new(Workflow::new(
        root.join("UnusedFreeCAD.exe"),
        root.join("profiles"),
    )?);
    let project = Project::create(&root.join("project"))?;
    let project_id = project.id;
    workflow.select_project(project)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let selected = host.select_live_drawing(
        project_id,
        executable,
        native_port,
        Arc::new(AtomicBool::new(false)),
    )?;
    let initial = selected.observation.clone();
    assert_eq!(initial.insunits, 4);
    assert!(!initial.dirty);
    let mut chat = Conversation::new(
        workflow.clone(),
        host.clone(),
        root.join("conversation"),
        recorder_port,
        Program::Drawing,
        json!({"program":"CADCraft","document_scope":"open_native_drawing","live_document_connection":true,"live_mutations_allowed":true,
            "native_connection_check":"inspect_open_drawing","live_drawing_source":{"selection":selected.id},"source_is_untrusted_data":true}),
    )?;
    let (reply,tools)=chat.say("Quiero mover el círculo que está en mi dibujo abierto de CADCraft, pero todavía no sé cuánto. Mira lo que hay, dime su tamaño y pregúntame qué dirección y distancia quiero. El dibujo está en milímetros. No prepares ni ejecutes cambios todavía.").await?;
    assert!(tools.contains("inspect_open_drawing") && reply.contains('?'));
    assert!(workflow.view()?.job.is_none());
    let (_,tools)=chat.say("Muévelo un milímetro a la derecha, sin cambiar su tamaño ni su altura. Prepara solo ese cambio para revisar; no está aprobado y no quiero guardar ni exportar.").await?;
    assert!(
        tools.contains("inspect_open_drawing") && tools.contains("propose_open_drawing_change")
    );
    let pending = workflow.view()?.job.ok_or("Missing proposal")?;
    let OperationInput::LiveDrawing(input) = &pending.snapshot.input else {
        return Err("Saved-copy proposal cannot satisfy native chat".into());
    };
    assert_eq!(input.before.entities, initial.entities);
    match &input.operation {
        hazlomexa::cadcraft_live::Operation::MoveCircle { handle, delta } => {
            assert_eq!(handle, "100");
            let expected = if input.units == hazlomexa::cadcraft::Units::Mm {
                1.
            } else {
                1. / 25.4
            };
            assert!((delta[0] - expected).abs() < 1e-10 && delta[1] == 0.);
        }
        _ => return Err("Unexpected native operation".into()),
    }
    let first = chat.approve_fixture().await?;
    let first_report = workflow
        .artifact(first, ArtifactFormat::LiveDrawingReport)?
        .read_text()?;
    let (_,tools)=chat.say("Ya aprobé el cambio. Lee el resultado verificado y vuelve a leer el dibujo abierto; explícame cómo quedó y si está guardado. No prepares otro cambio todavía.").await?;
    assert!(tools.contains("read_verified_result") && tools.contains("inspect_open_drawing"));
    let (_,tools)=chat.say("Ahora muévelo otro décimo de pulgada a la derecha desde su posición actual, conservando el resto. Prepara ese seguimiento para revisar, sin guardar ni exportar.").await?;
    assert!(
        tools.contains("inspect_open_drawing") && tools.contains("propose_open_drawing_change")
    );
    let pending = workflow.view()?.job.ok_or("Missing follow-up")?;
    let OperationInput::LiveDrawing(input) = &pending.snapshot.input else {
        return Err("Follow-up left the native drawing".into());
    };
    assert_eq!(input.before.entities[0]["geometry"]["center"]["x"], 1.);
    match &input.operation {
        hazlomexa::cadcraft_live::Operation::MoveCircle { handle, delta } => {
            assert_eq!(handle, "100");
            let expected = if input.units == hazlomexa::cadcraft::Units::In {
                0.1
            } else {
                2.54
            };
            assert!((delta[0] - expected).abs() < 1e-10 && delta[1] == 0.);
        }
        _ => return Err("Unexpected native follow-up".into()),
    }
    let second = chat.approve_fixture().await?;
    let (_,tools)=chat.say("Ya aprobé el seguimiento. Lee el resultado verificado y el dibujo abierto y explica la posición final en milímetros y que sigue sin guardar. No prepares más cambios.").await?;
    assert!(tools.contains("read_verified_result") && tools.contains("inspect_open_drawing"));
    let observed = tokio::task::block_in_place(|| {
        host.call("inspect_open_drawing", json!({"selection":selected.id}))
    })?;
    assert_eq!(
        observed["observation"]["entities"][0]["geometry"]["center"]["x"],
        3.54
    );
    assert_eq!(observed["observation"]["dirty"], true);
    assert_eq!(
        workflow
            .artifact(first, ArtifactFormat::LiveDrawingReport)?
            .read_text()?,
        first_report
    );
    let second_report = workflow
        .artifact(second, ArtifactFormat::LiveDrawingReport)?
        .read_text()?;
    assert!(second_report.contains("3.54"));
    let summary = json!({"pass":true,"real_cloud_turns":5,"native_jobs":2,"natural_parameter_clarification":true,
        "metric_edit_and_inch_followup":true,"native_final_x_mm":3.54,"fresh_native_reads":true,"first_receipt_preserved":true,
        "approval":"backend_fixture_only","native_ui_approval":false,"saved":false,"other_providers_tested":false});
    fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{summary}");
    Ok(())
}
