//! Native-window acceptance of a board copy; no provider or original-board mutation.
#[path="support/owned_kicad.rs"]
mod owned_kicad;
use hazlomexa::{host_tools::HostTools,jobs::Workflow,kicad_live::Connection,project::{ArtifactFormat,OperationInput,Phase,Project}};
use serde_json::json;
use std::{fs,path::{Path,PathBuf},sync::{Arc,atomic::AtomicBool},time::{Duration,Instant}};
fn copy_profile(source:&Path,destination:&Path){
    fs::create_dir(destination).unwrap();
    for item in fs::read_dir(source).unwrap(){let item=item.unwrap();let name=item.file_name();let kind=item.file_type().unwrap();
        assert!(!kind.is_symlink());if kind.is_dir(){copy_profile(&item.path(),&destination.join(name));}else{assert!(kind.is_file());fs::copy(item.path(),destination.join(name)).unwrap();}
    }
}
fn main(){
    let workspace=PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().to_owned();
    let previous:serde_json::Value=serde_json::from_slice(&fs::read(workspace.join("target/current-kicad-live-ui.json")).unwrap()).unwrap();
    let previous=PathBuf::from(previous["root"].as_str().unwrap()).canonicalize().unwrap();
    assert_eq!(previous.parent(),Some(workspace.join("target").canonicalize().unwrap().as_path()));
    assert!(previous.file_name().unwrap().to_str().unwrap().starts_with("kicad-live-ui-"));
    let root=workspace.join(format!("target/kicad-live-ui-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
    copy_profile(&previous.join("profile"),&root.join("profile"));fs::create_dir(root.join("temp")).unwrap();
    let board=root.join("OwnedBoard.kicad_pcb");
    let source=fs::read(workspace.join("docs/research/konnect-pcb-2026-10-12/Original/basic_test.kicad_pcb")).unwrap();fs::write(&board,&source).unwrap();
    fs::write(workspace.join("target/current-owned-kicad-copy.json"),serde_json::to_vec(&json!({"root":root})).unwrap()).unwrap();
    let _editor=owned_kicad::editor(&workspace,&root,&board,&root);
    let project=root.join("task");fs::create_dir(&project).unwrap();let project=Project::create(&project).unwrap();let project_id=project.id;
    let workflow=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap());workflow.select_project(project).unwrap();
    let host=HostTools::new(workflow.clone());let data=root.join("host-data");fs::create_dir_all(data.join("program-locations")).unwrap();
    fs::write(data.join("program-locations/portable-folders.json"),serde_json::to_vec(&json!({"kicad":[workspace.join("target/konnect-probe/runtime"),workspace.join("target/kicad-probe/runtime/bin")]})).unwrap()).unwrap();
    host.configure_native_discovery(root.join("resources"),data).unwrap();
    let connection=Connection{endpoint:format!("ipc://{}",root.join("temp/kicad/api.sock").display()),token:String::new()};
    let deadline=Instant::now()+Duration::from_secs(45);
    let selected=loop{
        if let Ok(selected)=host.select_live_pcb(project_id,board.clone(),connection.clone(),Arc::new(AtomicBool::new(false))){break selected;}
        assert!(Instant::now()<deadline,"The owned KiCad window did not become ready");std::thread::sleep(Duration::from_millis(250));
    };
    let original=selected.observation.clone();let component=original.components.iter().find(|c|c.reference=="R1").unwrap();
    // A divergent saved file makes a stale-disk substitution fail this acceptance.
    let disk=b"owned stale disk sentinel: native memory remains the original PCB";fs::write(&board,disk).unwrap();
    let prepared=host.call("propose_selected_live_pcb_move",json!({"selection":selected.id,"instance_sha256":original.instance_sha256,"document_sha256":original.document_sha256,
        "state_sha256":original.state_sha256,"component":component.uuid,"units":"mm","x":component.x_mm+1.,"y":component.y_mm,"description":"Move the resistor right in a separate native copy",
        "agreement":{"kind":"software_operation","outcome":"Move the resistor right in a checked native working copy","assumptions":["Physical function is unverified"],"constraints":["Preserve the open board and saved original"],"losses":["Pending settings from other windows are not captured"],"verification":"Reopen the copied board through KiCad and check the moved component"}})).unwrap();
    assert_eq!(prepared["scope"],"captured_native_pcb_working_copy");let selection=prepared["working_copy_selection"].clone();
    let pending=workflow.view().unwrap().job.unwrap();let OperationInput::KonnectPcb(input)=&pending.snapshot.input else{panic!("Expected an isolated native copy")};
    assert_eq!(input.native_origin.as_ref().unwrap().state_sha256,original.state_sha256);assert!(input.agreement.is_some());
    assert_eq!(host.call("inspect_selected_live_pcb",json!({"selection":selected.id})).unwrap()["observation"]["state_sha256"],original.state_sha256);
    let done=workflow.execute(workflow.begin(pending.approval_id.unwrap()).unwrap()).unwrap();assert_eq!(done.snapshot.phase,Phase::Completed,"{:?}",done.error);
    let first=done.snapshot.input.job_id();let report=host.call("read_verified_result",json!({"job_id":first})).unwrap();assert_eq!(report["format"],"konnect_report");
    let followup=host.call("inspect_selected_pcb",json!({"selection":selection,"base_job_id":first,"result_sha256":report["sha256"]})).unwrap();
    let moved=followup["components"].as_array().unwrap().iter().find(|c|c["reference"]=="R1").unwrap();assert_eq!(moved["x_mm"],component.x_mm+1.);
    host.call("propose_pcb_move",json!({"selection":selection,"base_job_id":first,"result_sha256":report["sha256"],"component":component.uuid,"units":"in","x":(component.x_mm+3.54)/25.4,"y":component.y_mm/25.4})).unwrap();
    let pending=workflow.view().unwrap().job.unwrap();let second=workflow.execute(workflow.begin(pending.approval_id.unwrap()).unwrap()).unwrap();assert_eq!(second.snapshot.phase,Phase::Completed,"{:?}",second.error);
    workflow.artifact(first,ArtifactFormat::KonnectEdited).unwrap();workflow.artifact(second.snapshot.input.job_id(),ArtifactFormat::KonnectEdited).unwrap();
    assert_eq!(fs::read(&board).unwrap(),disk);
    assert_eq!(host.call("inspect_selected_live_pcb",json!({"selection":selected.id})).unwrap()["observation"]["state_sha256"],original.state_sha256);
    let project_root=workflow.view().unwrap().project.unwrap().1;drop(host);drop(workflow);let recovered=Project::open(&project_root).unwrap().recover().unwrap();
    assert_eq!(recovered.len(),2);assert!(recovered.iter().all(|r|r.phase==Phase::Completed && matches!(r.input,OperationInput::KonnectPcb(_))));
    let restored=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("restored-profiles")).unwrap());restored.select_project(Project::open(&project_root).unwrap()).unwrap();
    let restored=HostTools::new(restored);let result=restored.call("read_verified_result",json!({"job_id":second.snapshot.input.job_id()})).unwrap();
    let context=result["pcb_followup"].clone();assert!(context["selection"].is_string());
    let final_board=restored.call("inspect_selected_pcb",json!({"selection":context["selection"],"base_job_id":context["base_job_id"],"result_sha256":context["result_sha256"]})).unwrap();
    let final_component=final_board["components"].as_array().unwrap().iter().find(|c|c["reference"]=="R1").unwrap();assert_eq!(final_component["x_mm"],component.x_mm+3.54);
    fs::write(root.join("acceptance.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_program_window":true,"provider_used":false,"automatic_tool_detection":true,
        "native_capture_over_stale_disk":true,"native_original_unchanged":true,"saved_original_unchanged":true,"checked_copies":2,"imperial_followup":true,"recovered_jobs":2,"restored_followup_selection":true,
        "gui_copy_reopen_checked":false,"physical_function_checked":false})).unwrap()).unwrap();
    println!("Owned KiCad native capture, isolated copy/edit/readback, imperial follow-up and recovery passed; originals preserved.");
}
