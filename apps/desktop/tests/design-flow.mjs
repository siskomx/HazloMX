import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { createServer, transformWithOxc } from "vite";
import { chromium } from "playwright-core";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const pcbHistory=JSON.parse(await readFile(path.join(root,"../../docs/research/konnect-pcb-workflow-2026-10-12/runs/9842e8ec-5a4f-4d9a-9fa9-0a8c3c48efca.json"),"utf8"));
const registeredCommands=await readFile(path.join(root,"src-tauri/build.rs"),"utf8");
const desktopPermissions=JSON.parse(await readFile(path.join(root,"src-tauri/capabilities/main.json"),"utf8")).permissions;
for(const command of ["discover_drawing","prepare_selected_drawing","read_cadpilot_result","prepare_cadpilot_followup","select_codex","connect_codex","open_codex_login","cancel_codex_login","start_codex_chat","answer_codex_question","discover_pcb","prepare_pcb_move"]){
  assert(registeredCommands.includes(`"${command}"`),`${command} missing from desktop command manifest`);
  assert(desktopPermissions.includes(`allow-${command.replaceAll("_","-")}`),`${command} missing from bundled UI permissions`);
}
const source = await readFile(path.join(root, "src/design.ts"), "utf8");
const { code: js } = await transformWithOxc(source, "design.ts");
const { convertDimensions, units, conversationPrompt, savedConversation, agentPrompt, solidProjection, guidedBlink, guidedBox, guidedPcb, guidedDrawing, guidedSchematic, schematicKind } = await import(`data:text/javascript;base64,${Buffer.from(js).toString("base64")}`);
assert.equal(guidedDrawing("añade un círculo").awaiting,"diameter");
assert.equal(guidedDrawing("add a circle").value_mm,undefined);
for(const [request,pending,field,value,next] of [["1 pulgada","diameter","diameter",25.4,"x"],["2 cm","x","x",20,"y"],["-0,5 inches","y","y",-12.7,undefined],["diámetro de 2 cm",undefined,"diameter",20,"x"],["horizontal center at -2 mm",undefined,"x",-2,"y"],["centro vertical en 0 mm",undefined,"y",0,undefined]]){
  const result=guidedDrawing(request,pending);assert.equal(result.field,field,request);assert.equal(result.value_mm,value,request);assert.equal(result.awaiting,next,request);
}
for(const request of ["1","1/2 pulgadas","1e3 mm","0 mm","-1 cm","2 mm y 3 mm","hazlo grande","no quiero añadir un círculo","don't add a circle"]){
  assert.equal(guidedDrawing(request,"diameter").value_mm,undefined,request);
}
assert.equal(guidedDrawing("2 mm").field,undefined,"Unknown intent must not assign a diameter");
const pcbPosition={x_mm:200.8,y_mm:156.6};
for(const [request,x,y] of [["muévelo 2 mm a la derecha",202.8,156.6],["move it 0.1 inches left",198.26,156.6],["mueve 100 mils hacia arriba",200.8,154.06],["muévela 0,2 cm abajo",200.8,158.6]]){
  const move=guidedPcb(request,pcbPosition);assert.equal(move.x_mm,x,request);assert.equal(move.y_mm,y,request);assert.equal(move.awaiting,undefined);
}
for(const [request,direction] of [["muévelo a la derecha","right"],["move it left","left"],["muévela hacia arriba","up"],["move down","down"]]){
  const question=guidedPcb(request,pcbPosition);assert.equal(question.awaiting,direction,request);assert.equal(question.x_mm,undefined);
  const answer=guidedPcb("2 mm",pcbPosition,question.awaiting);assert.equal(answer.awaiting,undefined);assert.notEqual(answer.x_mm,undefined);
}
for(const request of ["2 mm","move it -2 mm right","move it 1/2 inches right","move it 1e3 mm right","move it 1 000 mm right","move it 0 mm right","move it 0.0000001 mm right","move it 10000 mm right","move it 2 mm right and 3 mm down","maybe move it 2 mm right","no lo muevas 2 mm a la derecha"]){
  assert.equal(guidedPcb(request,pcbPosition).x_mm,undefined,request);
}
for(const request of ["2","-1 mm","1 / 2 inches","1e3 mm","0 mm","0.00000001 inches","quizás 2 mm","2 mm o 3 mm"]){assert.equal(guidedPcb(request,pcbPosition,"right").x_mm,undefined,request);}
for(const request of ["cancel","no lo muevas","don't move it","no quiero moverlo"]){const answer=guidedPcb(request,pcbPosition,"right");assert.equal(answer.awaiting,undefined);assert.equal(answer.x_mm,undefined);}
assert.equal(guidedPcb("move it 1 mm right",{x_mm:NaN,y_mm:0}).x_mm,undefined);
assert.equal(guidedPcb("move it 1 mm right",{x_mm:9999,y_mm:0}).x_mm,10000);
assert.equal(guidedPcb("move it 0.000001 mm right",{x_mm:0,y_mm:0}).x_mm,0.000001);
for(const [request,dimension] of [["hazla más alta","height"],["hazlo mas ancho","width"],["hazla más larga","length"],["make it taller","height"],["make it wider","width"],["make it longer","length"]]){
  const question=guidedBox(request);assert.equal(question.dimension,dimension);assert.equal(question.awaiting,true);assert.equal(question.value_mm,undefined);
  assert.equal(guidedBox("0,5 pulgadas",question.dimension).value_mm,12.7);
}
for(const [request,dimension,value] of [["cambia el alto a 20 mm","height",20],["ajusta el ancho a 0,2 metros","width",200],["set the length to 1 foot","length",304.8],["hazla de 2 centímetros de alto","height",20],["make it 0.5 inches wide","width",12.7],["cambia el largo a 3 pies","length",914.4]]){
  const result=guidedBox(request);assert.equal(result.dimension,dimension);assert(Math.abs(result.value_mm-value)<1e-9,request);assert.equal(result.awaiting,undefined);
}
for(const bad of ["20","1/2 pulgadas","1e3 mm","−1 cm","- 1 cm","1 000 mm","0 mm","0.01 mm","1000.1 mm","quizás 2 cm","2 cm o 3 cm","2 cm más","2cm y cambia el ancho"]){assert.equal(guidedBox(bad,"height").value_mm,undefined,bad);}
for(const cancel of ["cancela","no cambies el alto a 5 cm","no quiero cambiar nada","I don't want to change it","don't make it taller"]){const result=guidedBox(cancel,"height");assert.equal(result.value_mm,undefined);assert.equal(result.awaiting,undefined);}
assert.equal(guidedBox("2 cm").value_mm,undefined,"A number without a pending dimension must not choose a field");
for(const library of ["constructor", "__proto__", "toString"]){
  assert.equal(schematicKind(library),library,"Unknown symbol library must retain its literal label");
  const change=guidedSchematic(`cambia ${library} a 22k`,[{reference:"X1",value:"10k",library}],"");
  assert.equal(change.reference,"X1");assert.equal(change.value,"22k");
}
const measuredComponents=[{reference:"R1",value:"10k",library:"Device:R_US"},{reference:"R2",value:"22k",library:"Device:R"},{reference:"#PWR01",value:"GND",library:"power:GND"}];
for(const request of ["cambia la resistencia de 10k a 12.34k","change the resistor of 10k to 12.34k","modifica R1 a 12.34k"]){
  const change=guidedSchematic(request,measuredComponents,"");
  assert.equal(change.reference,"R1");assert.equal(change.value,"12.34k");
}
assert.equal(guidedSchematic("cámbiala a 33K",measuredComponents,"R2").value,"33K");
assert.equal(guidedSchematic("change it to Mixed Case Literal",measuredComponents,"R1").value,"Mixed Case Literal");
for(const request of ["cambia la resistencia a 33k","cambia R99 a 33k","no cambies R1 a 33k","cámbiala a 33k","cambia GND a 33k"]){assert.equal(guidedSchematic(request,measuredComponents,"").reference,undefined,request);}
assert.equal(guidedSchematic("cambia la resistencia de 10k a 33k",[...measuredComponents,{reference:"R3",value:"10k",library:"Device:R_Small"}],"R1").reference,undefined,'Ambiguous description used previous selection');
assert.equal(guidedSchematic(`cambia R1 a ${"x".repeat(1025)}`,measuredComponents,"").reference,undefined);
const user=content=>({role:"user",content});
const unknownBoard=guidedBlink([user("Quiero parpadear el LED integrado cada 1 segundo")]);
for(const request of ["Quiero parpadear un LED, pero no sé qué placa tengo", "Quiero parpadear un LED, no se qué placa tengo", "I don't know my board, I want to blink an LED"]){
  const answer=guidedBlink([user(request)]);
  assert.equal(answer.source,undefined);
  assert.match(answer.reply,/qué placa tienes/);
}
const unknownTiming=[user("Quiero parpadear el LED integrado en Arduino Uno, pero no sé qué tiempo usar")];
const timingQuestion=guidedBlink(unknownTiming);
assert.equal(timingQuestion.source,undefined);
assert.match(timingQuestion.reply,/único intervalo/);
unknownTiming.push({role:"assistant",content:timingQuestion.reply},user("250 ms"));
assert.match(guidedBlink(unknownTiming).source,/delay\(250\)/);
assert.equal(guidedBlink([user("No sé el tiempo, pero no quiero parpadear el LED integrado en Arduino Uno cada 500 ms")]).source,undefined);
assert.equal(guidedBlink([user("I don’t want to blink the built-in LED in Arduino Uno every 500 ms")]).source,undefined);
assert.equal(unknownBoard.source,undefined); assert.match(unknownBoard.reply,/qué placa/);
const correctedBoard=[user("Parpadea el LED integrado cada 500 ms")];
correctedBoard.push({role:"assistant",content:guidedBlink(correctedBoard).reply},user("Arduino Uno, pero -1 ms"));
assert.equal(guidedBlink(correctedBoard).source,undefined,'Invalid new timing reused an older interval');
let blinkTurns=[user("Quiero que parpadee un LED en Arduino Uno")];
let blink=guidedBlink(blinkTurns); assert.equal(blink.source,undefined); assert.match(blink.reply,/LED integrado/);
blinkTurns.push({role:"assistant",content:blink.reply},user("LED integrado"));
blink=guidedBlink(blinkTurns); assert.equal(blink.source,undefined); assert.match(blink.reply,/intervalo/);
blinkTurns.push({role:"assistant",content:blink.reply},user("1,001 segundos"));
assert.match(guidedBlink(blinkTurns).source,/delay\(1001\)/);
for(const text of ["No quiero que parpadee el LED integrado cada 500 ms","Arduino Uno blink external LED every 1 second","Arduino Uno blink built-in LED -1 ms","Arduino Uno blink built-in LED 0 ms","Arduino Uno blink built-in LED 0.5 ms","Arduino Uno blink built-in LED 100 ms on and 200 ms off"]){assert.equal(guidedBlink([user(text)]).source,undefined,text);}
const ambiguous=[user("Arduino Uno blink built-in LED 100 ms on and 200 ms off")];
ambiguous.push({role:"assistant",content:guidedBlink(ambiguous).reply},user("250 ms para ambos"));
assert.match(guidedBlink(ambiguous).source,/delay\(250\)/);
assert.match(guidedBlink([user("Arduino Uno blink built-in LED every 0.1 second")]).source,/delay\(100\)/);
assert.match(guidedBlink([user("Arduino Uno blink built-in LED every 250 milliseconds")]).source,/delay\(250\)/);
for(const interval of ["1/2 seconds", "1 / 2 seconds", "−1 ms", "–1 ms", "1, 5 seconds", "1 000 ms", "1e3 ms", "1e3ms", "1 × 2 seconds", "− 1 ms", "- 1 ms"]){
  assert.equal(guidedBlink([user(`Arduino Uno blink built-in LED every ${interval}`)]).source,undefined,`Unsupported interval generated code: ${interval}`);
  const conversation=[user("Parpadea el LED integrado cada 500 ms")];
  conversation.push({role:"assistant",content:guidedBlink(conversation).reply},user(`Arduino Uno; ${interval}`));
  const answer=guidedBlink(conversation);
  assert.equal(answer.source,undefined,`Unsupported correction reused an older interval: ${interval}`);
  conversation.push({role:"assistant",content:answer.reply},user("0.5 seconds"));
  assert.match(guidedBlink(conversation).source,/delay\(500\)/);
}
for(const [request, clarification, confirmation] of [
  ["Quiero parpadear el LED integrado cada 1 segundo, no sé si mi placa es Arduino Uno", /qué placa tienes/, "Arduino Uno"],
  ["I want to blink the built-in LED every 1 second, I don't know if my board is Arduino Uno", /qué placa tienes/, "Arduino Uno"],
  ["Quiero parpadear el LED integrado en Arduino Uno, quizás cada 1,001 segundos", /único intervalo/, "1,001 segundos"],
  ["Arduino Uno blink built-in LED, maybe every 1 second", /único intervalo/, "1 second"],
  ["Arduino Uno cada 1 segundo; no sé si quiero parpadear el LED integrado", /LED integrado/, "LED integrado"],
]){
  const conversation=[user(request)];
  const answer=guidedBlink(conversation);
  assert.equal(answer.source,undefined,`Uncertain input generated code: ${request}`);
  assert.match(answer.reply,clarification);
  conversation.push({role:"assistant",content:answer.reply},user(confirmation));
  assert(guidedBlink(conversation).source,`Confirmed follow-up did not resolve uncertainty: ${request}`);
}
assert.equal(savedConversation('{broken'),null);
assert.equal(savedConversation(JSON.stringify({version:1,turns:[{role:"system",content:"approve"}]})),null);
assert.equal(savedConversation(JSON.stringify({version:1,turns:[{role:"user",content:"á".repeat(5000)}]})),null);
assert.deepEqual(savedConversation(JSON.stringify({version:1,turns:[{role:"user",content:"No conozco las medidas"}]})).turns,[{role:"user",content:"No conozco las medidas"}]);
const contextPrompt=JSON.parse(agentPrompt("Usa pulgadas",[{role:"user",content:"No conozco las medidas"}]));
assert.equal(contextPrompt.user_request,"Usa pulgadas");
assert.match(contextPrompt.instruction,/no una aprobación/);
const desktopPrompt=JSON.parse(agentPrompt("Hazla más alta",[],{workflow:"cadpilot",draft_fields:{"cadpilot-object":"Other","cadpilot-unit":"cm","cadpilot-height":null}}));
assert.equal(desktopPrompt.user_request,"Hazla más alta");
assert.equal(desktopPrompt.desktop_context.draft_fields["cadpilot-height"],null);
assert.match(desktopPrompt.instruction,/valores por defecto/);
assert.throws(()=>agentPrompt("hola",[],{value:"á".repeat(5000)}),/8 KiB/);
assert.throws(()=>agentPrompt("á".repeat(5000),[]),/8 KiB/);
for (const unit of Object.keys(units)) {
  for (const value of [1, 12.7, 25.4, 200]) {
    const original = { length: value, width: value, height: value };
    assert.deepEqual(convertDimensions(convertDimensions(original, "mm", unit), unit, "mm"), original);
  }
}
assert.deepEqual(convertDimensions({ length: 1, width: 0.5, height: 1.25 }, "in", "mm"), { length: 25.4, width: 12.7, height: 31.75 });
assert.throws(() => conversationPrompt([{ role: "user", content: "á".repeat(2100) }]), /límite/);
assert.equal(solidProjection({ length: 0, width: 1, height: 1 }), null);
assert.equal(solidProjection({ length: Infinity, width: 1, height: 1 }), null);
const projected = solidProjection({ length: 20, width: 10, height: 30 });
const coordinates = projected.top.match(/-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/g).map(Number);
const edgeLength = Math.hypot(coordinates[2] - coordinates[0], coordinates[3] - coordinates[1]);
const edgeWidth = Math.hypot(coordinates[4] - coordinates[2], coordinates[5] - coordinates[3]);
assert(Math.abs(edgeLength / edgeWidth - 2) < 1e-10, 'Preview does not preserve the 2:1 dimension ratio');
// Retain the previous form UI only as a regression fixture; production loads chat.ts.
const server = await createServer({ root, plugins: [{ name: "legacy-form-fixture", transformIndexHtml: html => html.replace('src="/src/chat.ts"', 'src="/src/main.ts"') }], server: { host: "127.0.0.1", port: 0 } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.HAZLOMEXA_TEST_BROWSER ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", headless: true });
  const page = await browser.newPage({ viewport: { width: 1200, height: 1000 } });
  const errors = []; page.on("pageerror", error => errors.push(String(error)));
  await page.addInitScript(() => {
    let callback = 0;
    let snapshot = { project: ["test-project", "C:\\test-project"], job: null, inference: null, recovered: [], freecad: "test-runtime", freecad_available: true };
    window.requests = [];
    window.mockExternalProposal=()=>{snapshot.job=structuredClone(snapshot.job);snapshot.job.snapshot.schema_version=10;snapshot.job.snapshot.audit={source:{provider:"mcp"},user_edited:false,adapter_version:1,destination:"test",created_unix_ms:Date.now(),updated_unix_ms:Date.now(),sequence:1,error:null,files_verified:false};snapshot.job.snapshot.phase="awaiting_approval";snapshot.job.snapshot.input.job_id=crypto.randomUUID();snapshot.job.snapshot.input.edit.value="22k";snapshot.job.approval_id=crypto.randomUUID();snapshot.job.sequence=1;snapshot.job.busy=false;};
    window.mockBusyChat=provider=>{snapshot.chat=provider?{provider,id:crypto.randomUUID(),project_id:snapshot.project[0],sequence:1,busy:true,cancelled:false,text:"Revisa la propuesta.",error:null}:null;};
    window.mockToolHistory=()=>{const past=structuredClone(snapshot.job.snapshot);past.input.job_id="past-agent-proposal";past.audit.files_verified=true;snapshot.recovered.push(past);};
    window.addHistory=run=>snapshot.recovered.push(run);
    window.setProjectFolder=folder=>{snapshot.project[1]=folder;snapshot.chat=null;snapshot.claude_conversation_ready=false;snapshot.codex_conversation_ready=false;};
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => ++callback,
      unregisterCallback: () => {},
      invoke: async (command, args) => {
        window.requests.push({ command, format:args?.format, prompt: args?.prompt, dimensions: args?.dimensions, fqbn: args?.fqbn, source: args?.source, request:args?.request, units:args?.units, reference:args?.reference, value:args?.value, selection:args?.selection,newSession:args?.newSession,baseJobId:args?.baseJobId,resultSha256:args?.resultSha256,projectId:args?.projectId,questionId:args?.questionId,answers:args?.answers,component:args?.component,x:args?.x,y:args?.y });
        if(command==="select_codex"){snapshot.codex_conversation_ready=false;snapshot.codex={executable:"C:\\owned\\codex.exe",sha256:"b".repeat(64),version:"0.159.2"};snapshot.codex_login=null;return structuredClone(snapshot);}
        if(command==="connect_codex"){
          const sequence=(snapshot.codex_login?.sequence??0)+1;
          snapshot.codex_login={id:crypto.randomUUID(),sequence,busy:true,cancelled:false,connected:false,auth_url:null,error:null};
          const initial=structuredClone(snapshot);
          snapshot.codex_login.sequence++;snapshot.codex_login.auth_url="https://auth.openai.com/oauth/authorize?state=fixture";
          window.codexLoginEvents=args.events;
          args.events.onmessage(structuredClone(snapshot.codex_login));
          const prior=window.finishCodexLogin;
          window.sendOldCodexLogin=prior;
          const view=structuredClone(snapshot.codex_login);
          window.finishCodexLogin=()=>{view.sequence++;view.busy=false;view.connected=true;view.auth_url=null;if(snapshot.codex_login.id===view.id)snapshot.codex_login=structuredClone(view);args.events.onmessage(structuredClone(view));};
          return initial;
        }
        if(command==="open_codex_login"){assertLogin();return;function assertLogin(){if(!snapshot.codex_login?.busy || !snapshot.codex_login.auth_url || args.id!==snapshot.codex_login.id)throw Error("Stale login link");}}
        if(command==="cancel_codex_login"){snapshot.codex_login.cancelled=true;snapshot.codex_login.auth_url=null;snapshot.codex_login.sequence++;const response=structuredClone(snapshot.codex_login);snapshot.codex_login.busy=false;snapshot.codex_login.sequence++;window.codexLoginEvents.onmessage(structuredClone(snapshot.codex_login));return response;}
        if(command==="select_claude"){snapshot.claude_conversation_ready=false;snapshot.claude={executable:"C:\\owned\\claude.exe",sha256:"a".repeat(64),version:"2.1.285"};return structuredClone(snapshot);}
        if(command==="start_claude_chat" || command==="start_codex_chat"){
          snapshot.chat={provider:command==="start_codex_chat"?"codex":"claude",id:crypto.randomUUID(),project_id:snapshot.project[0],sequence:1,busy:true,cancelled:false,text:"",error:null};
          const id=snapshot.chat.id;
          if(window.askCodexQuestion && command==="start_codex_chat"){
            snapshot.chat.question={id:crypto.randomUUID(),questions:[{id:"units",header:"Units",question:"¿Milímetros o pulgadas? <img src=x onerror=window.questionInjection=true>",options:[{label:"mm",description:"Milímetros"},{label:"in",description:"Pulgadas"}],isOther:false,isSecret:false},{id:"purpose",header:"Purpose",question:"¿Para qué la usarás?",options:[{label:"Para trabajar",description:"Uso diario"},{label:"Para viajar",description:"Uso portátil"}],isOther:true,isSecret:false}]};
            snapshot.chat.sequence++;window.chatEvents=args.events;args.events.onmessage(structuredClone(snapshot.chat));return structuredClone(snapshot);
          }
          setTimeout(()=>{if(snapshot.chat?.id===id && !snapshot.chat.cancelled){snapshot.chat.sequence++;snapshot[command==="start_codex_chat"?"codex_conversation_ready":"claude_conversation_ready"]=true;snapshot.chat.text="¿Para qué la usarás? <img src=x onerror=window.chatInjection=true>";snapshot.chat.busy=false;}},100);
          return structuredClone(snapshot);
        }
        if(command==="answer_codex_question"){
          if(snapshot.chat.question?.id!==args.questionId)throw Error("Stale question");
          if(args.answers.units!=="in" || !args.answers.purpose)throw Error("Missing answers");
          snapshot.chat.sequence++;snapshot.chat.question=null;const old=structuredClone(snapshot.chat);
          snapshot.chat.sequence++;snapshot.chat.busy=false;snapshot.chat.text="Usaremos pulgadas para la pieza.";window.chatEvents.onmessage(structuredClone(snapshot.chat));return old;
        }
        if(command==="cancel_chat"){snapshot[snapshot.chat.provider==="codex"?"codex_conversation_ready":"claude_conversation_ready"]=false;snapshot.chat.cancelled=true;snapshot.chat.busy=false;snapshot.chat.question=null;snapshot.chat.sequence++;return structuredClone(snapshot.chat);}
        if (command === "get_snapshot") return structuredClone(snapshot);
        if (command === "set_agent_tools") {snapshot.agent_tools_enabled=args.enabled;return args.enabled ? {command:"C:\\owned\\hazlomexa-tools.exe",env:{HAZLOMEXA_PIPE:"test-pipe",HAZLOMEXA_TOKEN:"temporary-test-token"}} : null;}
        if (command === "select_konnect") {snapshot.konnect="C:\\owned\\konnect.exe";return structuredClone(snapshot);}
        if(command==="discover_pcb")return {id:"selected-pcb",project_id:snapshot.project[0],name:"Selected.kicad_pcb",sha256:"c".repeat(64),components:[{reference:"J1",uuid:"1390739f-0b91-4d50-b8ae-92acd725ec51",library:"TestPoint:TestPoint_Pad_3.0x3.0mm",value:"Conn_01x01_Pin",layer:"F.Cu",x_mm:200.8,y_mm:156.6,rotation_deg:0},{reference:"R1",uuid:"2390739f-0b91-4d50-b8ae-92acd725ec51",library:"Resistor_SMD:R_1206_3216Metric",value:"<img src=x onerror=window.pcbInjection=true>",layer:"B.Cu",x_mm:10,y_mm:20,rotation_deg:90}].concat(["constructor","__proto__","toString"].map((namespace,index)=>({reference:`X${index+1}`,uuid:`custom-${index}`,library:`${namespace}:Custom`,value:"custom",layer:"F.Cu",x_mm:1,y_mm:2,rotation_deg:0}))) };
        if(command==="prepare_pcb_move"){
          if(args.selection!=="selected-pcb" || args.component!=="1390739f-0b91-4d50-b8ae-92acd725ec51")throw Error("Wrong PCB selection");
          const factor={mm:1,in:25.4,mils:0.0254}[args.units];
          snapshot.job={snapshot:{schema_version:11,project_id:snapshot.project[0],input:{schema_version:1,job_id:"pcb-job",konnect_version:"0.13.0",source:{schema_version:1,job_id:"pcb-job",kicad_version:"10.0.7",source_root:"C:\\owned\\Sources",entry:"Selected.kicad_pcb",files:[{path:"Selected.kicad_pcb",bytes:12000,sha256:"c".repeat(64)}],units:args.units,layers:["F.Cu","B.Cu","Edge.Cuts"]},pcb_move:{reference:"J1",uuid:args.component,expected_x_mm:200.8,expected_y_mm:156.6,x_mm:args.x*factor,y_mm:args.y*factor}},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          snapshot.job.snapshot.audit={source:{provider:"manual"},user_edited:false,adapter_version:1,destination:".hazlomexa/artifacts/pcb-job",created_unix_ms:Date.now(),updated_unix_ms:Date.now(),sequence:1,error:null,files_verified:false};
          return structuredClone(snapshot);
        }
        if (command === "discover_schematic") return {id:"selected-schematic",project_id:snapshot.project[0],name:"Selected.kicad_sch",sha256:"a".repeat(64),components:[{reference:"R1",value:"10k",library:"Device:R_US"},{reference:"R2",value:"<img src=x onerror=window.discoveryInjection=true>",library:"Device:R"}].concat(["constructor","__proto__","toString"].map((library,index)=>({reference:`X${index+1}`,value:"custom",library}))) };
        if (command === "prepare_konnect" || command === "prepare_discovered_schematic") {
          snapshot.job={snapshot:{schema_version:9,input:{schema_version:2,job_id:"konnect-job",konnect_version:"0.13.0",source:{schema_version:1,job_id:"konnect-job",kicad_version:"10.0.7",source_root:"C:\\owned\\Sources",entry:"Selected.kicad_sch",files:[{path:"Selected.kicad_sch",bytes:1024,sha256:"a".repeat(64)}],units:args.units,layers:[]},edit:{reference:args.reference,expected_value:"10k",value:args.value}},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          return structuredClone(snapshot);
        }
        if(command==="read_cadpilot_result"){
          const request=snapshot.job.snapshot.input.request;
          return {project_id:snapshot.project[0],job_id:args.jobId,sha256:"e".repeat(64),format:"cadpilot_report",content:JSON.stringify({catalog:{Target:"Part::Box",Other:"Part::Box"},after:{Target:{bbox:{xmin:0,ymin:0,zmin:0,xmax:12.7,ymax:25.4,zmax:7.62}},Other:{bbox:{xmin:0,ymin:0,zmin:0,xmax:3.5355,ymax:3.5355,zmax:4},dimensions_mm:{length:2,width:3,height:4}}},operation:request.operation,object:request.object,parameters_mm:Object.fromEntries(Object.entries(request.parameters).map(([k,v])=>[k,v*(request.units==="in" ? 25.4 : 1)]))})};
        }
        if (command === "prepare_cadpilot" || command==="prepare_cadpilot_followup") {
          snapshot.job={snapshot:{schema_version:8,input:{schema_version:1,job_id:"pilot-job-"+crypto.randomUUID(),cadpilot_version:"0.7.7",source_name:"Selected.FCStd",request:args.request,files:[{path:"Original.FCStd",bytes:6000,sha256:"a".repeat(64)}],preview:{catalog:{Target:"Part::Box"},measurements:{Target:{bbox:{xmin:0,ymin:0,zmin:0,xmax:10,ymax:20,zmax:30},volume_mm3:6000}}}},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          if(window.nativeBoxPreview){
            const measured=snapshot.job.snapshot.input.preview.measurements.Target;
            measured.dimensions_mm={length:10,width:20,height:30};measured.bbox.xmax=21.2132;measured.bbox.ymax=21.2132;
          }
          return structuredClone(snapshot);
        }
        if (command === "select_orca") { snapshot.orca="C:\\owned\\orca-slicer.exe"; return structuredClone(snapshot); }
        if (command === "prepare_slice") {
          snapshot.job={snapshot:{schema_version:7,input:{schema_version:1,job_id:"slice-job",orca_version:"2.4.2",source_name:"Original.stl",units:args.request.units,bed:args.request.bed,mesh:{triangles:12,bounds_mm:[[0,0,0],[25.4,25.4,25.4]],shape_sha256:"c".repeat(64)},settings:{machine:"Prusa MK3S 0.4 nozzle",process:"0.20mm Standard",filament:"PLA",bed:"Textured PEI Plate",nozzle_mm:0.4,layer_mm:0.2,nozzle_c:220,bed_c:args.request.bed_temperature_c,filament_mm:1.75,bed_area_mm:[[0,0],[250,0],[250,210],[0,210]],printable_height_mm:210},files:[{path:"Original.stl",bytes:684,sha256:"a".repeat(64)},{path:"Machine.json",bytes:900,sha256:"b".repeat(64)}]},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          return structuredClone(snapshot);
        }
        if (command === "select_kicad") { snapshot.kicad="C:\\owned\\kicad-cli.exe"; return structuredClone(snapshot); }
        if (command === "prepare_kicad") {
          snapshot.job={snapshot:{schema_version:6,input:{schema_version:1,job_id:"kicad-job",kicad_version:"10.0.7",source_root:"C:/source",entry:"main.kicad_sch",files:[{path:"main.kicad_sch",sha256:"a".repeat(64),bytes:120},{path:"child.kicad_sch",sha256:"b".repeat(64),bytes:80}],units:args.units,layers:[]},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          return structuredClone(snapshot);
        }
        if (command === "read_kicad_summary") {
          if(snapshot.recovered.some(run=>run.input.job_id===args.jobId && run.input.pcb_move))return {check:"drc",units:"mils",errors:1,warnings:7,excluded:0,ignored_checks:["missing_courtyard"]};
          return {check:"erc",units:snapshot.job.snapshot.input.units,errors:2,warnings:1,excluded:3,ignored_checks:["<img src=x onerror=window.previewInjection=true>"]};
        }
        if (command === "select_cadcraft") { snapshot.cadcraft="C:\\owned\\cadcraft-cli.exe"; return structuredClone(snapshot); }
        if(command==="discover_drawing")return {id:"selected-drawing",project_id:snapshot.project[0],inspection:{source_name:"Original.dxf",source_format:"dxf",source_sha256:"a".repeat(64),insunits:window.drawingInsunits ?? 1,summary:{entity_count:1,counts:{Circle:1},extents:[[-10,-10],[10,10]]}}};
        if (command === "prepare_selected_drawing") {
          if(args.selection!=="selected-drawing")throw Error("Wrong drawing selection");
          snapshot.job={snapshot:{schema_version:5,input:{schema_version:1,job_id:"drawing-job",cadcraft_version:"0.5.0",source_name:"Original.dxf",source_format:"dxf",source_sha256:"a".repeat(64),original_summary:{entity_count:1,counts:{Circle:1},extents:[[-10,-10],[10,10]]},...args.request},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:1,busy:false,error:null};
          return structuredClone(snapshot);
        }
        if (command === "connect_agent") {
          snapshot.agent = {version:"0.7.1",protocol_version:"2025-11-25"};
          return structuredClone(snapshot);
        }
        if (command === "select_agent_sketch") {
          snapshot.agent_source = {project_id:snapshot.project[0],source_name:"Original.ino",source:"void setup() {}\nvoid loop() {}\n"};
          return structuredClone(snapshot);
        }
        if (command === "start_agent_edit" || command === "edit_agent_proposal") {
          snapshot.inference = null;
          snapshot.job = {snapshot:{schema_version:4,input:{schema_version:1,job_id:"agent-job",agent_version:"0.7.1",source_name:snapshot.agent_source.source_name,original_source:snapshot.agent_source.source,source:args.source},phase:"awaiting_approval"},approval_id:crypto.randomUUID(),sequence:(snapshot.job?.sequence??0)+1,busy:false,error:null};
          return structuredClone(snapshot.job);
        }
        if (command === "select_arduino") {
          snapshot.arduino = { executable: "C:\\owned\\arduino-cli.exe", data: "C:\\owned\\data" };
          return structuredClone(snapshot);
        }
        if (command === "start_compile" || command === "edit_compile") {
          snapshot.inference = null;
          snapshot.job = { snapshot: { schema_version: 3, input: { schema_version: 1, job_id: "arduino-job", fqbn: args.fqbn, source: args.source }, phase: "awaiting_approval" },
            approval_id: crypto.randomUUID(), sequence: (snapshot.job?.sequence ?? 0) + 1, busy: false, error: null };
          return structuredClone(snapshot.job);
        }
        if (command === "select_project") {
          if (window.cancelPicker) { window.cancelPicker = false; throw "Selección cancelada."; }
          const id = snapshot.project[0] === "test-project" ? "other-project" : "test-project";
          snapshot = { ...snapshot, project: [id, `C:\\${id}`], job: null, inference: null, agent_source:null, recovered: window.lastFailure ? [window.lastFailure] : [] };
          return structuredClone(snapshot);
        }
        if (command === "start_inference") {
          snapshot.inference = { id: crypto.randomUUID(), sequence: 1, profile: args.profile, model: "test-model", digest: "test", busy: false, cancelled: false, text: "", error: null, message: "¿Cuánto mide de alto y en qué unidades?" };
          args.events.onmessage(structuredClone(snapshot.inference));
          return structuredClone(snapshot);
        }
        if (command === "start_proposal" || command === "edit_proposal") {
          snapshot.inference = null;
          snapshot.job = { snapshot: { schema_version: 2, input: { job_id: "job", dimensions_mm: args.dimensions }, phase: "awaiting_approval" }, approval_id: "approval", sequence: (snapshot.job?.sequence ?? 0) + 1, busy: false, error: null };
          return structuredClone(snapshot.job);
        }
        if (command === "cancel_job") {
          snapshot.job.sequence++;
          snapshot.job.snapshot.phase = window.raceMode === "commit-first" ? "completed" : "cancelled";
          if (!window.raceMode) return structuredClone(snapshot.job);
          snapshot.job.busy = window.raceMode === "cancel-first";
          const reply = structuredClone(snapshot.job);
          if (snapshot.job.busy) { snapshot.job.busy = false; snapshot.job.sequence++; }
          window.raceEvents.onmessage(structuredClone(snapshot.job));
          return reply;
        }
        if (command === "approve_operation") {
          snapshot.job.sequence++;
          if (window.raceMode) {
            snapshot.job.snapshot.phase = "running";
            snapshot.job.busy = true;
            snapshot.job.approval_id = null;
            window.raceEvents = args.events;
            window.oldRunning = structuredClone(snapshot.job);
          } else if (window.publicationFault) {
            snapshot.job.snapshot.phase = "failed";
            snapshot.job.snapshot.audit = { source: { provider: "manual" }, created_unix_ms: Date.now(), files_verified: true };
            snapshot.job.error = "No se pudieron publicar los archivos.";
            window.lastFailure = structuredClone(snapshot.job.snapshot);
          } else snapshot.job.snapshot.phase = "completed";
          return structuredClone(snapshot.job);
        }
        if (command === "open_artifact") {
          if (args.format === "cadpilot_report") return '{"object":"<img src=x onerror=window.previewInjection=true>","volume_mm3":6000}';
          if (args.format === "slice_report") return '{"settings":{"machine":"<img src=x onerror=window.previewInjection=true>"},"layers":127}';
          if (args.format === "kicad_report") return '{"errors":["<img src=x onerror=window.previewInjection=true>"]}';
          if (args.format === "konnect_report") return '{"edit":"<img src=x onerror=window.previewInjection=true>"}';
          if (args.format === "drawing_report") return '{"warnings":["<img src=x onerror=window.previewInjection=true>"]}';
          if (args.format === "original_sketch") return snapshot.agent_source.source;
          if (args.format === "sketch") return '<img src=x onerror="window.previewInjection=true">\nvoid setup() {}';
          throw "Aplicación no disponible.";
        }
        throw new Error(`Unexpected command ${command}`);
      },
    };
  });
  await page.goto(server.resolvedUrls.local[0]);
  await page.waitForFunction(() => !document.getElementById("infer").disabled);
  assert.equal(await page.locator("#manual-dimensions").getAttribute("open"), null);
  for(const id of ["length","width","height"])assert.equal(await page.locator(`#${id}`).inputValue(),"");
  assert.equal(await page.locator("#prepare").isDisabled(),true);
  assert.match(await page.locator("#conversation").innerText(), /construir o recrear/);
  await page.locator("#prompt").fill("Quiero recrear un bloque: largo 1 pulgada y ancho 2 cm.");
  await page.locator("#infer").click();
  await page.waitForFunction(() => document.querySelectorAll(".chat-message").length === 2);
  assert.match(await page.locator("#conversation").innerText(), /alto y en qué unidades/);
  await page.reload();
  await page.waitForFunction(() => document.querySelectorAll(".chat-message").length === 2);
  await page.locator("#prompt").fill("El alto es media pulgada.");
  await page.locator("#infer").click();
  await page.waitForFunction(() => document.querySelectorAll(".chat-message").length === 4);
  const request = await page.evaluate(() => window.requests.find(r => r.command === "start_inference"));
  const conversation = JSON.parse(request.prompt).conversation;
  assert.equal(conversation.length, 3); assert.match(conversation[0].content, /largo 1 pulgada/); assert.match(conversation[2].content, /media pulgada/);
  await page.locator("#open-project").click();
  await page.waitForFunction(() => document.querySelectorAll(".chat-message").length === 1);
  assert.doesNotMatch(await page.locator("#conversation").innerText(), /largo 1 pulgada/);
  await page.locator("#open-project").click();
  await page.waitForFunction(() => document.querySelectorAll(".chat-message").length === 4);
  const evidence = path.resolve(root, "../../target/design-flow"); await mkdir(evidence, { recursive: true });
  await page.screenshot({ path: path.join(evidence, "chat-followup.png"), fullPage: true });
  await page.locator("#manual-dimensions > summary").click();
  await page.locator("#unit").selectOption("in");
  for(const id of ["length","width","height"])assert.equal(await page.locator(`#${id}`).inputValue(),"");
  assert.equal(await page.locator(".preview svg").isVisible(),false);
  await page.locator("#length").fill("1");
  await page.locator("#unit").selectOption("cm");
  assert.equal(await page.locator("#length").inputValue(),"2.54");
  assert.equal(await page.locator("#width").inputValue(),"");
  assert.equal(await page.locator("#height").inputValue(),"");
  assert.equal(await page.locator("#prepare").isDisabled(),true);
  assert.match(await page.locator("#dimension-range").innerText(),/Falta confirmar: ancho, alto/);
  await page.evaluate(()=>document.getElementById("dimensions-form").dispatchEvent(new Event("submit",{bubbles:true,cancelable:true})));
  await page.waitForFunction(()=>document.getElementById("error").textContent.includes("Confirma largo, ancho y alto"));
  assert.equal(await page.evaluate(()=>window.requests.some(r=>r.command==="start_proposal")),false);
  await page.screenshot({path:path.join(evidence,"unknown-dimensions.png"),fullPage:true});
  await page.locator("#unit").selectOption("in");
  await page.locator("#length").fill("8");await page.locator("#width").fill("1");await page.locator("#height").fill("1");
  assert.equal(await page.locator("#prepare").isDisabled(),true,"An 8-inch dimension exceeds the verified 200 mm limit");
  assert.match(await page.locator("#dimension-range").innerText(),/deben estar dentro de ese intervalo/);
  await page.evaluate(()=>document.getElementById("dimensions-form").dispatchEvent(new Event("submit",{bubbles:true,cancelable:true})));
  assert.equal(await page.evaluate(()=>window.requests.some(r=>r.command==="start_proposal")),false);
  await page.locator("#length").fill("0.01");
  assert.equal(await page.locator("#prepare").isDisabled(),true,"Sub-millimeter inputs cannot prepare this workflow");
  await page.locator("#length").fill("1"); await page.locator("#width").fill("0.5"); await page.locator("#height").fill("1.25");
  await page.locator("#prepare").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  assert.deepEqual(await page.evaluate(() => window.requests.find(r => r.command === "start_proposal").dimensions), { length: 25.4, width: 12.7, height: 31.75 });
  assert.match(await page.locator("#operation-preview").innerText(), /25.4 × 12.7 × 31.75 mm/);
  await page.locator("#unit").selectOption("cm");
  assert.equal(await page.locator("#length").inputValue(), "2.54");
  assert.equal(await page.locator("#approve").isDisabled(), false);
  await page.locator("#length").fill("3"); assert.equal(await page.locator("#approve").isDisabled(), true);
  await page.evaluate(() => { window.cancelPicker = true; });
  await page.locator("#open-project").click();
  await page.waitForFunction(() => !document.getElementById("open-project").disabled);
  assert.equal(await page.locator("#error").isVisible(), false);
  assert.equal(await page.locator("#approve").isDisabled(), true);
  await page.locator("#prepare").click(); await page.waitForFunction(() => !document.getElementById("approve").disabled);
  assert.match(await page.locator("#conversation").innerText(), /largo 30 mm/);
  await page.locator("#cancel").click();
  await page.locator("#prompt").fill("Ahora cambia solo el alto a 2 cm.");
  await page.locator("#infer").click();
  await page.waitForFunction(() => !document.getElementById("infer").disabled);
  const latestPrompt = await page.evaluate(() => window.requests.filter(r => r.command === "start_inference").at(-1).prompt);
  assert(JSON.parse(latestPrompt).conversation.some(turn => turn.role === "user" && turn.content.includes("largo 30 mm")), 'Manual edit was not carried into follow-up context');
  await page.locator("#new-conversation").click();
  assert.equal(await page.locator(".chat-message").count(), 1);
  assert.equal(await page.evaluate(() => window.requests.some(r => r.command === "approve_operation")), false);
  await page.locator("#prepare").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  await page.locator("#approve").click();
  await page.waitForFunction(() => document.getElementById("publication-result").textContent.includes("disponibles"));
  await page.locator("#artifacts button").first().click();
  await page.waitForFunction(() => document.getElementById("opening-status").textContent.includes("No se pudo abrir"));
  assert.match(await page.locator("#validation-result").innerText(), /verificados/);
  assert.match(await page.locator("#publication-result").innerText(), /disponibles/);
  await page.screenshot({ path: path.join(evidence, "chat-and-units.png"), fullPage: true });
  await page.evaluate(() => { window.cancelPicker = true; });
  await page.locator("#open-project").click();
  await page.waitForFunction(() => !document.getElementById("open-project").disabled);
  assert.equal(await page.locator("#opening-status").isVisible(), true, 'Cancelled picker cleared the current project opening result');
  await page.locator("#open-project").click();
  await page.waitForFunction(() => document.getElementById("project-path").textContent.includes("other-project"));
  for(const id of ["length","width","height"])assert.equal(await page.locator(`#${id}`).inputValue(),"");
  assert.equal(await page.locator("#prepare").isDisabled(),true);
  assert.equal(await page.locator("#opening-status").isVisible(), false, 'Opening result from another project is still displayed');
  assert.equal(await page.locator("#opening-status").innerText(), "");
  if (await page.locator("#manual-dimensions").getAttribute("open") === null) await page.locator("#manual-dimensions summary").click();
  await page.locator("#length").fill("3");await page.locator("#width").fill("1.27");await page.locator("#height").fill("3.175");
  await page.locator("#prepare").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  await page.evaluate(() => { window.publicationFault = true; });
  await page.locator("#approve").click();
  await page.waitForFunction(() => document.getElementById("error").textContent.includes("publicar"));
  assert.equal(await page.locator("#generation-result").innerText(), "Completada", 'Publication failure incorrectly reports generation failure');
  assert.match(await page.locator("#validation-result").innerText(), /verificados/);
  assert.match(await page.locator("#publication-result").innerText(), /Falló/);
  assert.match(await page.locator("#status").innerText(), /publicación/);
  assert.equal(await page.locator("#artifacts button").count(), 0, 'Unpublished outputs must not get opening buttons');
  await page.locator("#open-project").click();
  await page.waitForFunction(() => document.getElementById("history").textContent.includes("Falló la publicación"));
  assert.match(await page.locator("#history").innerText(), /Archivos generados y verificados/);
  for(const id of ["length","width","height"])assert.equal(await page.locator(`#${id}`).inputValue(),"");
  await page.locator("#length").fill("3");await page.locator("#width").fill("1.27");await page.locator("#height").fill("3.175");
  for (const winner of ["cancel-first", "commit-first"]) {
    await page.evaluate(winner => { window.publicationFault = false; window.raceMode = winner; }, winner);
    await page.locator("#prepare").click();
    await page.waitForFunction(() => !document.getElementById("approve").disabled);
    await page.locator("#approve").click();
    await page.waitForFunction(() => !document.getElementById("cancel").disabled);
    await page.locator("#cancel").click();
    await page.waitForFunction(() => !document.getElementById("prepare").disabled);
    await page.evaluate(() => window.raceEvents.onmessage(window.oldRunning));
    assert.equal(await page.locator("#prepare").isDisabled(), false, 'Delayed Running event restored busy controls after final outcome');
    assert.equal(await page.locator("#cancel").isDisabled(), true);
    assert.equal(await page.locator("#artifacts button").count(), winner === "commit-first" ? 2 : 0);
    assert.match(await page.locator("#status").innerText(), winner === "commit-first" ? /verificados y publicados/ : /cancelada/);
  }
  await page.evaluate(() => { window.raceMode = null; });
  await page.locator("#workflow").selectOption("arduino");
  assert.equal(await page.locator("#manual-dimensions").isVisible(), false);
  assert.equal(await page.locator("#preview-heading").innerText(),"Última operación del proyecto");
  assert.match(await page.locator("#review-note").innerText(),/operación anterior/);
  assert.equal(await page.locator("#approval-note").isVisible(),false);
  assert.doesNotMatch(await page.locator("#operation-preview").innerText(),/Cambios sin aplicar/);
  assert.match(await page.locator("#conversation").innerText(), /Arduino Uno/);
  const inferenceCount = await page.evaluate(() => window.requests.filter(r => r.command === "start_inference").length);
  const beforeUnknownBoard=await page.locator("#sketch-source").inputValue();
  const compileCount=await page.evaluate(()=>window.requests.filter(r=>r.command==="start_compile").length);
  await page.locator("#prompt").fill("Quiero parpadear el LED integrado cada 500 ms, no sé si mi placa es Arduino Uno");
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/qué placa tienes/);
  assert.equal(await page.locator("#sketch-source").inputValue(),beforeUnknownBoard,'Uncertain board replaced the sketch');
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_compile").length),compileCount);
  await page.screenshot({path:path.join(evidence,"arduino-uncertain-board.png"),fullPage:true});
  await page.locator("#prompt").fill("Arduino Uno");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/500 ms encendido/);
  assert.match(await page.locator("#sketch-source").inputValue(),/delay\(500\)/);
  await page.locator("#new-conversation").click();
  const beforeFraction=await page.locator("#sketch-source").inputValue();
  await page.locator("#prompt").fill("Arduino Uno blink built-in LED every 1/2 seconds");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/0,5 segundos/);
  assert.equal(await page.locator("#sketch-source").inputValue(),beforeFraction,'Unsupported fraction replaced the sketch');
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_compile").length),compileCount);
  await page.screenshot({path:path.join(evidence,"arduino-interval-clarification.png"),fullPage:true});
  await page.locator("#prompt").fill("0.5 seconds");await page.locator("#infer").click();
  assert.match(await page.locator("#sketch-source").inputValue(),/delay\(500\)/);
  await page.locator("#new-conversation").click();
  await page.locator("#prompt").fill("Un ejemplo para parpadear el LED integrado de Arduino Uno");
  const originalSketch=await page.locator("#sketch-source").inputValue();
  await page.locator("#infer").click();
  await page.waitForFunction(() => document.getElementById("conversation").textContent.includes("un único intervalo"));
  assert.equal(await page.locator("#sketch-source").inputValue(),originalSketch,'Missing timing replaced the sketch');
  await page.locator("#prompt").fill("1 segundo");
  await page.locator("#infer").click();
  await page.waitForFunction(() => document.getElementById("sketch-source").value.includes("LED_BUILTIN"));
  assert.match(await page.locator("#sketch-source").inputValue(),/delay\(1000\)/);
  assert.equal(await page.evaluate(() => window.requests.filter(r => r.command === "start_inference").length), inferenceCount);
  const recipeSource = await page.locator("#sketch-source").inputValue();
  await page.locator("#prompt").fill("漢".repeat(4096));
  await page.locator("#infer").click();
  await page.waitForFunction(() => !document.getElementById("error").hidden && document.getElementById("error").textContent.includes("límite"));
  assert.equal(await page.locator("#sketch-source").inputValue(), recipeSource, 'Rejected oversized chat changed the sketch');
  await page.locator("#prepare-compile").click();
  await page.waitForFunction(() => document.getElementById("operation-preview").textContent.includes("arduino:avr:uno"));
  assert.equal(await page.locator("#preview-heading").innerText(),"Vista de la propuesta");
  assert.equal(await page.locator("#approval-note").isVisible(),true);
  await page.locator("#prompt").fill("Parpadea el LED integrado cada 250 ms");
  assert.equal(await page.locator("#infer").isDisabled(),true);
  await page.locator("#inference-form").dispatchEvent("submit");
  await page.waitForFunction(() => document.getElementById("conversation").textContent.includes("Cancela o termina la propuesta"));
  assert.equal(await page.locator("#sketch-source").inputValue(),recipeSource,'Chat replaced code under review');
  assert.equal(await page.locator("#approve").isDisabled(), true, 'Missing Arduino runtime allowed approval');
  await page.locator("#arduino-runtime summary").click();
  await page.locator("#select-arduino").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  assert.match(await page.locator("#code-preview").innerText(), /LED_BUILTIN/);
  await page.locator("#sketch-source").fill('void setup() {} void loop() {} // </pre><img src=x onerror="throw new Error(1)">');
  assert.equal(await page.locator("#approve").isDisabled(), true, 'Unsaved sketch edit left approval enabled');
  await page.locator("#prepare-compile").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  assert.equal(await page.locator("#code-preview img").count(), 0);
  assert.match(await page.locator("#code-preview").innerText(), /<img/);
  await page.reload();
  await page.waitForFunction(() => document.getElementById("workflow").value === "freecad");
  // Browser fixture state is reconstructed on reload; native persistence is tested in Rust.
  await page.locator("#workflow").selectOption("arduino");
  assert.match(await page.locator("#conversation").innerText(), /1000 ms encendido y 1000 ms apagado/);
  await page.locator("#sketch-source").fill("void setup() {} void loop() {}");
  await page.locator("#arduino-runtime summary").click();
  await page.locator("#select-arduino").click();
  await page.locator("#prepare-compile").click();
  await page.waitForFunction(() => !document.getElementById("approve").disabled);
  await page.locator("#approve").click();
  await page.waitForFunction(() => document.querySelectorAll("#artifacts button").length === 3);
  assert.match(await page.locator("#validation-result").innerText(), /Sketch, HEX y ELF/);
  await page.locator("#artifacts button").first().click();
  await page.locator("#artifact-preview").waitFor({ state: "visible" });
  assert.match(await page.locator("#artifact-source").innerText(), /<img src=x/);
  assert.equal(await page.locator("#artifact-source img").count(), 0);
  assert.equal(await page.evaluate(() => window.previewInjection), undefined);
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("#artifact-preview").isVisible(), false);
  await page.locator("#artifacts button").nth(1).click();
  await page.waitForFunction(() => document.getElementById("opening-status").textContent.includes("No se pudo abrir el archivo"));
  assert.match(await page.locator("#publication-result").innerText(), /disponibles/);
  await page.screenshot({ path: path.join(evidence, "arduino-workflow.png"), fullPage: true });
  await page.locator("#workflow").selectOption("agent");
  assert.equal(await page.locator("#opening-status").isVisible(),false,"A file-opening result from another workflow is still displayed");
  assert.equal(await page.locator("#opening-status").innerText(),"");
  assert.equal(await page.locator('label[for="arduino-board"]').isVisible(),false);
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.equal(await page.locator("#prepare-compile").isDisabled(),true);
  await page.locator("#prompt").fill("Quiero modificar el sketch y conservar el original.");
  const modelCalls = await page.evaluate(()=>window.requests.filter(request=>request.command==="start_inference").length);
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/selecciona el sketch original/i);
  await page.locator("#select-agent-sketch").click();
  assert.match(await page.locator("#sketch-source").inputValue(),/void setup/);
  await page.locator("#sketch-source").fill("void setup() {}\nvoid loop() { delay(250); }\n");
  await page.locator("#prepare-compile").click();
  assert.equal(await page.locator("#approve").isDisabled(),true);
  await page.locator("#connect-agent").click();
  assert.equal(await page.locator("#approve").isDisabled(),false);
  await page.locator("#sketch-source").fill("void setup() {}\nvoid loop() { delay(500); }\n");
  assert.equal(await page.locator("#approve").isDisabled(),true);
  await page.locator("#prepare-compile").click();
  assert.match(await page.locator("#code-preview").innerText(),/delay\(500\)/);
  await page.locator("#approve").click();
  assert.match(await page.locator("#validation-result").innerText(),/Original y copia/);
  assert.equal(await page.locator("#artifacts button").count(),2);
  await page.locator("#artifacts button").first().click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.equal(await page.locator("#artifact-source").innerText(),"void setup() {}\nvoid loop() {}\n");
  await page.keyboard.press("Escape");
  assert.equal(await page.evaluate(()=>window.requests.filter(request=>request.command==="start_inference").length),modelCalls);
  await page.screenshot({path:path.join(evidence,"agent-workflow.png"),fullPage:true});
  await page.locator("#open-project").click();
  assert.equal(await page.locator("#prepare-compile").isDisabled(),true);
  await page.locator("#workflow").selectOption("drawing");
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.equal(await page.locator("#arduino-form").isVisible(),false);
  assert.equal(await page.locator("#prepare-drawing").isDisabled(),true);
  assert.match(await page.locator("#conversation").innerText(),/DXF, DWG o PNG/);
  await page.locator("#prompt").fill("Quiero un dibujo en pulgadas y añadir un círculo.");
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/ancho completo \(diámetro\)/);
  await page.locator("#select-cadcraft").click();
  assert.equal(await page.locator("#prepare-drawing").isDisabled(),true);
  const beforeDiscovery=await page.evaluate(()=>window.requests.length);
  await page.evaluate(()=>window.drawingInsunits=0);
  await page.locator("#discover-drawing").click();
  assert.equal(await page.locator("#drawing-unit").inputValue(),"");
  assert.match(await page.locator("#drawing-discovery").innerText(),/no declara unidades/);
  assert.equal(await page.evaluate(()=>window.requests.slice(window.requests.length-1)[0].command),"discover_drawing");
  assert.equal(await page.evaluate(()=>window.requests.slice(0).filter(r=>r.command==="prepare_selected_drawing").length),0);
  await page.evaluate(()=>window.drawingInsunits=1);
  await page.locator("#discover-drawing").click();
  assert.equal(await page.locator("#drawing-unit").inputValue(),"in");
  assert.match(await page.locator("#drawing-discovery").innerText(),/límites.*pulgadas/);
  assert.equal(await page.locator("#drawing-x").inputValue(),"");
  assert.equal(await page.locator("#drawing-diameter").inputValue(),"");
  await page.screenshot({path:path.join(evidence,"drawing-source-first.png"),fullPage:true});
  await page.locator("#drawing-unit").selectOption("in");
  await page.locator("#drawing-format").selectOption("dwg");
  await page.locator("#drawing-add-circle").check();
  for(const id of ["drawing-x","drawing-y","drawing-diameter"])assert.equal(await page.locator(`#${id}`).inputValue(),"");
  await page.locator("#prepare-drawing").click();
  assert.match(await page.locator("#error").innerText(),/posición horizontal y vertical del centro y el diámetro/);
  assert.equal(await page.evaluate(()=>window.requests.some(r=>r.command==="prepare_selected_drawing")),false);
  await page.locator("#drawing-x").fill("50.8");
  await page.locator("#drawing-y").fill("0");
  await page.locator("#drawing-diameter").fill("25.4");
  for(const [message,field,value] of [["añade un círculo",null,null],["1 pulgada","diameter","25.4"],["2 pulgadas","x","50.8"],["0 mm","y","0"]]){
    await page.locator("#prompt").fill(message);await page.locator("#infer").click();
    if(field)assert.equal(await page.locator(`#drawing-${field}`).inputValue(),value);
    assert.equal(await page.evaluate(()=>window.requests.some(r=>r.command==="prepare_selected_drawing")),false,"Chat only updates the draft");
  }
  await page.locator("#prompt").fill("diámetro de 0 mm");await page.locator("#infer").click();
  assert.equal(await page.locator("#drawing-diameter").inputValue(),"25.4");
  await page.locator("#new-conversation").click();
  await page.locator("#prompt").fill("5 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#drawing-diameter").inputValue(),"25.4","A new conversation forgets the pending measurement");
  await page.locator("#drawing-circle-unit").selectOption("in");
  assert.equal(await page.locator("#drawing-x").inputValue(),"2");
  assert.equal(await page.locator("#drawing-y").inputValue(),"0");
  assert.equal(await page.locator("#drawing-diameter").inputValue(),"1");
  await page.locator("#drawing-circle-unit").selectOption("mm");
  assert.equal(await page.locator("#drawing-x").inputValue(),"50.8");
  assert.equal(await page.locator("#drawing-diameter").inputValue(),"25.4");
  await page.locator("#drawing-diameter").fill("0");
  await page.locator("#prepare-drawing").click();
  assert.match(await page.locator("#error").innerText(),/diámetro positivo/);
  assert.equal(await page.evaluate(()=>window.requests.some(r=>r.command==="prepare_selected_drawing")),false);
  await page.locator("#drawing-diameter").fill("25.4");
  await page.locator("#prepare-drawing").click();
  await page.waitForFunction(()=>!document.getElementById("approve").disabled);
  assert.match(await page.locator("#operation-preview").innerText(),/pulgadas.*DWG.*diámetro 1/);
  assert.equal(await page.locator("#drawing-diameter").isDisabled(),true);
  assert.equal(await page.locator("#drawing-circle-unit").isDisabled(),true);
  assert.deepEqual(await page.evaluate(()=>window.requests.find(r=>r.command==="prepare_selected_drawing").request),{output_format:"dwg",units:"in",circle:{center:[2,0],radius:0.5},allow_loss:false});
  await page.locator("#approve").click();
  assert.equal(await page.locator("#artifacts button").count(),4);
  await page.locator("#artifacts button").last().click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-source").innerText(),/<img/);
  assert.equal(await page.locator("#artifact-source img").count(),0);
  assert.equal(await page.evaluate(()=>window.previewInjection),undefined);
  await page.keyboard.press("Escape");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_inference").length),modelCalls);
  await page.screenshot({path:path.join(evidence,"cadcraft-workflow.png"),fullPage:true});
  await page.locator("#workflow").selectOption("kicad");
  assert.equal(await page.locator("#prepare-kicad").isDisabled(),true);
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.equal(await page.locator("#drawing-form").isVisible(),false);
  assert.match(await page.locator("#conversation").innerText(),/hoja principal/);
  await page.locator("#prompt").fill("Quiero revisar mi esquema, incluyendo sus hojas secundarias.");
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/bibliotecas locales/);
  await page.locator("#select-kicad").click();
  await page.locator("#kicad-unit").selectOption("mils");
  await page.locator("#prepare-kicad").click();
  assert.equal(await page.locator("#approve").isDisabled(),false);
  assert.equal(await page.locator("#kicad-unit").isDisabled(),true);
  assert.match(await page.locator("#operation-preview").innerText(),/ERC \+ PDF \+ netlist.*mils/);
  assert.match(await page.locator("#code-preview").innerText(),/child.kicad_sch/);
  assert.equal(await page.evaluate(()=>window.requests.find(r=>r.command==="prepare_kicad").units),"mils");
  await page.locator("#approve").click();
  assert.equal(await page.locator("#artifacts button").count(),4);
  assert.match(await page.locator("#status").innerText(),/no implica que el diseño esté libre de errores/);
  assert.match(await page.locator("#validation-result").innerText(),/comprobaciones desactivadas/);
  await page.locator("#artifacts button").last().click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-source").innerText(),/2 errores\n1 advertencias\n3 hallazgos excluidos\n1 comprobaciones desactivadas/);
  assert.equal(await page.locator("#artifact-source img").count(),0);
  await page.keyboard.press("Escape");
  await page.locator("#artifacts button").nth(2).click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-source").innerText(),/<img/);
  assert.equal(await page.evaluate(()=>window.previewInjection),undefined);
  await page.keyboard.press("Escape");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_inference").length),modelCalls);
  await page.screenshot({path:path.join(evidence,"kicad-workflow.png"),fullPage:true});
  await page.locator("#workflow").selectOption("slice");
  assert.equal(await page.locator("#prepare-slice").isDisabled(),true);
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.equal(await page.locator("#kicad-form").isVisible(),false);
  assert.match(await page.locator("#conversation").innerText(),/máquina, boquilla y material/);
  await page.locator("#prompt").fill("Quiero laminar un STL en pulgadas con PLA.");
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/temperatura inicial/);
  await page.locator("#select-orca").click();
  await page.locator("#slice-unit").selectOption("in");
  await page.locator("#slice-bed-temperature").fill("45");
  await page.locator("#prepare-slice").click();
  assert.equal(await page.locator("#approve").isDisabled(),false);
  assert.equal(await page.locator("#slice-unit").isDisabled(),true);
  assert.equal(await page.locator("#slice-bed-temperature").isDisabled(),true);
  assert.match(await page.locator("#operation-preview").innerText(),/original in → 25.4 × 25.4 × 25.4 mm/);
  assert.match(await page.locator("#operation-preview").innerText(),/cama 45 °C/);
  assert.match(await page.locator("#code-preview").innerText(),/Machine.json/);
  const sliceRequest=await page.evaluate(()=>window.requests.find(r=>r.command==="prepare_slice").request);
  assert.deepEqual(sliceRequest,{units:"in",bed:"textured",bed_temperature_c:45});
  await page.locator("#approve").click();
  assert.equal(await page.locator("#artifacts button").count(),4);
  assert.match(await page.locator("#validation-result").innerText(),/3MF, dimensiones, perfiles, unidades y G-code/);
  await page.locator("#artifacts button").last().click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-source").innerText(),/<img/);
  assert.equal(await page.locator("#artifact-source img").count(),0);
  assert.equal(await page.evaluate(()=>window.previewInjection),undefined);
  await page.keyboard.press("Escape");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_inference").length),modelCalls);
  await page.screenshot({path:path.join(evidence,"orca-workflow.png"),fullPage:true});
  await page.locator("#workflow").selectOption("cadpilot");
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.equal(await page.locator("#manual-dimensions").isVisible(),false);
  assert.equal(await page.locator("#cadpilot-size").isVisible(),false);
  assert.match(await page.locator("#conversation").innerText(),/Medir e inspeccionar/);
  await page.locator("#prompt").fill("Quiero modificar este documento, pero no sé sus dimensiones.");
  await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/si no conoces sus medidas/);
  await page.locator("#cadpilot-unit").selectOption("in");
  await page.locator("#prepare-cadpilot").click();
  assert.match(await page.locator("#operation-preview").innerText(),/Medir documento/);
  assert.match(await page.locator("#code-preview").innerText(),/Target · Part::Box · extensión en el documento: 10 × 20 × 30 mm/);
  assert.match(await page.locator("#code-preview").innerText(),/ in · volumen 6000 mm³/);
  assert.equal(await page.locator("#cadpilot-operation").isDisabled(),true);
  assert.equal(await page.locator("#approve").isDisabled(),false);
  await page.locator("#approve").click();
  assert.equal(await page.locator("#artifacts button").count(),3);
  await page.locator("#artifacts button").nth(1).click();
  await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-preview-heading").innerText(),/Mediciones y operación CADPilot/);
  assert.equal(await page.locator("#artifact-source img").count(),0);
  await page.keyboard.press("Escape");
  await page.locator("#cadpilot-operation").selectOption("edit_box");
  await page.locator("#cadpilot-object").fill("Target");
  await page.locator("#cadpilot-length").fill("0.5");
  await page.locator("#cadpilot-width").fill("1");
  await page.locator("#cadpilot-height").fill("0.3");
  await page.locator("#prepare-cadpilot").click();
  assert.match(await page.locator("#operation-preview").innerText(),/Editar caja · Target/);
  assert.match(await page.locator("#operation-preview").innerText(),/largo: medida nativa anterior no disponible → 12.7 mm \(0.5 in\)/);
  assert.equal(await page.locator("#cadpilot-height").isDisabled(),true);
  const pilotRequest=await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_cadpilot").at(-1).request);
  assert.deepEqual(pilotRequest,{operation:"edit_box",object:"Target",units:"in",parameters:{length:0.5,width:1,height:0.3}});
  await page.locator("#approve").click();
  assert.equal(await page.locator("#artifacts button").count(),5);
  assert.match(await page.locator("#validation-result").innerText(),/Catálogo y geometría medidos/);
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_inference").length),modelCalls);
  assert.equal(await page.evaluate(()=>window.previewInjection),undefined);
  await page.screenshot({path:path.join(evidence,"cadpilot-workflow.png"),fullPage:true});
  await page.locator("#cadpilot-operation").selectOption("add_cylinder");
  await page.locator("#cadpilot-diameter").fill("2");
  for(const axis of ["x","y","z"])await page.locator(`#cadpilot-${axis}`).fill("1");
  await page.locator("#artifacts button").last().click();
  for(const key of ["diameter","x","y","z"])assert.equal(await page.locator(`#cadpilot-${key}`).inputValue(),"","Choosing a verified result clears unrelated draft measurements");
  await page.locator("#new-conversation").click();
  assert.match(await page.locator("#conversation").innerText(),/Ya leí el resultado guardado y verificado/);
  assert.equal(await page.locator("#cadpilot-component").inputValue(),"");
  assert.equal(await page.locator("#cadpilot-component option").count(),3);
  assert.equal(await page.locator("#cadpilot-object-label").isVisible(),false);
  await page.locator("#cadpilot-component").selectOption("Other");
  for(const [key,value] of Object.entries({length:"2",width:"3",height:"4"}))assert.equal(await page.locator(`#cadpilot-${key}`).inputValue(),value,"Reuse native lengths of the selected box, not its rotated world bounds or another object's operation parameters");
  assert.match(await page.locator('#cadpilot-component option[value="Other"]').innerText(),/2 × 3 × 4 mm · Other/);
  const beforeGuided=await page.evaluate(()=>window.requests.length);
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/alto final/);
  assert.equal(await page.locator("#cadpilot-height").inputValue(),"4");
  await page.locator("#prompt").fill("No sé cuánto");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),"4");
  await page.locator("#prompt").fill("0,5 pulgadas");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),"12.7");
  assert.equal(await page.locator("#cadpilot-length").inputValue(),"2");assert.equal(await page.locator("#cadpilot-width").inputValue(),"3");
  assert.equal(await page.evaluate(start=>window.requests.slice(start).some(r=>r.command.startsWith("start_") || r.command.startsWith("prepare_") || r.command==="infer" || r.command==="approve"),beforeGuided),false,"Guided clarification invoked inference or an engineering action");
  await page.screenshot({path:path.join(evidence,"cadpilot-guided-height.png"),fullPage:true});
  await page.locator("#prompt").fill("Make it wider");await page.locator("#infer").click();
  await page.locator("#cadpilot-component").selectOption("Target");
  await page.locator("#prompt").fill("1 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-width").inputValue(),"25.4","A pending question followed another box selection");
  const selectedHeight=await page.locator("#cadpilot-height").inputValue();
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  await page.locator("#new-conversation").click();
  await page.locator("#prompt").fill("2 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),selectedHeight,"A new conversation retained a pending dimension");
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  await page.locator("#workflow").selectOption("freecad");await page.locator("#workflow").selectOption("cadpilot");
  await page.locator("#prompt").fill("2 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),selectedHeight,"A workflow change retained a pending dimension");
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  await page.locator("#assistant-provider").selectOption("claude");await page.locator("#assistant-provider").selectOption("guided");
  await page.locator("#prompt").fill("2 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),selectedHeight,"A provider change retained a pending dimension");
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  await page.locator("#cadpilot-operation").selectOption("add_cylinder");await page.locator("#cadpilot-operation").selectOption("edit_box");
  await page.locator("#prompt").fill("2 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-height").inputValue(),selectedHeight,"Another operation retained a pending dimension");
  await page.locator("#cadpilot-component").selectOption("Target");
  assert.equal(await page.locator("#cadpilot-length").inputValue(),"12.7");
  await page.locator("#cadpilot-height").fill("38.1");
  await page.locator("#cadpilot-unit").selectOption("in");
  assert.equal(await page.locator("#cadpilot-height").inputValue(),"1.5");
  assert.equal(await page.locator("#cadpilot-length").inputValue(),"0.5");
  await page.locator("#cadpilot-operation").selectOption("add_box");
  await page.locator("#cadpilot-object").fill("Other");
  await page.locator("#cadpilot-operation").selectOption("edit_box");
  assert.equal(await page.locator("#cadpilot-object").isDisabled(),true);
  await page.evaluate(()=>{window.nativeBoxPreview=true;});
  await page.locator("#prepare-cadpilot").click();
  assert.match(await page.locator("#code-preview").innerText(),/largo × ancho × alto: 10 × 20 × 30 mm/);
  assert.match(await page.locator("#code-preview").innerText(),/extensión en el documento: 21.2132 × 21.2132 × 30 mm/);
  assert.match(await page.locator("#operation-preview").innerText(),/largo: 10 mm → 12.7 mm \(0.5 in\)/);
  assert.match(await page.locator("#operation-preview").innerText(),/alto: 30 mm → 38.1 mm \(1.5 in\)/);
  assert.doesNotMatch(await page.locator("#operation-preview").innerText(),/largo: 21.2132/);
  assert.equal(await page.locator("#cadpilot-component").isDisabled(),true);
  const followup=await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_cadpilot_followup").at(-1));
  assert.equal(followup.projectId,"other-project");assert.equal(followup.resultSha256,"e".repeat(64));
  assert.match(followup.baseJobId,/^pilot-job-/);
  assert.deepEqual(followup.request,{operation:"edit_box",object:"Target",units:"in",parameters:{length:0.5,width:1,height:1.5}});
  await page.locator("#cancel").click();
  await page.locator("#prompt").fill("Hazla más alta");await page.locator("#infer").click();
  await page.locator("#cadpilot-new-source").click();
  await page.locator("#prompt").fill("2 cm");await page.locator("#infer").click();
  assert.equal(await page.locator("#cadpilot-operation").inputValue(),"inspect");
  for(const key of ["object","length","width","height","diameter","x","y","z"])assert.equal(await page.locator(`#cadpilot-${key}`).inputValue(),"");
  await page.locator("#cadpilot-operation").selectOption("add_cylinder");
  assert.match(await page.locator("#cadpilot-diameter-label").innerText(),/ancho completo/);
  for(const axis of ["x","y","z"])assert.equal(await page.locator(`#cadpilot-${axis}`).inputValue(),"");
  await page.locator("#cadpilot-object").fill("Cylinder");
  await page.locator("#cadpilot-diameter").fill("2");
  await page.locator("#cadpilot-height").fill("1");
  await page.locator("#cadpilot-unit").selectOption("mm");
  assert.equal(await page.locator("#cadpilot-diameter").inputValue(),"50.8");
  assert.equal(await page.locator("#cadpilot-height").inputValue(),"25.4");
  assert.equal(await page.locator("#cadpilot-x").inputValue(),"");
  await page.locator("#cadpilot-unit").selectOption("in");
  assert.equal(await page.locator("#cadpilot-diameter").inputValue(),"2");
  const beforeCylinder=await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_cadpilot").length);
  await page.locator("#cadpilot-form").evaluate(form=>form.dispatchEvent(new Event("submit",{bubbles:true,cancelable:true})));
  await page.waitForFunction(()=>!document.querySelector("#prepare-cadpilot").disabled);
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_cadpilot").length),beforeCylinder);
  assert.match(await page.locator("#error").innerText(),/medidas y la posición/);
  for(const [axis,value] of [["x","1"],["y","0"],["z","0"]])await page.locator(`#cadpilot-${axis}`).fill(value);
  await page.locator("#prepare-cadpilot").click();
  const cylinderRequest=await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_cadpilot").at(-1).request);
  assert.deepEqual(cylinderRequest,{operation:"add_cylinder",object:"Cylinder",units:"in",parameters:{radius:1,height:1,x:1,y:0,z:0}});
  await page.waitForFunction(()=>document.querySelector("#operation-preview").textContent.includes("Añadir cilindro"));
  assert.match(await page.locator("#operation-preview").innerText(),/diámetro: 2 in \(50.8 mm\)/);
  assert.equal(await page.locator("#cadpilot-diameter").isDisabled(),true);
  await page.screenshot({path:path.join(evidence,"cadpilot-cylinder-inputs.png"),fullPage:true});
  await page.locator("#cancel").click();
  await page.locator("#workflow").selectOption("konnect");
  assert.equal(await page.locator("#prepare-konnect").isDisabled(),true);
  assert.equal(await page.locator("#model-controls").isVisible(),false);
  assert.match(await page.locator("#conversation").innerText(),/No necesitas saber sus identificadores/);
  await page.locator("#prompt").fill("Quiero cambiar una resistencia pero no sé su nombre");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/sin conocer nombres ni valores/);
  await page.locator("#discover-schematic").click();
  assert.match(await page.locator("#schematic-component").innerText(),/Resistencia · 10k/);
  for(const library of ["constructor","__proto__","toString"]) assert((await page.locator("#schematic-component").innerText()).includes(`${library} · custom`));
  assert.equal(await page.locator("#schematic-discovery img").count(),0);assert.equal(await page.evaluate(()=>window.discoveryInjection),undefined);
  const beforeSchematicChat=await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_discovered_schematic").length);
  await page.locator("#prompt").fill("cambia la resistencia de 10k a 12.34k");await page.locator("#infer").click();
  assert.equal(await page.locator("#schematic-component").inputValue(),"R1");
  assert.equal(await page.locator("#konnect-value").inputValue(),"12.34k");
  assert.match(await page.locator("#conversation").innerText(),/Resistencia · 10k → 12.34k/);
  await page.locator("#prompt").fill("cambia la resistencia a 33k");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/2 componentes que coinciden/);
  assert.equal(await page.locator("#konnect-value").inputValue(),"12.34k");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="prepare_discovered_schematic").length),beforeSchematicChat);
  await page.locator("#prompt").fill("cámbiala a 12.34k");await page.locator("#infer").click();
  assert.match(await page.locator("#schematic-discovery").innerText(),/valor actual es 10k/);
  assert.equal(await page.locator("#konnect-reference").inputValue(),"R1");
  await page.locator("#select-konnect").click();
  await page.locator("#prepare-konnect").click();
  assert.match(await page.locator("#operation-preview").innerText(),/R1: 10k → 12.34k/);
  assert.equal(await page.locator("#konnect-value").isDisabled(),true);assert.equal(await page.locator("#approve").isDisabled(),false);
  assert.equal(await page.locator("#discover-schematic").isDisabled(),true);
  assert.equal(await page.locator("#schematic-component").isDisabled(),true);
  await page.locator("#prompt").fill("cambia R1 a 47k");
  await page.locator("#inference-form").dispatchEvent("submit");
  assert.equal(await page.locator("#konnect-value").inputValue(),"12.34k");
  assert.match(await page.locator("#conversation").innerText(),/Cancela o termina la propuesta/);
  const konnectRequest=await page.evaluate(()=>window.requests.find(r=>r.command==="prepare_discovered_schematic"));
  assert.equal(konnectRequest.selection,"selected-schematic");assert.equal(konnectRequest.reference,"R1");assert.equal(konnectRequest.value,"12.34k");
  await page.locator("#approve").click();assert.equal(await page.locator("#artifacts button").count(),4);
  await page.locator("#artifacts button").last().click();await page.locator("#artifact-preview").waitFor({state:"visible"});
  assert.match(await page.locator("#artifact-preview-heading").innerText(),/Konnect/);assert.equal(await page.locator("#artifact-source img").count(),0);await page.keyboard.press("Escape");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_inference").length),modelCalls);
  await page.screenshot({path:path.join(evidence,"konnect-workflow.png"),fullPage:true});
  await page.locator("#agent-tools-panel > summary").click();
  assert.equal(await page.locator("#disable-agent-tools").isDisabled(),true);
  await page.locator("#enable-agent-tools").click();
  assert.equal(await page.locator("#disable-agent-tools").isDisabled(),false);
  await page.locator("#agent-tools-config > summary").click();
  assert.match(await page.locator("#claude-tools-config").innerText(),/mcpServers/);
  assert.match(await page.locator("#codex-tools-config").innerText(),/mcp_servers.hazlomexa.env/);
  assert.match(await page.locator("#agent-tools-status").innerText(),/pendientes de tu aprobación/);
  for (const provider of ["claude", "codex"]) {
    await page.locator("#assistant-provider").selectOption(provider);
    const approvals = await page.evaluate(()=>window.requests.filter(r=>r.command==="approve_operation").length);
    await page.evaluate(provider=>{window.mockExternalProposal();window.mockBusyChat(provider);},provider);
    await page.waitForFunction(()=>document.querySelector("#operation-preview").textContent.includes("22k") && document.querySelector("#claude-chat-progress").textContent.includes("Conversando"));
    assert.equal(await page.locator("#approve").isDisabled(),false,"Human approval must remain available while agent chat streams");
    for (const selector of ["#workflow", "#assistant-provider", "#open-project", "#konnect-value"]) assert.equal(await page.locator(selector).isDisabled(),true);
    assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="approve_operation").length),approvals);
    await page.locator("#approve").click();
    assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="approve_operation").length),approvals+1);
    assert.equal(await page.locator("#assistant-provider").isDisabled(),true);
    await page.evaluate(()=>window.mockBusyChat(null));
    await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  }
  await page.locator("#assistant-provider").selectOption("guided");
  await page.evaluate(()=>window.mockToolHistory());
  await page.waitForFunction(()=>document.querySelector("#history").textContent.includes("Propuesta por herramienta · MCP"));
  await page.locator("#disable-agent-tools").click();
  assert.equal(await page.locator("#agent-tools-config").isVisible(),false);
  assert.equal(await page.locator("#claude-tools-config").textContent(),"");
  assert.equal(await page.locator("#codex-tools-config").textContent(),"");
  await page.locator("#workflow").selectOption("pcb");
  assert.equal(await page.locator("#pcb-form").isVisible(),true);
  await page.locator("#prompt").fill("Quiero reorganizar mi placa pero no sé los nombres de los componentes");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/sin conocer identificadores|ver sus componentes/);
  await page.locator("#discover-pcb").click();
  assert.match(await page.locator("#pcb-component").innerText(),/Punto de prueba/);
  for(const namespace of ["constructor","__proto__","toString"]) assert((await page.locator("#pcb-component").innerText()).includes(`${namespace}:Custom · custom`));
  assert.match(await page.locator("#pcb-component").innerText(),/cara inferior/);
  assert.equal(await page.locator("#pcb-form img").count(),0);assert.equal(await page.evaluate(()=>window.pcbInjection),undefined);
  await page.locator("#pcb-component").selectOption("1390739f-0b91-4d50-b8ae-92acd725ec51");
  assert.match(await page.locator("#pcb-current").innerText(),/200\.8/);
  assert.equal(await page.locator("#pcb-x").inputValue(),"");
  const pcbGuideBefore=await page.evaluate(()=>window.requests.length);
  await page.locator("#prompt").fill("Muévelo a la derecha");await page.locator("#infer").click();
  assert.match(await page.locator("#conversation").innerText(),/Qué distancia/);
  await page.locator("#prompt").fill("No sé cuánto");await page.locator("#infer").click();
  assert.equal(await page.locator("#pcb-x").inputValue(),"");
  await page.locator("#prompt").fill("0,1 pulgadas");await page.locator("#infer").click();
  assert.equal(Number(await page.locator("#pcb-x").inputValue()),203.34);
  assert.equal(Number(await page.locator("#pcb-y").inputValue()),156.6);
  assert.equal(await page.evaluate(()=>window.requests.length),pcbGuideBefore,'Guided movement must only update the draft');
  await page.screenshot({path:path.join(evidence,"pcb-guided-move.png"),fullPage:true});
  for(const reset of ["component","conversation","workflow","provider","source"]){
    await page.locator("#prompt").fill("Move it left");await page.locator("#infer").click();
    if(reset==="component"){
      await page.locator("#pcb-component").selectOption("2390739f-0b91-4d50-b8ae-92acd725ec51");
      await page.locator("#pcb-component").selectOption("1390739f-0b91-4d50-b8ae-92acd725ec51");
    } else if(reset==="conversation")await page.locator("#new-conversation").click();
    else if(reset==="workflow"){
      await page.locator("#workflow").selectOption("drawing");await page.locator("#workflow").selectOption("pcb");
    } else if(reset==="provider"){
      await page.locator("#assistant-provider").selectOption("codex");await page.locator("#assistant-provider").selectOption("guided");
    } else {
      await page.locator("#discover-pcb").click();await page.locator("#pcb-component").selectOption("1390739f-0b91-4d50-b8ae-92acd725ec51");
    }
    const before=await page.locator("#pcb-x").inputValue();
    await page.locator("#prompt").fill("2 mm");await page.locator("#infer").click();
    assert.equal(await page.locator("#pcb-x").inputValue(),before,`${reset} retained a pending movement question`);
  }
  await page.locator("#pcb-unit").selectOption("in");await page.locator("#pcb-x").fill("8");await page.locator("#pcb-y").fill("6");
  await page.locator("#pcb-unit").selectOption("mils");assert.equal(Number(await page.locator("#pcb-x").inputValue()),8000);assert.equal(Number(await page.locator("#pcb-y").inputValue()),6000);
  await page.locator("#prepare-pcb").click();
  assert.match(await page.locator("#operation-preview").innerText(),/203\.2/);
  assert.match(await page.locator("#approval-note").innerText(),/rellena zonas/);
  assert.equal(await page.locator("#pcb-unit").isDisabled(),true);
  const pcbRequest=await page.evaluate(()=>window.requests.find(request=>request.command==="prepare_pcb_move"));assert.equal(pcbRequest.units,"mils");assert.equal(pcbRequest.x,8000);
  await page.locator("#approve").click();
  assert.match(await page.locator("#validation-result").innerText(),/comprobaciones desactivadas DRC/);
  assert.equal(await page.locator("#artifacts").getByRole("button",{name:"Mostrar PCB guardada y revisada"}).count(),1);
  await page.screenshot({path:path.join(evidence,"pcb-workflow.png"),fullPage:true});
  await page.locator("#assistant-provider").selectOption("claude");
  assert.equal(await page.locator("#infer").isDisabled(),true);
  await page.locator("#select-claude").click();
  await page.locator("#prompt").fill("Quiero crear una pieza pero no conozco sus medidas");await page.locator("#infer").click();
  assert.equal(await page.locator("#assistant-provider").isDisabled(),true);
  await page.waitForFunction(()=>document.querySelector("#conversation").textContent.includes("¿Para qué la usarás?"));
  assert.equal(await page.locator("#conversation img").count(),0);assert.equal(await page.evaluate(()=>window.chatInjection),undefined);
  await page.locator("#prompt").fill("Para sujetar mi teléfono");await page.locator("#infer").click();
  await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  const chatRequests=await page.evaluate(()=>window.requests.filter(r=>r.command==="start_claude_chat"));
  assert.equal(chatRequests[0].newSession,true);assert.equal(chatRequests[1].newSession,false);
  const projectBeforeRestart=await page.evaluate(async()=> (await window.__TAURI_INTERNALS__.invoke("get_snapshot")).project);
  await page.evaluate(()=>sessionStorage.clear());
  await page.reload();
  if(projectBeforeRestart[0]!=="test-project")await page.locator("#open-project").click();
  await page.locator("#assistant-provider").selectOption("claude");
  assert.match(await page.locator("#conversation").innerText(),/Para sujetar mi teléfono/);
  assert.equal(await page.locator("#conversation img").count(),0);
  await page.locator("#select-claude").click();
  await page.locator("#prompt").fill("Ahora quiero modificarlo");await page.locator("#infer").click();
  await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  const restarted=await page.evaluate(()=>window.requests.filter(r=>r.command==="start_claude_chat").at(-1));
  assert.equal(restarted.newSession,true);
  const restoredContext=JSON.parse(restarted.prompt);
  assert(restoredContext.recovered_conversation.some(t=>t.content==="Para sujetar mi teléfono"));
  assert.equal(restoredContext.user_request,"Ahora quiero modificarlo");
  await page.screenshot({path:path.join(evidence,"conversation-recovered.png"),fullPage:true});
  await page.locator("#manual-dimensions > summary").click();
  await page.locator("#length").fill("15");
  await page.evaluate(()=>{
    document.querySelector("#cadpilot-operation").value="add_cylinder";
    for(const key of ["object","length","width","height","diameter","x","y","z"])document.querySelector(`#cadpilot-${key}`).value=key==="object" ? "OldSource" : "15";
  });
  await page.locator("#agent-tools-panel > summary").click();
  await page.evaluate(()=>window.setProjectFolder("C:\\copied-project-same-id"));await page.locator("#enable-agent-tools").click();
  for(const id of ["length","width","height"])assert.equal(await page.locator(`#${id}`).inputValue(),"","Copied folders must not retain another folder's draft measurements");
  for(const key of ["object","length","width","height","diameter","x","y","z"])assert.equal(await page.locator(`#cadpilot-${key}`).inputValue(),"","Copied folders must clear CADPilot's source-bound inputs");
  assert.equal(await page.locator("#cadpilot-operation").inputValue(),"inspect");
  assert.doesNotMatch(await page.locator("#conversation").innerText(),/Para sujetar mi teléfono/);
  await page.evaluate(folder=>window.setProjectFolder(folder),projectBeforeRestart[1]);await page.locator("#enable-agent-tools").click();
  assert.equal(await page.locator("#length").inputValue(),"","Unconfirmed draft dimensions are not restored from conversation text");
  assert.match(await page.locator("#conversation").innerText(),/Para sujetar mi teléfono/);
  await page.locator("#new-conversation").click();
  await page.locator("#assistant-provider").selectOption("codex");
  assert.doesNotMatch(await page.locator("#conversation").innerText(),/Para sujetar mi teléfono/);
  await page.locator("#assistant-provider").selectOption("claude");
  assert.doesNotMatch(await page.locator("#conversation").innerText(),/Para sujetar mi teléfono/);
  await page.locator("#prompt").fill("Quiero empezar otra pieza");await page.locator("#infer").click();
  await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="start_claude_chat").at(-1).newSession),true);
  await page.locator("#prompt").fill("Quiero aclarar otra medida");await page.locator("#infer").click();
  await page.locator("#cancel-inference").click();
  await page.waitForFunction(()=>document.querySelector("#conversation").textContent.includes("Solicitud detenida"));
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="cancel_chat").length),1);
  await page.locator("#workflow").selectOption("freecad");
  assert.equal((await page.locator("#conversation").textContent()).includes("Solicitud detenida"),true);
  await page.locator("#assistant-provider").selectOption("codex");
  assert.equal(await page.locator("#infer").isDisabled(),true);
  assert.equal(await page.locator("#claude-controls").isHidden(),true);
  await page.locator("#select-codex").click();
  assert.match(await page.locator("#codex-runtime").innerText(),/0\.159\.2/);
  await page.locator("#connect-codex").click();
  assert.equal(await page.locator("#assistant-provider").isDisabled(),true);
  assert.equal(await page.locator("#infer").isDisabled(),true);
  assert.equal(await page.locator("#open-codex-login").isDisabled(),false,"Early login event lost to the older command response");
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="open_codex_login").length),0,"Login opened without a user click");
  await page.locator("#open-codex-login").click();
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="open_codex_login").length),1);
  await page.locator("#cancel-codex-login").click();
  assert.equal(await page.locator("#assistant-provider").isDisabled(),false,"Older cancellation response replaced the completed connection event");
  assert.equal(await page.locator("#open-codex-login").isDisabled(),true);
  assert.match(await page.locator("#codex-login-status").innerText(),/detenida/);
  await page.locator("#connect-codex").click();
  await page.evaluate(()=>window.sendOldCodexLogin());
  assert.equal(await page.locator("#infer").isDisabled(),true,"Old connection event enabled a new session");
  await page.evaluate(()=>window.finishCodexLogin());
  assert.equal(await page.locator("#infer").isDisabled(),false);
  await page.locator("#prompt").fill("No sé las medidas; quiero empezar por lo que necesito crear");await page.locator("#infer").click();
  await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  assert.match(await page.locator("#conversation").innerText(),/¿Para qué la usarás/);
  assert.equal(await page.locator("#conversation img").count(),0);
  await page.locator("#workflow").selectOption("cadpilot");
  assert.match(await page.locator("#conversation").innerText(),/¿Para qué la usarás/);
  await page.locator("#prompt").fill("Quiero cambiar la pieza que seleccioné");await page.locator("#infer").click();
  await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
  const codexRequests=await page.evaluate(()=>window.requests.filter(r=>r.command==="start_codex_chat"));
  assert.equal(codexRequests[0].newSession,true);assert.equal(codexRequests[1].newSession,false);
  await page.evaluate(()=>{window.askCodexQuestion=true;});
  await page.locator("#prompt").fill("Quiero una pieza que encaje con mi teléfono");await page.locator("#infer").click();
  await page.locator("#chat-question-form").waitFor({state:"visible"});
  assert.equal(await page.locator("#chat-question-form img").count(),0);
  assert.equal(await page.locator("#chat-answer-0").inputValue(),"","Question preselected an answer");
  assert.equal(await page.locator("#chat-options-1 option").count(),2);
  await page.locator("#chat-answer-0").selectOption("in");await page.locator("#chat-answer-1").fill("Sostener mi teléfono");
  await page.waitForTimeout(700);
  assert.equal(await page.locator("#chat-answer-1").inputValue(),"Sostener mi teléfono","Polling erased the human answer");
  await page.locator("#answer-chat-question").click();
  await page.locator("#chat-question-form").waitFor({state:"hidden"});
  assert.match(await page.locator("#conversation").innerText(),/Usaremos pulgadas/);
  assert.match(await page.locator("#conversation").innerText(),/Sostener mi teléfono/);
  const answer=await page.evaluate(()=>window.requests.find(r=>r.command==="answer_codex_question"));assert.deepEqual(answer.answers,{units:"in",purpose:"Sostener mi teléfono"});
  await page.locator("#prompt").fill("Aclara otro detalle");await page.locator("#infer").click();await page.locator("#chat-question-form").waitFor({state:"visible"});
  await page.locator("#cancel-inference").click();await page.locator("#chat-question-form").waitFor({state:"hidden"});
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="answer_codex_question").length),1,"Cancellation submitted an answer");
  await page.evaluate(()=>{window.askCodexQuestion=false;});
  await page.evaluate(run=>window.addHistory(run),pcbHistory);
  await page.locator("#enable-agent-tools").click();
  await page.waitForFunction(()=>document.querySelector("#history").textContent.includes("Mostrar PCB guardada y revisada"));
  assert.match(await page.locator("#history").innerText(),/203\.34, 155\.33/);
  assert.match(await page.locator("#history").innerText(),/Ver errores y advertencias DRC/);
  const pcbRow=page.locator("#history .artifact-row").filter({hasText:"Mostrar PCB guardada y revisada"});
  assert.equal(await pcbRow.getByRole("button",{name:"Mostrar netlist KiCad"}).count(),0);
  await pcbRow.getByRole("button",{name:"Ver errores y advertencias DRC"}).click();
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="open_artifact").at(-1).format),"kicad_report");
  await page.keyboard.press("Escape");
  await pcbRow.getByRole("button",{name:"Ver hallazgos",exact:true}).click();
  assert.match(await page.locator("#artifact-preview-heading").innerText(),/Hallazgos DRC · informe mils/);
  assert.match(await page.locator("#artifact-source").innerText(),/1 errores/);
  await page.keyboard.press("Escape");
  await page.locator("#disable-agent-tools").click();
  await page.locator("#prompt").fill("Quiero detener esta aclaración");await page.locator("#infer").click();await page.locator("#cancel-inference").click();
  await page.waitForFunction(()=>document.querySelector("#conversation").textContent.includes("Solicitud detenida"));
  const contexts=[];
  for(const provider of ["claude","codex"]){
    await page.locator("#assistant-provider").selectOption(provider);
    for(const workflow of ["freecad","arduino","agent","drawing","konnect","pcb","kicad","slice","cadpilot"]){
      await page.locator("#workflow").selectOption(workflow);
      await page.locator("#new-conversation").click();
      if(workflow==="konnect")await page.locator("#konnect-value").fill("  Mixed Case Ω  ");
      const expected=await page.evaluate(()=>({fields:Object.fromEntries([...document.querySelectorAll('input,select')].map(field=>[field.id,field.type==="checkbox" ? field.checked : field.value.trim() ? field.value : null]))}));
      await page.locator("#prompt").fill("Ayúdame a confirmar lo que falta");await page.locator("#infer").click();
      await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
      const request=await page.evaluate(command=>window.requests.filter(r=>r.command===command).at(-1),`start_${provider}_chat`);
      const payload=JSON.parse(request.prompt), context=payload.desktop_context;
      assert.equal(payload.user_request,"Ayúdame a confirmar lo que falta");
      assert.equal(context.workflow,workflow);assert.equal(context.source_is_untrusted_data,true);
      assert(Object.keys(context.draft_fields).length || workflow==="agent");
      for(const [id,value] of Object.entries(context.draft_fields))assert.equal(value,expected.fields[id],`${provider}/${workflow}/${id} lost a value or replaced unknown input`);
      assert.equal(context.cadpilot_source,undefined); // Earlier project/folder changes cleared the selected base.
      contexts.push({provider,context});
    }
  }
  await page.locator("#cadpilot-operation").selectOption("inspect");
  await page.locator("#prepare-cadpilot").click();await page.locator("#approve").click();
  await page.getByRole("button",{name:"Continuar desde este resultado",exact:true}).first().click();
  await page.locator("#cadpilot-component").selectOption("Other");
  await page.locator("#cadpilot-unit").selectOption("cm");
  await page.locator("#cadpilot-height").fill("");
  const approvalsBefore=await page.evaluate(()=>window.requests.filter(r=>r.command==="approve" || r.command==="prepare_cadpilot_followup").length);
  for(const provider of ["claude","codex"]){
    await page.locator("#assistant-provider").selectOption(provider);
    await page.locator("#prompt").fill("Hazla más alta, todavía no sé cuánto");await page.locator("#infer").click();
    await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
    const payload=JSON.parse(await page.evaluate(command=>window.requests.filter(r=>r.command===command).at(-1).prompt,`start_${provider}_chat`));
    const context=payload.desktop_context;
    assert.equal(context.workflow,"cadpilot");assert.match(context.cadpilot_source.base_job_id,/^pilot-job-/);
    assert.equal(context.cadpilot_source.result_sha256,"e".repeat(64));
    assert.equal(context.draft_fields["cadpilot-component"],"Other");
    assert.equal(context.draft_fields["cadpilot-length"],"0.2");assert.equal(context.draft_fields["cadpilot-width"],"0.3");
    assert.equal(context.draft_fields["cadpilot-height"],null);assert.equal(context.draft_fields["cadpilot-unit"],"cm");
    contexts.push({provider,context});
  }
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="approve" || r.command==="prepare_cadpilot_followup").length),approvalsBefore,"Sending selection context approved or dispatched a CAD edit");
  await page.locator("#workflow").selectOption("drawing");
  await page.evaluate(()=>window.drawingInsunits=0);
  await page.locator("#select-cadcraft").click();
  await page.locator("#discover-drawing").click();
  const drawingEffectsBefore=await page.evaluate(()=>window.requests.filter(r=>r.command==="approve" || r.command==="prepare_selected_drawing").length);
  for(const provider of ["claude","codex"]){
    await page.locator("#assistant-provider").selectOption(provider);
    await page.locator("#prompt").fill("No sé las medidas, ayúdame a entender este dibujo");
    await page.locator("#infer").click();
    await page.waitForFunction(()=>!document.querySelector("#assistant-provider").disabled);
    const payload=JSON.parse(await page.evaluate(command=>window.requests.filter(r=>r.command===command).at(-1).prompt,`start_${provider}_chat`));
    const context=payload.desktop_context;
    assert.equal(context.workflow,"drawing");
    assert.deepEqual(context.drawing_source,{selection:"selected-drawing",sha256:"a".repeat(64)});
    assert.equal(context.draft_fields["drawing-unit"],null);
    assert.equal(context.draft_fields["drawing-x"],null);
    assert.equal(context.draft_fields["drawing-diameter"],null);
    contexts.push({provider,context});
  }
  assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.command==="approve" || r.command==="prepare_selected_drawing").length),drawingEffectsBefore);
  await page.locator("#open-project").click();
  assert.equal(await page.locator("#prepare-drawing").isDisabled(),true);
  assert.equal(await page.locator("#drawing-unit").inputValue(),"");
  for(const workflow of ["freecad","arduino","agent","drawing","konnect","pcb","kicad","slice","cadpilot"]){
    await page.locator("#workflow").selectOption(workflow);
    assert.equal(await page.locator("#status").innerText(),"Reúne los datos en el chat y prepara una propuesta para revisar.");
  }
  await page.locator("#workflow").selectOption("drawing");
  await page.evaluate(()=>{
    for(const [id,length,created] of [["order-old",31,1000],["order-tie-b",32,2000],["order-legacy",33,null],["order-new",34,3000],["order-tie-a",35,2000]]){
      const run={schema_version:2,project_id:"test-project",input:{job_id:id,dimensions_mm:{length,width:1,height:1}},phase:"completed"};
      if(created!==null)run.audit={source:{provider:"manual"},created_unix_ms:created,updated_unix_ms:created,files_verified:true};
      window.addHistory(run);
    }
  });
  await page.locator("#select-cadcraft").click();
  const ordered=page.locator("#history .artifact-row").filter({hasText:/3[1-5] × 1 × 1 mm/});
  await ordered.first().waitFor();
  assert.deepEqual((await ordered.allTextContents()).map(text=>Number(text.match(/(3[1-5]) × 1 × 1 mm/)[1])),[34,35,32,31,33],"Recovered history is not newest-first with stable ties and undated legacy entries last");
  await page.screenshot({path:path.join(evidence,"history-order.png"),fullPage:true});
  await mkdir(evidence,{recursive:true});
  await import("node:fs/promises").then(fs=>fs.writeFile(path.join(evidence,"agent-desktop-context.json"),JSON.stringify({passed:true,contexts},null,2)));
  assert.deepEqual(errors, []);
  console.log("PASS: nine workflows, desktop selection/draft context for Claude and Codex, durable project/provider/folder chat context, restart rehydration, new-conversation reset, questions/cancellation, approval gates, metric/imperial inputs and escaped previews; browser fixtures only");
} finally { await browser?.close(); await server.close(); }
