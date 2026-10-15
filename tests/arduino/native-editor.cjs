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

test("inspection reads the dirty native buffer and excludes its private URI", async () => {
  const f = fixture(), state = await read(f);
  assert.equal(state.dirty, true); assert.equal(state.source, f.doc.text);
  assert.notEqual(state.source, f.disk); assert.equal(state.name, "Sketch.ino");
  assert.equal(state.state_sha256, crypto.createHash("sha256").update(f.doc.text).digest("hex"));
  assert(!JSON.stringify(state).includes("C:")); assert.equal(f.editor.calls, 0);
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
