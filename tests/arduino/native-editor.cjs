"use strict";
const assert = require("node:assert/strict");
const {test} = require("node:test");
const crypto = require("node:crypto");
const {createEditorTools} = require("../../apps/desktop/src-tauri/resources/arduino-editor/core.cjs");

function fixture(eol = 1) {
  const separator = eol === 1 ? "\n" : "\r\n";
  const disk = "void loop() { delay(1000); }" + separator;
  const doc = {uri:{scheme:"file", fsPath:"C:\\owned\\Sketch\\Sketch.ino"}, version:3, eol,
    isDirty:true, isClosed:false, text:disk + "// unsaved user note" + separator,
    getText() { return this.text; }, positionAt(offset) { return offset; }};
  const api = {window:{}, Range:class { constructor(start, end) { this.start = start; this.end = end; } }};
  const editor = {document:doc, calls:0, history:[], mode:"normal", hold:null,
    async edit(callback, options) {
      this.calls++;
      assert.deepEqual(options, {undoStopBefore:true, undoStopAfter:true});
      const version = doc.version;
      let replacement;
      callback({replace(range, text) {
        assert.equal(range.start, 0); assert.equal(range.end, doc.text.length); replacement = text;
      }});
      if (this.hold) await this.hold;
      if (this.mode === "late_user_edit") { doc.text += "// user typed during RPC"; doc.version++; }
      if (doc.version !== version) return false;
      this.history.push(doc.text); doc.text = replacement; doc.version++; doc.isDirty = true;
      if (this.mode === "lost_reply") throw Error("private C:/user/path and secret token");
      if (this.mode === "changed_readback") { doc.text += "// another user change"; doc.version++; }
      return true;
    },
    undo() { doc.text = this.history.pop(); doc.version++; }
  };
  api.window.activeTextEditor = editor;
  const tools = createEditorTools(api);
  return {api, doc, editor, tools, disk};
}
async function read(f) {
  const result = await f.tools.request({action:"inspect"});
  assert.equal(result.ok, true); return result.result;
}
function apply(before, source) {
  return {action:"apply", job_id:crypto.randomUUID(), instance_id:before.instance_id,
    document_token:before.document_token, version:before.version, state_sha256:before.state_sha256, source};
}

