//! Real native tools, isolated optional pySerial setup, and desktop approval/readback routing.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::{HostTools, McpTools, Program},
        jobs::Workflow,
        native_setup::{self, Setup},
        project::{Phase, Project},
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        3,
        "Pass owned directory, tshark.exe and USBPcapCMD.exe"
    );
    let root = &args[0];
    std::fs::create_dir_all(root)?;
    let data = root.join("data");
    std::fs::create_dir_all(data.join("program-locations"))?;
    std::fs::write(
        data.join("program-locations/portable-folders.json"),
        serde_json::to_vec(&json!({"tshark":[args[1].parent()],"usbpcap":[args[2].parent()]}))?,
    )?;
    for program in [Program::Tshark, Program::Usbpcap] {
        let installer = hazlomexa::diagnostic_downloads::prepare(
            program,
            &data,
            Arc::new(AtomicBool::new(false)),
        )
        .await?
        .unwrap();
        assert!(installer.is_file());
        assert_eq!(
            Some(installer),
            hazlomexa::diagnostic_downloads::prepare(
                program,
                &data,
                Arc::new(AtomicBool::new(false))
            )
            .await?
        );
    }
    hazlomexa::diagnostic_downloads::prepare(
        Program::Serial,
        &data,
        Arc::new(AtomicBool::new(false)),
    )
    .await?;
    let project = root.join("project");
    std::fs::create_dir_all(&project)?;
    // One owned Ethernet/IPv4/UDP packet, no network traffic is collected.
    let packet: Vec<u8> = vec![
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2, 0, 0, 0, 0, 1, 8, 0, 0x45, 0, 0, 32, 0, 1, 0, 0, 64,
        17, 0, 0, 192, 0, 2, 1, 192, 0, 2, 2, 0x13, 0x88, 0x13, 0x89, 0, 12, 0, 0, 0x54, 0x45,
        0x53, 0x54,
    ];
    let mut pcap = Vec::new();
    for n in [0xa1b2c3d4u32, 0x00040002, 0, 0, 65535, 1] {
        pcap.extend(n.to_le_bytes());
    }
    for n in [0u32, 0, packet.len() as u32, packet.len() as u32] {
        pcap.extend(n.to_le_bytes());
    }
    pcap.extend(packet);
    std::fs::write(project.join("owned.pcap"), pcap)?;
    let workflow = Arc::new(Workflow::new(
        root.join("unused.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(if project.join("hazlomx.json").exists() {
        Project::open(&project)?
    } else {
        Project::create(&project)?
    })?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let mut receipts = Vec::new();
    for program in [Program::Tshark, Program::Usbpcap, Program::Serial] {
        let setup = native_setup::discover(
            program,
            root,
            &project,
            &root.join(uuid::Uuid::new_v4().to_string()),
            &data.join("program-locations"),
        )?
        .ok_or("Automatic detection failed")?;
        let Setup::Cli(exe) = setup else {
            return Err("Expected native CLI/API connection".into());
        };
        let catalog = host.connect_native_cli(program, exe, Arc::new(AtomicBool::new(false)))?;
        let scoped =
            McpTools::for_program(host.clone(), workflow.view()?.project.unwrap(), program)?;
        assert_eq!(scoped.catalog().len(), 7);
        let readback = match program {
            Program::Tshark => {
                json!({"tool":"cli","arguments":{"args":["-r","owned.pcap","-n","-T","json"]}})
            }
            Program::Usbpcap => json!({"tool":"cli","arguments":{"args":["--extcap-version"]}}),
            _ => json!({"tool":"list_ports","arguments":{}}),
        };
        let request = json!({"connection_id":catalog.connection_id,"catalog_sha256":catalog.catalog_sha256,"description":"Inspect owned diagnostic evidence","agreement":{"kind":"software_operation","outcome":"Inspect only the owned diagnostic evidence","assumptions":[],"constraints":["No device commands or live capture"],"losses":[],"verification":"Check the native diagnostic response; this does not establish physical function"},"calls":[],"readback":readback});
        host.call("propose_native_tool_calls", request.clone())?;
        let job = workflow.view()?.job.unwrap();
        assert_eq!(job.snapshot.phase, Phase::AwaitingApproval);
        let approval = job.approval_id.unwrap();
        let completed = workflow.execute(workflow.begin(approval)?)?;
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        assert!(workflow.begin(approval).is_err());
        let report = host.call(
            "read_verified_result",
            json!({"job_id":completed.snapshot.input.job_id()}),
        )?;
        let parsed: Value = serde_json::from_str(report["content"].as_str().unwrap())?;
        let stdout = parsed["readback"]["stdout"]
            .as_str()
            .ok_or("Missing native stdout")?;
        match program {
            Program::Tshark => assert!(stdout.contains("udp") && stdout.contains("192.0.2.1")),
            Program::Usbpcap => assert!(stdout.contains("extcap") && stdout.contains("version=")),
            _ => {
                let ports: Value = serde_json::from_str(stdout)?;
                assert!(ports["ports"].is_array());
            }
        }
        if program == Program::Tshark {
            let name = format!("copy-{}.pcapng", uuid::Uuid::new_v4());
            let mut save = request.clone();
            save["agreement"]["outcome"]=json!("Create a new owned capture copy without replacing the original");
            save["readback"] =
                json!({"tool":"cli","arguments":{"args":["-r","owned.pcap","-w",name]}});
            host.call("propose_native_tool_calls", save.clone())?;
            let pending = workflow.view()?.job.unwrap();
            let saved = workflow.execute(workflow.begin(pending.approval_id.unwrap())?)?;
            assert_eq!(saved.snapshot.phase, Phase::Completed, "{:?}", saved.error);
            let verified = host.call(
                "read_verified_result",
                json!({"job_id":saved.snapshot.input.job_id()}),
            )?;
            let saved: Value = serde_json::from_str(verified["content"].as_str().unwrap())?;
            let before = std::fs::read(project.join(&name))?;
            assert_eq!(
                saved["readback"]["capture_output"]["sha256"],
                format!("{:x}", Sha256::digest(&before))
            );
            host.call("propose_native_tool_calls", save)?;
            let pending = workflow.view()?.job.unwrap();
            let refused = workflow.execute(workflow.begin(pending.approval_id.unwrap())?)?;
            assert_eq!(refused.snapshot.phase, Phase::Failed);
            assert_eq!(before, std::fs::read(project.join(name))?);
        }
        let mut repeated = request.clone();
        repeated["calls"] = json!([readback]);
        assert!(host.call("propose_native_tool_calls", repeated).is_err());
        host.call("propose_native_tool_calls", request)?;
        let cancelled = workflow.view()?.job.unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id())?;
        assert!(workflow.begin(cancelled.approval_id.unwrap()).is_err());
        receipts.push(json!({"program":program,"automatic_detection":true,"native_catalog":catalog.tools.len(),"approval_and_verified_readback":true,"cancel_before_approval":true,"repeated_capture_refused":true}));
        host.disconnect_native_tools()?;
    }
    std::fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(&receipts)?,
    )?;
    println!(
        "{}",
        json!({"native_diagnostic_routes":receipts,"physical_capture_tested":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows diagnostic test");
}
