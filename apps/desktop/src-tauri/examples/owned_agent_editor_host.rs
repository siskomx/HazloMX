//! Exercise the Rust host against the real companion transport in an owned test buffer.
use hazlomexa::{
    agent_editor::{self, Failure},
    host_tools::HostTools,
    jobs::Workflow,
    project::{ArtifactFormat, Phase, Project},
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};
use uuid::Uuid;

fn main() {
    let root = PathBuf::from(
        std::env::var_os("HAZLOMEXA_NATIVE_EDITOR_TEST_ROOT").expect("Owned fixture root required"),
    );
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("target")
        .canonicalize()
        .unwrap();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(canonical.parent(), Some(target.as_path()));
    let name=root.file_name().unwrap().to_str().unwrap();
    let actual=name.starts_with("arduino-native-ui-");
    assert!(actual || name.starts_with("native-arduino-transport-"));
    if actual {
        let marker:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("owned-native-probe.json")).unwrap()).unwrap();
        let result:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("native-api-result.json")).unwrap()).unwrap();
        assert_eq!(marker["owned"],true);
        assert_eq!(result["pass"],true);
        assert_eq!(result["native_undo_verified"],true);
    } else {
        assert_eq!(std::fs::read(root.join("owned-host-probe")).unwrap(),b"owned");
    }
    if std::env::var("HAZLOMEXA_ARDUINO_RETAINED_RESUME_ONLY").as_deref()==Ok("1"){
        assert!(actual);let project_root=std::fs::read_dir(&root).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("project-") && e.path().join(".hazlomexa").is_dir()).map(|e|e.path()).collect::<Vec<_>>();assert_eq!(project_root.len(),1);let project_root=&project_root[0];
        let project=Project::open(project_root).unwrap();let targets=project.history().unwrap().into_iter().filter(|s|matches!(&s.input,hazlomexa::project::OperationInput::LiveAgent(_)) && matches!(s.phase,Phase::Cancelled|Phase::Interrupted)).map(|s|s.input.job_id()).collect::<Vec<_>>();assert_eq!(targets.len(),1);drop(project);
        check_retained_copy(&root,&canonical,project_root,&root.join("sketchbook/OwnedSketch/OwnedSketch.ino"),targets[0]);println!("Retained Arduino native copy resume and offline link repair passed; no original editor launched.");return;
    }
    let discovery=agent_editor::default_directory().unwrap();
    assert_eq!(discovery,root.join("profile/.arduinoIDE/hazlomexa-editor"));
    let registration=if actual {discovery.clone()} else {root.clone()};
    let records = std::fs::read_dir(&registration)
        .unwrap()
        .map(Result::unwrap)
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), if actual {1} else {2});
    for record in &records { agent_editor::Connection::load(&record.path()).unwrap(); }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let sessions = runtime
        .block_on(agent_editor::discover(
            &registration,
            Arc::new(AtomicBool::new(false)),
        ))
        .unwrap();
    assert_eq!(sessions.len(), if actual {1} else {2});
    let serialized = serde_json::to_value(&sessions).unwrap();
    assert!(serialized[0].get("connection").is_none());
    let connection = sessions[0].connection().clone();
    let cancel = AtomicBool::new(false);
    let before = runtime
        .block_on(agent_editor::inspect(&connection, &cancel))
        .unwrap();
    assert_eq!(before.state_sha256, sessions[0].observation.state_sha256);
    assert_eq!(before.name, if actual {"OwnedSketch.ino"} else {"Sketch.ino"});
    assert!(before.source.contains("// unsaved user note\n") && before.dirty);
    assert_eq!(before.source.matches(if actual {"delay(250)"} else {"delay(1000)"}).count(), 1);
    let saved_path=if actual {root.join("sketchbook/OwnedSketch/OwnedSketch.ino")} else {root.join("Sketch/Sketch.ino")};
    let saved = std::fs::read(&saved_path).unwrap();
    let project_root = root.join(format!("project-{}",Uuid::new_v4()));
    std::fs::create_dir(&project_root).unwrap();
    let workflow =
        Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
    workflow
        .select_project(Project::create(&project_root).unwrap())
        .unwrap();
    let host = HostTools::new(workflow.clone());
    let detection=root.join("host-data");std::fs::create_dir_all(detection.join("program-locations")).unwrap();
    let catalog=if actual {serde_json::json!({"agent":[root.join("runtime")]})}else{
        let cli=PathBuf::from(std::env::var_os("HAZLOMEXA_NATIVE_EDITOR_CLI").expect("Provide the identified CLI for the owned project-copy check"));
        serde_json::json!({"arduino":[cli.parent().unwrap()]})
    };
    std::fs::write(detection.join("program-locations/portable-folders.json"),serde_json::to_vec(&catalog).unwrap()).unwrap();
    host.configure_native_discovery(root.join("resources"),detection).unwrap();
    let listed=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    if actual {
        assert_eq!(listed["observation"]["state_sha256"],before.state_sha256);
    } else {
        assert_eq!(listed["selection_required"],true);
        assert_eq!(listed["documents"].as_array().unwrap().len(),2);
        assert!(listed.get("observation").is_none());
        assert!(!listed.to_string().contains("delay("));
    }
    let args=serde_json::json!({"instance_id":before.instance_id});
    let read=host.call("inspect_open_agent_documents",args.clone()).unwrap();
    assert_eq!(read["observation"],serde_json::to_value(&before).unwrap());
    assert_eq!(read["mutations_allowed"],false);
    {
        let context=host.call("inspect_open_agent_documents",serde_json::json!({"instance_id":before.instance_id,"include_open_buffers":true})).unwrap();
        assert_eq!(context["observation"],read["observation"]);
        assert_eq!(context["open_buffers"]["complete_project"],false);
        assert_eq!(context["open_buffers"]["scope"],"native_open_buffers_only");
        let buffers=context["open_buffers"]["buffers"].as_array().unwrap();
        assert!(buffers.len()>=2);
        assert_eq!(buffers.iter().find(|buffer|buffer["relative_path"]=="Helper.h").unwrap()["source"],"// unsaved header stays local\n#define DELAY 500\n");
        assert!(context["open_buffers"].get("root").is_none());
        for private in ["\"token\":", "\"pipe\":",root.to_str().unwrap()] {assert!(!context.to_string().contains(private));}
        assert_eq!(std::fs::read(saved_path.parent().unwrap().join("Helper.h")).unwrap(),b"// saved header is stale\n");
    }
    for private in ["\"token\":", "\"pipe\":",root.to_str().unwrap()] {
        assert!(!read.to_string().contains(private));
    }
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"instance_id":Uuid::new_v4()})).is_err());
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"path":"Sketch.ino"})).is_err());
    if let Some(other)=sessions.iter().find(|s|s.observation.instance_id!=before.instance_id) {
        std::fs::remove_file(discovery.join(format!("{}.json",other.observation.instance_id))).unwrap();
    }
    let automatic=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    assert_eq!(automatic["observation"],read["observation"]);
    let mut current = before.clone();
    let mut copies=Vec::new();
    let changes=if actual {[("delay(250)","delay(200)"),("delay(200)","delay(100)")]}
        else {[("delay(1000)","delay(500)"),("delay(500)","delay(250)")]};
    for (old, new) in changes {
        let source = current.source.replace(old, new);
        let request=serde_json::json!({"instance_id":current.instance_id,"document_token":current.document_token,
            "version":current.version,"state_sha256":current.state_sha256,"agreement":{"kind":"software_operation","outcome":"Apply the explicitly requested owned fixture operation","assumptions":["Physical function is not verified"],"constraints":["Preserve unrelated source and manual edits"],"losses":[],"verification":"Check the native report and final state; independent reopen remains unverified"},"description":"Change timing in the owned sketch","source":source});
        let prepared=host.call("propose_open_agent_change",request.clone()).unwrap();
        let pending=workflow.view().unwrap().job.unwrap();
        let job_id=pending.snapshot.input.job_id();
        assert_eq!(prepared["job_id"],job_id.to_string());
        assert_eq!(prepared["human_approval_required"],true);
        assert_eq!(runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap().source,current.source);
        let approval = pending.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        let report = workflow
            .artifact(job_id, ArtifactFormat::LiveAgentReport)
            .unwrap()
            .read_text()
            .unwrap();
        let result: serde_json::Value = serde_json::from_str(&report).unwrap();
        let verified = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":job_id}),
            )
            .unwrap();
        assert_eq!(verified["format"], "live_agent_report");
        assert_eq!(verified["checks"]["native_buffer_checkpoint"]["scope"],"historical_native_open_buffers_checkpoint");
        assert!(verified["checks"]["native_buffer_checkpoint"]["buffer_count"].as_u64().unwrap()>=2);
        assert_eq!(verified["checks"]["native_buffer_checkpoint"]["complete_project"],false);
        assert_eq!(verified["checks"]["native_buffer_checkpoint"]["working_version_reopen_checked"],false);
        assert_eq!(verified["scope"],"verified_native_working_version_report");
        assert_eq!(verified["checks"]["native_working_version"]["native_reader_reopened"],true);
        assert_eq!(verified["checks"]["native_working_version"]["external_dependencies_checked"],false);
        let version=workflow.artifact(job_id,ArtifactFormat::NativeWorkingVersion).unwrap();
        assert!(version.path.is_dir());
        assert_eq!(std::fs::read_to_string(version.path.join(if actual{"OwnedSketch.ino"}else{"Sketch.ino"})).unwrap(),source);
        assert_eq!(std::fs::read_to_string(version.path.join("Helper.h")).unwrap(),"// unsaved header stays local\n#define DELAY 500\n");
        if !actual {
            assert_eq!(std::fs::read(version.path.join("Empty.h")).unwrap(),b"");
            assert!(version.path.join("empty-folder").is_dir());
            assert_eq!(std::fs::read(version.path.join("data/reference.bin")).unwrap(),[0,1,0,255]);
            assert_eq!(std::fs::read(version.path.join("build/kept.bin")).unwrap(),[7,0,8]);
            assert_eq!(std::fs::read_to_string(version.path.join("Closed.cpp")).unwrap(),if old=="delay(1000)"{"// initial closed source\n"}else{"// later closed manual change\n"});
            if old=="delay(1000)"{std::fs::write(saved_path.parent().unwrap().join("Closed.cpp"),b"// later closed manual change\n").unwrap();}
        }
        let published=workflow.current().unwrap().unwrap();
        // Use the artifact's actual directory; workspace storage paths are host-owned.
        let checkpoint=workflow.artifact(job_id,ArtifactFormat::LiveAgentReport).unwrap().path.parent().unwrap().join("agent-buffers.json");
        let retained:serde_json::Value=serde_json::from_slice(&std::fs::read(&checkpoint).unwrap()).unwrap();
        copies.push((job_id,version.path.clone(),checkpoint.parent().unwrap().to_owned()));
        assert!(retained.to_string().contains("unsaved header stays local"));
        assert!(matches!(&published.snapshot.input,hazlomexa::project::OperationInput::LiveAgent(input) if input.buffer_checkpoint.is_some()));
        assert!(!verified
            .to_string()
            .contains(&root.to_string_lossy().to_string()));
        current = runtime
            .block_on(agent_editor::inspect(&connection, &cancel))
            .unwrap();
        assert_eq!(current.source, source);
        let live=host.call("inspect_open_agent_documents",args.clone()).unwrap();
        assert_eq!(live["observation"]["source"],source);
        assert_eq!(live["observation"]["state_sha256"],current.state_sha256);
        assert_eq!(result["after"]["state_sha256"], current.state_sha256);
        assert_eq!(std::fs::read(&saved_path).unwrap(), saved);
        assert!(host.call("propose_open_agent_change",request).is_err());
    }
    assert_eq!(
        runtime
            .block_on(agent_editor::apply(
                &connection,
                Uuid::new_v4(),
                &before,
                &before.source.replace(changes[0].0, changes[0].1),
                &cancel
            ))
            .unwrap_err(),
        Failure::Refused
    );
    let final_state = current;
    assert!(final_state.source.contains("// unsaved user note\n") && final_state.dirty);
    assert_eq!(std::fs::read(&saved_path).unwrap(),saved);
    let saving=std::env::var("HAZLOMEXA_NATIVE_SAVE_TEST").as_deref()==Ok("1");
    if saving {
        let request=serde_json::json!({"instance_id":final_state.instance_id,"document_token":final_state.document_token,
            "version":final_state.version,"state_sha256":final_state.state_sha256,"agreement":{"kind":"software_operation","outcome":"Apply the explicitly requested owned fixture operation","assumptions":["Physical function is not verified"],"constraints":["Preserve unrelated source and manual edits"],"losses":[],"verification":"Check the native report and final state; independent reopen remains unverified"},"description":"Save the reviewed current sketch"});
        let reply=host.call("propose_open_agent_save",request.clone()).unwrap();
        assert_eq!(reply["human_approval_required"],true);
        let pending=workflow.view().unwrap().job.unwrap();
        assert_eq!(std::fs::read(&saved_path).unwrap(),saved,"Preparing save must not write");
        let outcome=workflow.execute(workflow.begin(pending.approval_id.unwrap()).unwrap()).unwrap();
        assert_eq!(outcome.snapshot.phase,Phase::Completed,"{:?}",outcome.error);
        let report=host.call("read_verified_result",serde_json::json!({"job_id":outcome.snapshot.input.job_id()})).unwrap();
        assert_eq!(report["format"],"live_agent_report");
        let after=runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap();
        assert_eq!(after.source,final_state.source);assert_eq!(after.version,final_state.version);assert!(!after.dirty);
        assert_eq!(std::fs::read(&saved_path).unwrap(),final_state.source.as_bytes());
        assert!(host.call("propose_open_agent_save",request).is_err(),"A clean buffer must not prepare a second save");
    }
    let cancelling=std::env::var("HAZLOMEXA_NATIVE_CANCEL_TEST").as_deref()==Ok("1");
    if cancelling {
        assert!(!actual && !saving,"Delayed delivery is a controlled IDE fixture only");
        let source=final_state.source.replace("delay(250)","delay(125)");
        assert_ne!(source,final_state.source);
        let request=serde_json::json!({"instance_id":final_state.instance_id,"document_token":final_state.document_token,
            "version":final_state.version,"state_sha256":final_state.state_sha256,"agreement":{"kind":"software_operation","outcome":"Apply the explicitly requested owned fixture operation","assumptions":["Physical function is not verified"],"constraints":["Preserve unrelated source and manual edits"],"losses":[],"verification":"Check the native report and final state; independent reopen remains unverified"},"description":"Owned delayed edit","source":source});
        host.call("propose_open_agent_change",request.clone()).unwrap();
        let pending=workflow.view().unwrap().job.unwrap();
        let job_id=pending.snapshot.input.job_id();
        let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();
        let executing=workflow.clone();
        let worker=std::thread::spawn(move||executing.execute(dispatch).unwrap());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        while !root.join("edit-delivered").is_file() {
            assert!(std::time::Instant::now()<deadline,"Native delivery not observed");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap().source,final_state.source);
        assert_eq!(workflow.cancel(job_id).unwrap().snapshot.phase,Phase::Cancelled);
        let outcome=worker.join().unwrap();
        assert_eq!(outcome.snapshot.phase,Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(outcome.error.as_deref().unwrap().contains("Cancelar no deshace"));
        assert!(workflow.begin(approval).is_err());
        assert!(workflow.artifact(job_id,ArtifactFormat::LiveAgentReport).is_err());
        assert!(host.call("read_verified_result",serde_json::json!({"job_id":job_id})).is_err());
        std::fs::write(root.join("release-edit"),b"owned").unwrap();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        let after=loop {
            let observed=runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap();
            if observed.source==source {break observed;}
            assert!(std::time::Instant::now()<deadline,"Late edit not observed");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(after.instance_id,final_state.instance_id);
        assert_eq!(after.document_token,final_state.document_token);
        assert_eq!(after.version,final_state.version+1);
        assert!(after.dirty && after.source.contains("// unsaved user note"));
        let read=host.call("inspect_open_agent_documents",args).unwrap();
        assert_eq!(read["observation"],serde_json::to_value(&after).unwrap());
        assert!(host.call("propose_open_agent_change",request).is_err());
        assert_eq!(std::fs::read(&saved_path).unwrap(),saved);
        assert_eq!(workflow.view().unwrap().job.unwrap().snapshot.phase,Phase::Cancelled);
        assert!(workflow.artifact(job_id,ArtifactFormat::LiveAgentReport).is_err());
        assert!(host.call("read_verified_result",serde_json::json!({"job_id":job_id})).is_err());
        println!("late native edit after cancellation passed");
    }
    if !actual {
        std::fs::remove_file(discovery.join(format!("{}.json",before.instance_id))).unwrap();
        let disconnected=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
        assert!(disconnected["documents"].as_array().unwrap().is_empty());
        assert!(disconnected.get("observation").is_none());
        let (job_id,copy,directory)=&copies[0];
        let journal=directory.join("live-dispatch.json");
        let mut record:serde_json::Value=serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
        record["outcome"]=serde_json::json!("uncertain");
        record.as_object_mut().unwrap().remove("receipt_sha256");
        record.as_object_mut().unwrap().remove("agent_project_check_sha256");
        std::fs::write(&journal,serde_json::to_vec(&record).unwrap()).unwrap();
        let offline=root.join("offline-original");std::fs::rename(saved_path.parent().unwrap(),&offline).unwrap();
        let recovered=host.call("recover_native_result",serde_json::json!({"job_id":job_id})).unwrap();
        assert_eq!(recovered["published"],true);
        std::fs::rename(&offline,saved_path.parent().unwrap()).unwrap();
        assert_eq!(std::fs::read_dir(directory).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("SketchVersion-")).count(),1,"Recovery recaptured today's source");
        let closed=copy.join("Closed.cpp");let original=std::fs::read(&closed).unwrap();std::fs::write(&closed,b"// later edit to the preserved copy\n").unwrap();
        assert!(host.call("recover_native_result",serde_json::json!({"job_id":job_id})).is_err(),"A modified copy was silently replaced");
        assert_eq!(std::fs::read(&closed).unwrap(),b"// later edit to the preserved copy\n");std::fs::write(&closed,original).unwrap();
    }
    drop(host);
    drop(workflow);

    let resume_check=std::env::var("HAZLOMEXA_TEST_ARDUINO_CHECK_RESUME").as_deref()==Ok("1");
    if resume_check{assert!(actual && !saving && !cancelling);check_retained_copy(&root,&canonical,&project_root,&saved_path,copies[0].0);}

    let recovered = Project::open(&project_root).unwrap().recover().unwrap();
    assert_eq!(recovered.len(), if saving || cancelling {3} else {2}+usize::from(resume_check));
    assert_eq!(recovered.iter().filter(|run|run.phase==Phase::Cancelled).count(),usize::from(cancelling));
    assert!(recovered
        .iter()
        .all(|run| matches!(run.phase,Phase::Completed|Phase::Cancelled) && (run.schema_version == 15 || resume_check && run.schema_version==19)));
    println!("Rust host and companion protocol: edit/readback/follow-up/stale refusal passed");
}