function save(before) {
  const {source,...request}=apply(before,before.source);
  return {...request,action:"save"};
}
test("native recovery preserves completed edits and later manual changes without replay",async()=>{
  const f=fixture(),before=await read(f),request=apply(before,before.source.replace("1000","500"));
  const done=await f.tools.request(request);assert.equal(done.ok,true);
  const expected=JSON.parse(JSON.stringify(done));done.result.after.source="caller changed its copy";
  f.doc.text+="// later manual change\n";f.doc.version++;const manual=f.doc.text;
  assert.deepEqual(await f.tools.request({action:"recover",original:request}),expected);
  f.doc.isClosed=true;f.api.window.activeTextEditor=undefined;
  assert.deepEqual(await f.tools.request({action:"recover",original:{...request}}),expected);
  assert.equal(f.doc.text,manual);assert.equal(f.editor.calls,1);assert.equal(f.editor.history.length,1);
  for(const field of ["job_id","instance_id","document_token","version","state_sha256","source","action"]){
    const changed={...request,[field]:field==="version"?request.version+1:field==="action"?"save":"other"};
    assert.equal((await f.tools.request({action:"recover",original:changed})).ok,false);
  }
  const restarted=createEditorTools(f.api);assert.equal((await restarted.request({action:"recover",original:request})).ok,false);assert.equal(f.editor.calls,1);
});
test("native save recovery never saves again and rejects reuse of an attempted identity",async()=>{
  const f=fixture(),before=await read(f),request=save(before);let saves=0;
  f.doc.save=async()=>{saves++;f.doc.isDirty=false;return true;};
  const done=await f.tools.request(request);assert.equal(done.ok,true);f.doc.isDirty=true;
  assert.deepEqual(await f.tools.request({action:"recover",original:request}),done);
  assert.equal((await f.tools.request(request)).outcome,"refused");assert.equal(f.doc.isDirty,true);assert.equal(saves,1);assert.equal(f.editor.calls,0);
});
test("running and uncertain attempts never become recovered completion",async()=>{
  const f=fixture(),before=await read(f),request=apply(before,before.source.replace("1000","500"));let release;
  f.editor.hold=new Promise(resolve=>{release=resolve;});const pending=f.tools.request(request);
  assert.deepEqual(await f.tools.request({action:"recover",original:request}),{ok:true,result:{editor_recovery_version:1,status:"running"}});
  assert.equal((await f.tools.request(request)).outcome,"refused");release();assert.equal((await pending).ok,true);assert.equal(f.editor.calls,1);
  const bad=fixture(),initial=await read(bad),uncertain=apply(initial,initial.source.replace("1000","500"));bad.editor.mode="lost_reply";
  assert.equal((await bad.tools.request(uncertain)).outcome,"uncertain");assert.equal((await bad.tools.request({action:"recover",original:uncertain})).ok,false);assert.equal((await bad.tools.request(uncertain)).outcome,"refused");assert.equal(bad.editor.calls,1);
});
test("native receipt quota refuses before effects and preserves earlier completed work",async()=>{
  const f=fixture();let first,done;
  for(let i=0;i<128;i++){
    const before=await read(f),request=apply(before,before.source+`// change ${i}\n`),reply=await f.tools.request(request);assert.equal(reply.ok,true);if(i===0){first=request;done=reply;}
  }
  const before=await read(f),request=apply(before,before.source+"// not admitted\n");assert.equal((await f.tools.request(request)).outcome,"refused");assert.equal(f.editor.calls,128);assert.equal(f.doc.text,before.source);assert.deepEqual(await f.tools.request({action:"recover",original:first}),done);
});
test("explicit native save retains the exact buffer and edit history", async()=>{
  const f=fixture(),before=await read(f);let saves=0;
  f.doc.save=async()=>{saves++;f.doc.isDirty=false;return true;};
  const result=await f.tools.request(save(before));
  assert.equal(result.ok,true);assert.equal(result.result.native_save_confirmed,true);
  assert.equal(result.result.native_edit_confirmed,false);assert.equal(result.result.after.source,before.source);
  assert.equal(result.result.after.version,before.version);assert.equal(result.result.after.dirty,false);
  assert.equal(saves,1);assert.equal(f.editor.calls,0);assert.deepEqual(f.editor.history,[]);
  assert.equal((await f.tools.request(save(before))).outcome,"refused");assert.equal(saves,1);
});
test("stale or path-bearing save requests never invoke native save", async()=>{
  for(const field of ["instance_id","document_token","version","state_sha256","path"]){
    const f=fixture(),before=await read(f);let saves=0;
    f.doc.save=async()=>{saves++;return true;};
    const request=save(before);request[field]=field==="version"?before.version+1:"invalid";
    assert.equal((await f.tools.request(request)).outcome,"refused");assert.equal(saves,0);
  }
});
test("save waits for a delayed clean notification without resaving and blocks another edit", async()=>{
  const f=fixture(),before=await read(f);let saves=0,release;
  const hold=new Promise(resolve=>{release=resolve;});
  f.doc.save=async()=>{saves++;await hold;setTimeout(()=>{f.doc.isDirty=false;},40);return true;};
  const pending=f.tools.request(save(before));
  assert.equal((await f.tools.request(apply(before,before.source.replace("1000","500")))).outcome,"refused");
  release();const result=await pending;
  assert.equal(result.ok,true);assert.equal(result.result.after.dirty,false);assert.equal(saves,1);assert.equal(f.editor.calls,0);
});
test("save failure, concurrent edits or an unconfirmed clean state remain uncertain without retries", async()=>{
  for(const mode of ["false","throw","dirty","edit","rename"]){
    const f=fixture(),before=await read(f);let saves=0;
    f.doc.save=async()=>{
      saves++;if(mode==="throw")throw Error("private save detail");if(mode==="false")return false;
      if(mode!=="dirty")f.doc.isDirty=false;
      if(mode==="edit"){f.doc.text+="// user edit";f.doc.version++;}
      if(mode==="rename")f.doc.uri.fsPath="C:\\owned\\Another.ino";
      return true;
    };
    const result=await f.tools.request(save(before));
    assert.equal(result.outcome,"uncertain");assert.equal(saves,1);assert.equal(f.editor.calls,0);
    assert(!JSON.stringify(result).includes("private save detail"));
  }
});

test("inspection reads the dirty native buffer and excludes its private URI", async () => {
  const f = fixture(), state = await read(f);
  assert.equal(state.dirty, true); assert.equal(state.source, f.doc.text);
  assert.notEqual(state.source, f.disk); assert.equal(state.name, "Sketch.ino");
  assert.equal(state.state_sha256, crypto.createHash("sha256").update(f.doc.text).digest("hex"));
  assert(!JSON.stringify(state).includes("C:")); assert.equal(f.editor.calls, 0);
});

test("native readback waits for a delayed document notification without resending the edit", async () => {
  const f=fixture(), before=await read(f), source=before.source.replace("1000","500");
  f.editor.edit=async callback=>{
    f.editor.calls++;let replacement;
    callback({replace(range,text){replacement=text;}});
    setTimeout(()=>{f.doc.text=replacement;f.doc.version++;},40);
    return true;
  };
  const pending=f.tools.request(apply(before,source));
  const concurrent=await f.tools.request(apply(before,source));
  assert.equal(concurrent.outcome,"refused");
  const result=await pending;assert.equal(result.ok,true);assert.equal(result.result.after.source,source);
  assert.equal(f.editor.calls,1);assert(result.result.after.version>before.version);
});

test("an acknowledgement without any native source change stays uncertain and is never retried", async () => {
  const f=fixture(), before=await read(f);
  f.editor.edit=async ()=>{f.editor.calls++;return true;};
  const result=await f.tools.request(apply(before,before.source.replace("1000","500")));
  assert.equal(result.outcome,"uncertain");assert.equal(f.editor.calls,1);assert.equal(f.doc.text,before.source);
});

