import assert from "node:assert/strict";
import {createServer} from "vite";
import {chromium} from "playwright-core";
import {fileURLToPath} from "node:url";
const server=await createServer({root:fileURLToPath(new URL("..",import.meta.url)),server:{host:"127.0.0.1",port:0}});
let browser;
try {
  await server.listen();
  browser=await chromium.launch({executablePath:process.env.HAZLOMEXA_TEST_BROWSER??"C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",headless:true});
  const page=await browser.newPage();
  const errors=[];page.on("pageerror",error=>errors.push(error.message));
  await page.addInitScript(()=>{
    const state={project:["project-image","owned-project"],job:null,recovered:[],inference:null,freecad:"",freecad_available:false,codex:{version:"fixture"},codex_login:{id:"login",sequence:1,connected:true,busy:false},claude:{version:"fixture"}};
    window.calls=[];
    window.makePending=()=>state.job={snapshot:{schema_version:17,project_id:state.project[0],phase:"awaiting_approval",input:{job_id:"native-image-job",native_tools_version:1,program:"freecad",description:"Cambio acordado",calls:[],readback:{tool:"inspect",arguments:{}}}},approval_id:"fixture-approval",busy:false,sequence:1,error:null};
    window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(command,args)=>{
      if((command==="get_snapshot" || command==="detect_clients"))return structuredClone(state);
      if(command.startsWith("start_")) {
        window.calls.push({command,args});
        state.chat={provider:command==="start_codex_chat"?"codex":command==="start_claude_chat"?"claude":args.profile==="cloud"?"ollama_cloud":"ollama_local",id:crypto.randomUUID(),project_id:state.project[0],sequence:1,busy:false,cancelled:false,error:null,text:"Veo una pieza; falta una medida conocida para recrearla."};
        return structuredClone(state);
      }
      if(command==="approve_operation") {
        state.job.snapshot.phase="completed";state.job.approval_id=null;state.job.sequence++;
        return structuredClone(state.job);
      }
      throw Error(`Unexpected fixture IPC: ${command}`);
    }};
  });
  await page.goto(server.resolvedUrls.local[0]);
  try {await page.locator("#send:enabled").waitFor();} catch(error) {throw Error(JSON.stringify({page_errors:errors,fixture_ui:await page.locator("body").innerText()}),{cause:error});}
  const original=await page.evaluate(async()=>{
    const canvas=document.createElement("canvas");canvas.width=2400;canvas.height=1200;
    const ctx=canvas.getContext("2d");ctx.fillStyle="white";ctx.fillRect(0,0,2400,1200);ctx.fillStyle="blue";ctx.fillRect(300,300,1800,600);
    const bytes=new Uint8Array(await(await new Promise(r=>canvas.toBlob(r,"image/jpeg"))).arrayBuffer());
    const metadata=new TextEncoder().encode("Exif\0\0private GPS reference and private original name");
    const segment=new Uint8Array(metadata.length+4);segment.set([255,225,(metadata.length+2)>>8,(metadata.length+2)&255]);segment.set(metadata,4);
    const result=new Uint8Array(bytes.length+segment.length);result.set(bytes.slice(0,2));result.set(segment,2);result.set(bytes.slice(2),segment.length+2);
    return Array.from(result);
  });
  const attach=()=>page.locator("#image-reference").setInputFiles({name:"private-original.jpg",mimeType:"image/jpeg",buffer:Buffer.from(original)});
  await page.locator("#setup").click();
  for(const provider of ["codex","claude","ollama_local","ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    assert.equal(await page.locator("#reference-preview").isVisible(),false,"Provider switch retained a reference for another service");
    await attach();await page.locator("#reference-preview:visible").waitFor();await page.locator("#send:enabled").waitFor();
    await page.locator("#prompt").fill("Identifica esta pieza y ayúdame a recrearla.");await page.locator("#send").click();await page.locator("#send:enabled").waitFor();
    const sent=await page.evaluate(()=>window.calls.at(-1));
    assert.equal(sent.args.image.media_type,"image/jpeg");assert(Buffer.from(sent.args.image.data,"base64").length<=512*1024);
    assert(!Buffer.from(sent.args.image.data,"base64").includes(Buffer.from("private GPS reference")),"Original metadata reached the provider payload");
    assert(!JSON.stringify(sent.args).includes("private-original.jpg"));
    const prompt=JSON.parse(sent.args.prompt);assert.equal(prompt.desktop_context.reference_image_attached,true);assert.match(prompt.instruction,/foto sin escala no da medidas exactas/);
    assert(!JSON.stringify(await page.evaluate(()=>Object.values(localStorage))).includes(sent.args.image.data),"Image bytes entered saved chat history");
    assert.equal(await page.locator("#reference-preview").evaluate(img=>Math.max(img.naturalWidth,img.naturalHeight)),1536);
  }
  await page.locator("#program").selectOption("freecad");
  await page.evaluate(()=>window.makePending());
  await page.locator("#prompt").fill("Muestra el cambio acordado.");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();
  await page.locator("#approve").click();await page.locator("#send:enabled").waitFor();
  const review=await page.evaluate(()=>window.calls.at(-1));assert.equal(review.args.image,undefined,"Automatic result review resent a private image");assert.equal(JSON.parse(review.args.prompt).desktop_context.reference_image_attached,false);
  await page.locator("#new-chat").click();assert.equal(await page.locator("#reference-preview").isVisible(),false);
  await page.locator("#image-reference").setInputFiles({name:"bad.png",mimeType:"image/png",buffer:Buffer.from("not an image")});
  await page.locator("#error:visible").waitFor();assert.equal(await page.locator("#reference-preview").isVisible(),false);
  console.log("PASS: four-provider image dispatch fixtures, scale clarification, bounded normalization, EXIF/name removal, no image in saved chat, provider/new-chat clearing, malformed refusal and no automatic review retransmission; no real inference");
} finally {await browser?.close();await server.close();}
