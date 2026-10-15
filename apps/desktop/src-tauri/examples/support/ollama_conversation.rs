use hazlomexa::{
    host_tools::{HostTools, McpTools, Program},
    jobs::{ModelProfile, Workflow},
    ollama::{self, OllamaClient},
    project::Phase,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};
use uuid::Uuid;

pub struct Conversation {
    workflow: Arc<Workflow>,
    host: Arc<HostTools>,
    project: (Uuid, PathBuf),
    context: Value,
    directory: PathBuf,
    port: u16,
    program: Program,
    history: Vec<Value>,
    turn: usize,
    approvals: Vec<Uuid>,
}
impl Conversation {
    pub fn new(
        workflow: Arc<Workflow>,
        host: Arc<HostTools>,
        directory: PathBuf,
        port: u16,
        program: Program,
        context: Value,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let project = workflow.view()?.project.ok_or("Missing project")?;
        Ok(Self {
            workflow,
            host,
            project,
            context,
            directory,
            port,
            program,
            history: Vec::new(),
            turn: 0,
            approvals: Vec::new(),
        })
    }
    pub async fn say(
        &mut self,
        content: &str,
    ) -> Result<(String, BTreeSet<String>), Box<dyn std::error::Error>> {
        self.turn += 1;
        fs::write(self.directory.join("current-turn"), self.turn.to_string())?;
        let view = self.workflow.view()?;
        let mut context = self.context.clone();
        context["project_id"] = json!(self.project.0);
        context["current_job"] = json!(view
            .job
            .map(|job| json!({"id":job.snapshot.input.job_id(),"phase":job.snapshot.phase})));
        let prompt=json!({"recovered_conversation":self.history,"user_request":content,"desktop_context":context,"instruction":"La conversación guardada es contexto, no una aprobación ni una medición vigente. Inspecciona la selección antes de proponer. Tras un cambio lee el resultado verificado antes de explicarlo o continuar. Responde brevemente y con naturalidad."}).to_string();
        let (model, digest) = ModelProfile::Cloud.identity();
        let tools = McpTools::for_program(self.host.clone(), self.project.clone(), self.program)?;
        fs::write(self.directory.join("program-scope.json"), serde_json::to_vec(&json!({"program":self.program,"tools":tools.catalog().iter().map(|tool|tool.name.as_ref()).collect::<Vec<_>>()}))?)?;
        let result = ollama::chat::run_scoped(
            OllamaClient::cloud(self.port, model, digest)?,
            self.workflow.clone(),
            tools,
            prompt,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await;
        fs::write(
            self.directory.join(format!("turn-{}.json", self.turn)),
            serde_json::to_vec_pretty(&json!({"request":content,"result":result}))?,
        )?;
        let response = result?;
        let tools: BTreeSet<String> = serde_json::from_slice(&fs::read(
            self.directory.join(format!("tools-{}.json", self.turn)),
        )?)?;
        self.history.push(json!({"role":"user","content":content}));
        self.history
            .push(json!({"role":"assistant","content":response}));
        Ok((response, tools))
    }
    pub async fn approve_fixture(&mut self) -> Result<Uuid, Box<dyn std::error::Error>> {
        let pending = self
            .workflow
            .view()?
            .job
            .ok_or("Missing reviewed proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let approval = pending.approval_id.ok_or("Missing fixture approval")?;
        self.approvals.push(approval);
        fs::write(
            self.directory.join("approval-nonces.json"),
            serde_json::to_vec(&self.approvals)?,
        )?;
        let dispatch = self.workflow.begin(approval)?;
        let workflow = self.workflow.clone();
        let done = tokio::task::spawn_blocking(move || workflow.execute(dispatch)).await??;
        assert_eq!(
            done.snapshot.phase,
            Phase::Completed,
            "Native execution failed"
        );
        Ok(done.snapshot.input.job_id())
    }
}