test("a renamed buffer or changed line ending after delivery makes the edit uncertain", async () => {
  for(const field of ["name","eol"]){
    const f=fixture(), before=await read(f), edit=f.editor.edit.bind(f.editor);
    f.editor.edit=async (...args)=>{
      const result=await edit(...args);
      if(field==="name")f.doc.uri.fsPath="C:\\owned\\Other.ino";else f.doc.eol=2;
      return result;
    };
    const result=await f.tools.request(apply(before,before.source.replace("1000","500")));
    assert.equal(result.outcome,"uncertain");assert.equal(f.editor.calls,1);
  }
});
test("two edits and native undo preserve the preexisting unsaved note", async () => {
  const f = fixture(), original = f.doc.text;
  const first = await f.tools.request(apply(await read(f), original.replace("1000", "500")));
  assert.equal(first.ok, true); assert.equal(first.result.native_edit_confirmed, true);
  const second = await f.tools.request(apply(first.result.after, f.doc.text.replace("500", "250")));
  assert.equal(second.ok, true); assert(f.doc.text.includes("unsaved user note"));
  assert.equal(f.editor.calls, 2); assert.equal(second.result.after.dirty, true);
  f.editor.undo(); assert.equal(f.doc.text, first.result.after.source);
  f.editor.undo(); assert.equal(f.doc.text, original);
  assert.equal(f.disk, "void loop() { delay(1000); }\n");
});
test("stale instance, document, version or content hash never invokes native editing", async () => {
  const f = fixture(), before = await read(f);
  for (const [key, value] of [["instance_id", crypto.randomUUID()], ["document_token", crypto.randomUUID()],
      ["version", 2], ["state_sha256", "b".repeat(64)]]) {
    const result = await f.tools.request({...apply(before, "new source"), [key]:value});
    assert.equal(result.outcome, "refused");
  }
  assert.equal(f.editor.calls, 0);
});
test("a reopened document at the same URI receives a new native identity", async () => {
  const f = fixture(), before = await read(f);
  f.doc.isClosed = true;
  f.api.window.activeTextEditor = {document:{...f.doc, isClosed:false}, edit() { throw Error("must not edit"); }};
  assert.notEqual((await read(f)).document_token, before.document_token);
  assert.equal((await f.tools.request(apply(before, "new source"))).outcome, "refused");
});
test("paths, arbitrary actions, no-ops, oversized UTF-8 and incompatible line endings are refused", async () => {
  const f = fixture(), before = await read(f);
  for (const request of [{...apply(before, "new"), path:"C:/another/Sketch.ino"},
      {...apply(before, "new"), action:"upload"}, apply(before, before.source),
      apply(before, "á".repeat(32769)), apply(before, "nul\0source"), apply(before, "wrong\r\nline")]) {
    assert.equal((await f.tools.request(request)).outcome, "refused");
  }
  assert.equal(f.editor.calls, 0);
});
test("the native version check refuses edits arriving after user typing", async () => {
  const f = fixture(), before = await read(f);
  f.editor.mode = "late_user_edit";
  const result = await f.tools.request(apply(before, "approved source"));
  assert.equal(result.outcome, "refused"); assert(f.doc.text.includes("user typed during RPC"));
  assert.equal(f.editor.history.length, 0); assert(!f.doc.text.includes("approved source"));
});
test("a lost native reply remains uncertain without retry or rollback", async () => {
  const f = fixture(), before = await read(f);
  f.editor.mode = "lost_reply";
  const result = await f.tools.request(apply(before, "approved source"));
  assert.deepEqual(result, {ok:false, outcome:"uncertain", error:"native_editor_not_confirmed"});
  assert.equal(f.doc.text, "approved source"); assert.equal(f.editor.calls, 1);
  assert.equal(f.editor.history[0], before.source);
});
test("concurrent requests cannot clear another edit's busy state", async () => {
  const f = fixture(), before = await read(f);
  let release; f.editor.hold = new Promise(resolve => { release = resolve; });
  const first = f.tools.request(apply(before, "first source"));
  assert.equal((await f.tools.request(apply(before, "second source"))).outcome, "refused");
  assert.equal((await f.tools.request(apply(before, "third source"))).outcome, "refused");
  assert.equal(f.editor.calls, 1); release(); assert.equal((await first).ok, true);
});
test("a user change during readback stays intact and makes the outcome uncertain", async () => {
  const f = fixture(), before = await read(f); f.editor.mode = "changed_readback";
  const result = await f.tools.request(apply(before, "approved source"));
  assert.equal(result.outcome, "uncertain"); assert(f.doc.text.includes("another user change"));
  assert.equal(f.editor.calls, 1);
});
test("CRLF is explicit and clearing a sketch is a normal native edit", async () => {
  const f = fixture(2), before = await read(f);
  assert.equal((await f.tools.request(apply(before, "wrong\nline"))).outcome, "refused");
  const result = await f.tools.request(apply(before, ""));
  assert.equal(result.ok, true); assert.equal(result.result.after.source, "");
  assert.equal(f.editor.calls, 1); f.editor.undo(); assert.equal(f.doc.text, before.source);
});
