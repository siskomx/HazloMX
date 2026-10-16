"use strict";
const api = require("vscode");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");

async function connect(root, expected) {
  for (let attempt = 0; attempt < 100; attempt++) {
    const names = fs.readdirSync(root).filter(name => /^[0-9a-f-]{36}\.json$/.test(name));
    assert(names.length <= 64, "Too many owned companion registrations");
    const matches = await Promise.all(names.map(async name => { try {
      const connection = JSON.parse(fs.readFileSync(path.join(root, name), "utf8"));
      assert.equal(connection.pipe, "\\\\.\\pipe\\hazlomexa-arduino-" + connection.instance_id);
      assert.equal(name, connection.instance_id + ".json");
      const client = {request(input) { return new Promise((resolve, reject) => {
        const socket = net.connect(connection.pipe), chunks = [];
        let size = 0;
        socket.setTimeout(input.action === "inspect" ? 1000 : 15000, () => socket.destroy(Error("Native companion reply timed out; do not retry an edit")));
        socket.on("error", reject);
        socket.on("connect", () => socket.write(JSON.stringify({token:connection.token, request:input}) + "\n"));
        socket.on("data", chunk => { if ((size += chunk.length) > 512 * 1024) socket.destroy(Error("Oversized reply")); else chunks.push(chunk); });
        socket.on("end", () => { try { resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))); } catch { reject(Error("Invalid companion reply")); } });
      }); }};
      const read = await client.request({action:"inspect"});
      return read.ok && read.result.name === expected.name && read.result.version === expected.version
        && read.result.source === expected.source ? client : undefined;
    } catch { return undefined; } }));
    const responding = matches.filter(Boolean);
    assert(responding.length <= 1, "The owned sketch must have one matching native companion");
    if (responding.length === 1) return responding[0];
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw Error("Owned native companion did not become ready");
}

function activate(context) {
  const scheduled = setTimeout(() => { void runProbe(); }, 0);
  context.subscriptions.push({dispose() { clearTimeout(scheduled); }});
}

