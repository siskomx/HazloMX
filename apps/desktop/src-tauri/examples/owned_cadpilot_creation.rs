//! Actual empty-document creation through shared tools; backend fixture approvals only.
use hazlomexa::{host_tools::HostTools,jobs::Workflow,project::{ArtifactFormat,Phase,Project}};
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf},sync::Arc};

fn inspect(host:&HostTools)->Value {
    let read=host.call("inspect_open_cad_documents",json!({"document_name":"OwnedCreationFixture"})).unwrap();
    assert_eq!(read["document"],"OwnedCreationFixture");
    read
}
fn measure(observed:&Value,key:&str,expected:f64) {
    let objects=observed["objects"].as_array().unwrap();
    assert_eq!(objects.len(),1);
    assert_eq!(objects[0]["name"],"Support");
    let value=&objects[0]["properties"][key];
    let actual=if let Some(n)=value.as_f64() {n} else {
        let parts=value.as_str().unwrap().split_whitespace().collect::<Vec<_>>();
        assert_eq!(parts.len(),2);assert_eq!(parts[1],"mm");parts[0].parse::<f64>().unwrap()
    };
    assert!((actual-expected).abs()<1e-8,"Unexpected native dimension");
}
fn request(observed:&Value,operations:Value)->Value {
    json!({"document_name":"OwnedCreationFixture","document_token":observed["document_token"],
        "state_sha256":observed["state_sha256"],"description":"Create or revise the explicitly sized owned support","operations":operations})
}
fn main() {
    let root=PathBuf::from(std::env::var_os("HAZLOMEXA_NATIVE_CREATION_ROOT").expect("Owned fixture required"));
    let workspace=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap();
    assert_eq!(root.canonicalize().unwrap().parent(),Some(workspace.join("target").canonicalize().unwrap().as_path()));
    assert!(root.file_name().unwrap().to_str().unwrap().starts_with("native-creation-"));
    let marker:Value=serde_json::from_slice(&fs::read(root.join("native-ready.json")).unwrap()).unwrap();
    assert_eq!(marker["owned"],true);assert_eq!(marker["document"],"OwnedCreationFixture");
    let workflow=Arc::new(Workflow::new(root.join("unused-FreeCADCmd.exe"),root.join("profiles")).unwrap());
    workflow.select_project(Project::create(&root.join("project")).unwrap()).unwrap();
    let host=HostTools::new(workflow.clone());
    let before=inspect(&host);
    assert!(before["objects"].as_array().unwrap().is_empty());
    let missing=request(&before,json!([{"action":"create_object","obj_type":"Part::Box","obj_name":"Support"}]));
    assert!(host.call("propose_open_cad_change",missing).unwrap_err().contains("medidas"));
    assert!(workflow.current().unwrap().is_none());
    assert_eq!(inspect(&host)["state_sha256"],before["state_sha256"]);
    let mut current=before;
    let mut receipt_bytes=Vec::new();
    for (operations,height) in [
        (json!([{"action":"create_object","obj_type":"Part::Box","obj_name":"Support",
            "obj_properties":{"Length":"0.5 in","Width":"20 mm","Height":"10 mm"}}]),10.),
        (json!([{"action":"edit_object","obj_name":"Support","obj_properties":{"Height":"1 in"}}]),25.4),
    ] {
        let proposal=request(&current,operations);
        host.call("propose_open_cad_change",proposal.clone()).unwrap();
        let pending=workflow.current().unwrap().unwrap();
        assert_eq!(pending.snapshot.phase,Phase::AwaitingApproval);
        assert_eq!(inspect(&host)["state_sha256"],current["state_sha256"]);
        let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let result=workflow.execute(dispatch).unwrap();
        assert_eq!(result.snapshot.phase,Phase::Completed,"{:?}",result.error);
        let id=result.snapshot.input.job_id();
        let report=host.call("read_verified_result",json!({"job_id":id})).unwrap();
        assert_eq!(report["format"],"cadpilot_report");
        let bytes=workflow.artifact(id,ArtifactFormat::CadpilotReport).unwrap().read_text().unwrap();
        let parsed:Value=serde_json::from_str(&bytes).unwrap();
        current=inspect(&host);
        assert_eq!(parsed["after"]["state_sha256"],current["state_sha256"]);
        measure(&current,"Length",12.7);measure(&current,"Width",20.);measure(&current,"Height",height);
        assert!(host.call("propose_open_cad_change",proposal).is_err());
        receipt_bytes.push((id,bytes));
    }
    for (id,bytes) in receipt_bytes {assert_eq!(workflow.artifact(id,ArtifactFormat::CadpilotReport).unwrap().read_text().unwrap(),bytes);}
    fs::write(root.join("host-final-observation.json"),serde_json::to_vec_pretty(&current).unwrap()).unwrap();
    drop(host);drop(workflow);
    let project=Project::open(&root.join("project")).unwrap();
    let runs=project.recover().unwrap();
    assert_eq!(runs.len(),2);
    assert!(runs.iter().all(|run|run.phase==Phase::Completed && run.schema_version==13));
    fs::write(root.join("host-result.json"),serde_json::to_vec_pretty(&json!({"pass":true,
        "empty_document_creation":true,"missing_size_refused":true,"native_followup":true,
        "metric_and_imperial":true,"separate_fixture_approvals":true,"stale_refusal":true,
        "prior_receipt_preserved":true,"recovered_completed_jobs":2,"native_ui_approval":false,"real_model":false})).unwrap()).unwrap();
    println!("Owned native creation and follow-up passed with backend fixture approvals");
}
