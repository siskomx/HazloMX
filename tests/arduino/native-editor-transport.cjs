"use strict";
const {test} = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs/promises");
const path = require("node:path");
const net = require("node:net");
const Module = require("node:module");

test("the actual named-pipe entry authenticates before reading or editing a native buffer", async () => {
  const target = path.resolve(__dirname, "../../target");
  const root = path.join(target, "native-arduino-transport-" + crypto.randomUUID());
  await fs.mkdir(root, {recursive:true});
  const env = process.env.HAZLOMEXA_AGENT_EDITOR_ROOT, loader = Module._load;
  let reads = 0, edits = 0;
  const doc = {uri:{scheme:"file", fsPath:"C:\\owned\\Sketch.ino"}, version:1, eol:1, isDirty:true, isClosed:false,
    source:"// unsaved user note\n", getText() { reads++; return this.source; }, positionAt(offset) { return offset; }};
  const editor = {document:doc, async edit(callback) {
    edits++; callback({replace(range, source) {doc.source = source;}}); doc.version++; return true;
  }};
  const api = {window:{activeTextEditor:editor}, Range:class {}};
  const context = {subscriptions:[]};
  try {
    process.env.HAZLOMEXA_AGENT_EDITOR_ROOT = await fs.realpath(root);
    Module._load = function(name, ...args) { return name === "vscode" ? api : loader.call(this, name, ...args); };
    const extension = require("../../apps/desktop/src-tauri/resources/arduino-editor/extension.cjs");
    Module._load = loader;
    extension.activate(context);
    let names = [];
    for (let attempt = 0; attempt < 100; attempt++) {
      names = await fs.readdir(root); if (names.length) break;
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert.equal(names.length, 1);
    const connection = JSON.parse(await fs.readFile(path.join(root, names[0]), "utf8"));
    assert.equal(connection.editor_version, 1);
    function send(message, raw = false) {
      return new Promise((resolve, reject) => {
        const socket = net.connect(connection.pipe); let output = "";
        socket.setTimeout(3000, () => socket.destroy(Error("Private fixture reply timeout")));
        socket.on("error", reject);
        socket.on("connect", () => socket.write(raw ? message : JSON.stringify(message) + "\n"));
        socket.on("data", chunk => { output += chunk.toString("utf8"); });
        socket.on("end", () => { try { resolve(JSON.parse(output)); } catch { reject(Error("Invalid fixture reply")); } });
      });
    }
    assert.equal((await send({token:"0".repeat(64), request:{action:"inspect"}})).ok, false);
    assert.equal((await send(Buffer.from([255, 10]), true)).ok, false);
    assert.equal(reads, 0); assert.equal(edits, 0);
    const first = await send({token:connection.token, request:{action:"inspect"}});
    assert.equal(first.ok, true); assert.equal(first.result.source, doc.source);
    assert(!JSON.stringify(first).includes("C:"));
    const request = {action:"apply", job_id:crypto.randomUUID(), instance_id:first.result.instance_id,
      document_token:first.result.document_token, version:first.result.version,
      state_sha256:first.result.state_sha256, source:doc.source + "void setup() {}\n"};
    assert.equal((await send({token:connection.token, request})).ok, true);
    assert.equal((await send({token:connection.token, request})).outcome, "refused");
    assert.equal((await send({token:connection.token, request:{...request, path:"C:/another.ino"}})).ok, false);
    assert.equal(edits, 1); assert(doc.source.startsWith("// unsaved user note"));
  } finally {
    Module._load = loader;
    for (const item of context.subscriptions) item.dispose();
    if (env === undefined) delete process.env.HAZLOMEXA_AGENT_EDITOR_ROOT;
    else process.env.HAZLOMEXA_AGENT_EDITOR_ROOT = env;
    assert((await fs.realpath(root)).startsWith((await fs.realpath(target)) + path.sep));
    await fs.rm(root, {recursive:true, force:true});
  }
});