async function runProbe() {
  const root = process.env.HAZLOMEXA_AGENT_EDITOR_PROBE_ROOT;
  if (!root || !/^arduino-native-ui-[0-9a-f-]+$/.test(path.basename(root))) return;
  const marker = JSON.parse(fs.readFileSync(path.join(root, "owned-native-probe.json"), "utf8"));
  if (marker.owned !== true || fs.realpathSync(root).toLowerCase() !== path.resolve(root).toLowerCase()) return;
  const result = {pass:false, stage:"starting", hardware_mutations:false};
  const save = () => fs.writeFileSync(path.join(root, "native-api-result.json"), JSON.stringify(result, null, 2));
  save();
  try {
    const file = path.join(root, "sketchbook", "OwnedSketch", "OwnedSketch.ino");
    const disk = fs.readFileSync(file, "utf8");
    // A loose openTextDocument is read-only when Arduino has another sketch workspace.
    // Use its native sketch opener, which reloads this disposable window and probe.
    if (!api.workspace.workspaceFolders?.some(folder =>
      path.resolve(folder.uri.fsPath).toLowerCase() === path.dirname(file).toLowerCase())) {
      result.stage = "opening owned sketch workspace"; save();
      await api.commands.executeCommand("arduino-open-sketch", api.Uri.file(path.dirname(file)).toString());
      return;
    }
    result.stage = "opening owned sketch"; save();
    const opened = await api.workspace.openTextDocument(api.Uri.file(file));
    await api.window.showTextDocument(opened);
    let editor, version = -1, stable = 0;
    for (let attempt = 0; attempt < 50; attempt++) {
      const current = api.window.activeTextEditor;
      if (current && path.resolve(current.document.uri.fsPath).toLowerCase() === path.resolve(file).toLowerCase()) {
        stable = current.document.version === version ? stable + 1 : 0;
        version = current.document.version;
        if (stable >= 3) { editor = current; break; }
      } else { version = -1; stable = 0; }
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert(editor, "The owned sketch must be active with a stable native revision before fixture setup");
    const doc = editor.document;
    result.api_version = api.version; result.application_name = api.env.appName;
    result.autosave = api.workspace.getConfiguration("files", doc.uri).get("autoSave");
    assert.equal(result.autosave, "off", "Owned fixture must disable ordinary editor autosave");
    const eol = doc.eol === api.EndOfLine.CRLF ? "\r\n" : "\n";
    result.initial_document_version = doc.version;
    result.initial_source_sha256 = crypto.createHash("sha256").update(doc.getText()).digest("hex");
    const expectedSetupSource = doc.getText() + "// unsaved user note" + eol;
    result.initial_insert_confirmed = await editor.edit(edit => edit.insert(doc.positionAt(doc.getText().length), "// unsaved user note" + eol));
    if (result.initial_insert_confirmed) {
      for (let attempt = 0; attempt < 100 && doc.getText() !== expectedSetupSource; attempt++) {
        await new Promise(resolve => setTimeout(resolve, 20));
      }
    }
    result.after_insert_document_version = doc.version;
    result.after_insert_source_sha256 = crypto.createHash("sha256").update(doc.getText()).digest("hex"); save();
    assert.equal(result.initial_insert_confirmed, true, "The IDE must confirm the owned setup edit without retry");
    assert.equal(doc.getText(), expectedSetupSource, "The IDE must deliver the setup source change before the actual companion test");
    const tools = await connect(path.join(root, "profile", ".arduinoIDE", "hazlomexa-editor"), {name:path.basename(file),version:doc.version,source:doc.getText()});
    const inspect = async () => { const reply = await tools.request({action:"inspect"}); assert.equal(reply.ok,true); return reply.result; };
    const apply = async before => {
      const source = before.source.replace(/delay\(\d+\)/, before.source.includes("delay(1000)") ? "delay(500)" : "delay(250)");
      const reply = await tools.request({action:"apply", job_id:crypto.randomUUID(), instance_id:before.instance_id,
        document_token:before.document_token, version:before.version, state_sha256:before.state_sha256, source});
      assert.equal(reply.ok, true, "Native edit/readback must confirm");
      return reply.result.after;
    };
    const original = await inspect();
    assert.equal(original.dirty, true); assert(original.source.includes("unsaved user note"));
    assert.equal(fs.readFileSync(file,"utf8"), disk);
    result.stage = "native edit and undo"; save();
    const beforeUndo = await apply(original);
    const undoTarget = await inspect();
    assert.equal(undoTarget.document_token, original.document_token);
    assert.equal(undoTarget.source, beforeUndo.source);
    await api.commands.executeCommand("core.undo");
    let restored = await inspect();
    for (let attempt = 0; attempt < 100 && restored.document_token === original.document_token && restored.source !== original.source; attempt++) {
      await new Promise(resolve => setTimeout(resolve, 20)); restored = await inspect();
    }
    assert.equal(restored.document_token, original.document_token);
    assert(restored.version > beforeUndo.version);
    assert.equal(restored.source, original.source, "Native undo must preserve the earlier user note");
    result.native_undo_verified = true;
    result.stage = "native edit and follow-up"; save();
    const first = await apply(await inspect()), second = await apply(await inspect());
    assert(first.source.includes("delay(500)") && second.source.includes("delay(250)"));
    assert(second.source.includes("unsaved user note") && second.dirty);
    assert(second.version > first.version);
    assert.equal(fs.readFileSync(file,"utf8"), disk);
    result.original_disk_unchanged = true; result.final_sha256 = second.state_sha256;
    result.authenticated_native_pipe_verified = true;
    result.final_version = second.version; result.final_dirty = second.dirty;
    result.pass = true; result.stage = "completed"; save();
  } catch (error) {
    result.error = String(error.message); save();
  }
}
module.exports = {activate};
