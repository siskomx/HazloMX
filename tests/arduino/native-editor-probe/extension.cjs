"use strict";
const api = require("vscode");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");

async function connect(root) {
  for (let attempt = 0; attempt < 100; attempt++) {
    const names = fs.readdirSync(root).filter(name => /^[0-9a-f-]{36}\.json$/.test(name));
    assert(names.length <= 1, "Only the owned companion may be connected");
    if (names.length) {
      const connection = JSON.parse(fs.readFileSync(path.join(root, names[0]), "utf8"));
      assert.equal(connection.pipe, "\\\\.\\pipe\\hazlomexa-arduino-" + connection.instance_id);
      return {request(input) { return new Promise((resolve, reject) => {
        const socket = net.connect(connection.pipe), chunks = [];
        let size = 0;
        socket.setTimeout(15000, () => socket.destroy(Error("Native companion reply timed out; do not retry an edit")));
        socket.on("error", reject);
        socket.on("connect", () => socket.write(JSON.stringify({token:connection.token, request:input}) + "\n"));
        socket.on("data", chunk => { if ((size += chunk.length) > 512 * 1024) socket.destroy(Error("Oversized reply")); else chunks.push(chunk); });
        socket.on("end", () => { try { resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))); } catch { reject(Error("Invalid companion reply")); } });
      }); }};
    }
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw Error("Owned native companion did not become ready");
}

async function activate() {
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
    result.stage = "opening owned sketch"; save();
    const doc = await api.workspace.openTextDocument(api.Uri.file(file));
    const editor = await api.window.showTextDocument(doc);
    result.api_version = api.version; result.application_name = api.env.appName;
    result.autosave = api.workspace.getConfiguration("files", doc.uri).get("autoSave");
    assert.equal(result.autosave, "off", "Owned fixture must disable ordinary editor autosave");
    const eol = doc.eol === api.EndOfLine.CRLF ? "\r\n" : "\n";
    assert.equal(await editor.edit(edit => edit.insert(doc.positionAt(doc.getText().length), "// unsaved user note" + eol)), true);
    const tools = await connect(path.join(root, "connection"));
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
    await apply(original);
    await api.commands.executeCommand("undo");
    assert.equal((await inspect()).source, original.source, "Native undo must preserve the earlier user note");
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
