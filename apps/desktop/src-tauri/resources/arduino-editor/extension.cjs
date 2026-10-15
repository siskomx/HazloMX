"use strict";
const vscode = require("vscode");
const crypto = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");
const os = require("node:os");
const {TextDecoder} = require("node:util");
const {createEditorTools} = require("./core.cjs");

function connectionDirectory() {
  try {
    let root = process.env.HAZLOMEXA_AGENT_EDITOR_ROOT;
    if (!root) {
      const config = path.join(os.homedir(), ".arduinoIDE");
      if (!/^[a-z]:[\\/]/i.test(config) || !fs.lstatSync(config).isDirectory()
          || fs.realpathSync(config).toLowerCase() !== path.resolve(config).toLowerCase()) throw Error("profile");
      root = path.join(config, "hazlomexa-editor");
      try { fs.mkdirSync(root, {mode:0o700}); } catch (error) { if (error.code !== "EEXIST") throw error; }
    }
    if (!/^[a-z]:[\\/]/i.test(root) || !path.isAbsolute(root) || !fs.lstatSync(root).isDirectory()
        || fs.realpathSync(root).toLowerCase() !== path.resolve(root).toLowerCase()) throw Error("directory");
    return root;
  } catch { throw Error("Native editor connection directory is invalid."); }
}

function activate(context) {
  if (process.platform !== "win32" || vscode.env?.appName !== "Arduino Agent") return;
  const root = connectionDirectory();
  const tools = createEditorTools(vscode), token = crypto.randomBytes(32).toString("hex");
  const pipe = "\\\\.\\pipe\\hazlomexa-arduino-" + tools.instance_id;
  const discovery = path.join(root, tools.instance_id + ".json");
  const server = net.createServer(socket => {
    let chunks = [], size = 0, handling = false;
    const deadline = setTimeout(() => socket.destroy(), 5000);
    socket.on("close", () => clearTimeout(deadline));
    socket.setTimeout(5000, () => socket.destroy());
    socket.on("error", () => {});
    socket.on("data", async chunk => {
      if (handling || (size += chunk.length) > 512 * 1024) { socket.destroy(); return; }
      chunks.push(chunk);
      if (!chunk.includes(10)) return;
      handling = true;
      try {
        const text = new TextDecoder("utf-8", {fatal:true}).decode(Buffer.concat(chunks));
        chunks = [];
        const message = JSON.parse(text);
        if (Object.keys(message).sort().join("|") !== "request|token" || typeof message.token !== "string"
            || message.token.length !== token.length
            || !crypto.timingSafeEqual(Buffer.from(message.token), Buffer.from(token))) throw Error("auth");
        clearTimeout(deadline);
        socket.setTimeout(15000, () => socket.destroy());
        const result = await tools.request(message.request);
        if (!socket.destroyed) socket.end(JSON.stringify(result) + "\n");
      } catch {
        if (!socket.destroyed) socket.end('{"ok":false,"outcome":"refused","error":"native_editor_not_confirmed"}\n');
      }
    });
  });
  server.maxConnections = 8;
  server.on("error", () => {});
  server.listen(pipe, () => {
    try {
      fs.writeFileSync(discovery, JSON.stringify({editor_version:1, instance_id:tools.instance_id, pipe, token}), {flag:"wx", mode:0o600});
    } catch { server.close(); }
  });
  context.subscriptions.push({dispose() {
    server.close();
    try { if (JSON.parse(fs.readFileSync(discovery, "utf8")).token === token) fs.unlinkSync(discovery); } catch {}
  }});
}
module.exports = {activate};