fn check_retained_copy(root:&std::path::Path,canonical:&std::path::Path,project_root:&std::path::Path,saved_path:&std::path::Path,target:Uuid){
        let project=Project::open(project_root).unwrap();let published=project.artifact_directory(target);let stage=project_root.join(".hazlomexa/staging").join(target.to_string());let directory=if published.exists(){published.clone()}else{stage.clone()};
        let record_path=directory.join("live-dispatch.json");let mut record:serde_json::Value=serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
        record["outcome"]=serde_json::json!("uncertain");record.as_object_mut().unwrap().remove("receipt_sha256");record.as_object_mut().unwrap().remove("agent_project_check_sha256");
        std::fs::write(&record_path,serde_json::to_vec(&record).unwrap()).unwrap();
        let version=directory.join(record["agent_project_capture"]["folder"].as_str().unwrap());let capture=std::fs::read(version.join("capture.json")).unwrap();
        let old_ticket=std::fs::read_dir(&version).unwrap().filter_map(Result::ok).find(|e|e.file_name().to_string_lossy().starts_with("readback-")).unwrap().file_name();let old_bytes=std::fs::read(version.join(&old_ticket)).unwrap();
        let partial=version.join("preserved-partial.native");if partial.exists(){assert_eq!(std::fs::read(&partial).unwrap(),b"preserve previous check output");}else{std::fs::write(&partial,b"preserve previous check output").unwrap();}
        let mut snapshot=project.history().unwrap().into_iter().find(|s|s.input.job_id()==target).unwrap();let hazlomexa::project::OperationInput::LiveAgent(original)=&mut snapshot.input else{panic!("Expected retained Arduino result")};original.receipt_sha256=None;snapshot.phase=Phase::Cancelled;snapshot.audit.as_mut().unwrap().files_verified=false;project.save(&snapshot).unwrap();
        if directory!=stage{assert!(directory.canonicalize().unwrap().starts_with(canonical)&&stage.parent().unwrap().canonicalize().unwrap().starts_with(canonical)&&!stage.exists());std::fs::rename(&directory,&stage).unwrap();}drop(project);
        let resume=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("check-resume-profiles")).unwrap());resume.select_project(Project::open(project_root).unwrap()).unwrap();let resume_host=HostTools::new(resume.clone());
        assert!(resume.kicad_runtime().unwrap().is_none());assert!(resume_host.call("recover_native_result",serde_json::json!({"job_id":target})).is_err());
        let proposed=resume_host.call("propose_result_checks",serde_json::json!({"job_id":target,"agreement":{"kind":"software_operation","outcome":"Check the same retained sketch project without repeating its edit","assumptions":["Physical behavior is unverified"],"constraints":["Preserve the original and retained copy"],"losses":[],"verification":"One unused native sketch-reader archive roundtrip within the original two-attempt bound"}})).unwrap();
        assert!(proposed.get("approval_id").is_none());let pending=resume.current().unwrap().unwrap();let resume_id=pending.snapshot.input.job_id();assert!(matches!(&pending.snapshot.input,hazlomexa::project::OperationInput::ResultChecks(i) if i.program==Some(hazlomexa::host_tools::Program::Agent) && i.attempts_before==1));
        let offline=root.join("source-unavailable-during-resume");assert!(saved_path.parent().unwrap().canonicalize().unwrap().starts_with(canonical)&&offline.parent().unwrap().canonicalize().unwrap().as_path()==canonical&&!offline.exists());std::fs::rename(saved_path.parent().unwrap(),&offline).unwrap();
        let done=resume.execute(resume.begin(pending.approval_id.unwrap()).unwrap()).unwrap();assert!(offline.canonicalize().unwrap().starts_with(canonical)&&saved_path.parent().unwrap().parent().unwrap().canonicalize().unwrap().starts_with(canonical));std::fs::rename(&offline,saved_path.parent().unwrap()).unwrap();assert_eq!(done.snapshot.phase,Phase::Completed,"{:?}",done.error);
        assert!(resume.kicad_runtime().unwrap().is_none());let status=resume_host.call("recover_native_result",serde_json::json!({"job_id":resume_id})).unwrap();assert_eq!(status["published"],true);assert_eq!(status["checked_result_job_id"],serde_json::json!(target));
        let original=project_root.join(".hazlomexa/artifacts").join(target.to_string());let version=original.join(record["agent_project_capture"]["folder"].as_str().unwrap());assert_eq!(std::fs::read(version.join("capture.json")).unwrap(),capture);assert_eq!(std::fs::read(version.join(&old_ticket)).unwrap(),old_bytes);assert_eq!(std::fs::read(version.join("preserved-partial.native")).unwrap(),b"preserve previous check output");
        let tickets=std::fs::read_dir(&version).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("readback-")).collect::<Vec<_>>();assert_eq!(tickets.len(),2);assert_eq!(tickets.iter().filter(|e|serde_json::from_slice::<serde_json::Value>(&std::fs::read(e.path()).unwrap()).unwrap()["approval"]["job_id"]==serde_json::json!(resume_id)).count(),1);
        let receipt=resume.artifact(resume_id,ArtifactFormat::ResultChecksReport).unwrap();let path=receipt.path.clone();let bytes=std::fs::read(&path).unwrap();drop(receipt);drop(resume_host);drop(resume);
        // Model a confirmed reader result whose parent link/publication did not commit.
        let project=Project::open(project_root).unwrap();let parent=project.artifact_directory(target);let mut linked:serde_json::Value=serde_json::from_slice(&std::fs::read(parent.join("live-dispatch.json")).unwrap()).unwrap();linked["outcome"]=serde_json::json!("uncertain");linked.as_object_mut().unwrap().remove("receipt_sha256");linked.as_object_mut().unwrap().remove("agent_project_check_sha256");std::fs::write(parent.join("live-dispatch.json"),serde_json::to_vec(&linked).unwrap()).unwrap();
        for id in [target,resume_id]{let mut snapshot=project.history().unwrap().into_iter().find(|s|s.input.job_id()==id).unwrap();match &mut snapshot.input{hazlomexa::project::OperationInput::LiveAgent(i)=>i.receipt_sha256=None,hazlomexa::project::OperationInput::ResultChecks(i)=>i.receipt_sha256=None,_=>panic!("Unexpected resume proof")};snapshot.phase=Phase::Interrupted;snapshot.audit.as_mut().unwrap().files_verified=false;project.save(&snapshot).unwrap();let from=project.artifact_directory(id);let to=project_root.join(".hazlomexa/staging").join(id.to_string());assert!(from.canonicalize().unwrap().starts_with(canonical)&&to.parent().unwrap().canonicalize().unwrap().starts_with(canonical)&&!to.exists());std::fs::rename(from,to).unwrap();}drop(project);
        let repair=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("offline-link-repair-profiles")).unwrap());repair.select_project(Project::open(project_root).unwrap()).unwrap();let repair_host=HostTools::new(repair.clone());assert_eq!(repair_host.call("recover_native_result",serde_json::json!({"job_id":resume_id})).unwrap()["published"],true);assert_eq!(std::fs::read_dir(root.join("offline-link-repair-profiles")).unwrap().count(),0);drop(repair_host);drop(repair);
        let directory=project_root.join(".hazlomexa/artifacts").join(target.to_string()).join(record["agent_project_capture"]["folder"].as_str().unwrap());assert_eq!(std::fs::read_dir(directory).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("readback-")).count(),2);
        assert!(path.starts_with(root));std::fs::remove_file(&path).unwrap();let restored=Project::open(project_root).unwrap().recover().unwrap();assert_eq!(restored.iter().find(|s|s.input.job_id()==resume_id).unwrap().phase,Phase::Completed);assert_eq!(std::fs::read(path).unwrap(),bytes);
        std::fs::write(root.join("arduino-check-resume.json"),serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"fresh_human_approval_fixture":true,"no_kicad_setup_required":true,"same_capture_retained":true,"old_attempt_preserved":true,"attempts":2,"new_attempt_owned_by_approval":true,"source_unavailable_during_check":true,"edit_repeated":false,"report_recovered_offline":true,"lost_parent_link_repaired_without_reader":true,"real_provider":false,"visual_approval":false,"physical_function_checked":false})).unwrap()).unwrap();
}
