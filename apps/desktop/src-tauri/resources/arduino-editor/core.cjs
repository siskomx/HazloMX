"use strict";
const crypto = require("node:crypto");
const path = require("node:path");

const LIMIT = 65536;
const hash = text => crypto.createHash("sha256").update(text, "utf8").digest("hex");
const uuid = value => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
function fields(value, expected) {
  return value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).sort().join("|") === expected.slice().sort().join("|");
}

function createEditorTools(api) {
  const instance = crypto.randomUUID(), documents = new WeakMap(), attempts = new Map();
  let busy = false;
  const identity = input => hash(JSON.stringify(Object.keys(input).sort().map(key=>[key,input[key]])));
  function reserve(input) {
    if (attempts.has(input.job_id) || attempts.size >= 128) throw Error("attempt");
    attempts.set(input.job_id,{identity:identity(input),status:"running"});
  }
  function completed(input, result) {
    const reply={ok:true,result};
    attempts.set(input.job_id,{identity:identity(input),status:"completed",reply:JSON.stringify(reply)});
    return reply;
  }

  function observe(editor) {
    const doc = editor?.document;
    if (!doc || doc.isClosed || doc.uri.scheme !== "file"
        || !/^[a-z]:[\\/]/i.test(doc.uri.fsPath) || path.extname(doc.uri.fsPath).toLowerCase() !== ".ino"
        || typeof doc.isDirty !== "boolean" || !Number.isSafeInteger(doc.version) || doc.version < 1 || ![1, 2].includes(doc.eol)) throw Error("document");
    const source = doc.getText();
    if (typeof source !== "string" || Buffer.byteLength(source, "utf8") > LIMIT || source.includes("\0")) throw Error("limit");
    if (!documents.has(doc)) documents.set(doc, crypto.randomUUID());
    return {editor_version:1, instance_id:instance, document_token:documents.get(doc),
      name:path.basename(doc.uri.fsPath), version:doc.version, state_sha256:hash(source),
      source, eol:doc.eol, dirty:doc.isDirty, source_is_untrusted_data:true};
  }

  function projectBuffers(input) {
    const editor = api.window.activeTextEditor, before = observe(editor);
    for (const key of ["instance_id", "document_token", "version", "state_sha256"]) {
      if (input[key] !== before[key]) throw Error("stale");
    }
    const folder = api.workspace?.getWorkspaceFolder(editor.document.uri);
    const root = folder?.uri?.fsPath;
    const openDocuments = api.workspace?.textDocuments;
    if (folder?.uri?.scheme !== "file" || folder.uri.authority || typeof root !== "string"
        || !/^[a-z]:[\\/]/i.test(root) || path.win32.normalize(root) !== root
        || path.win32.dirname(editor.document.uri.fsPath).toLowerCase() !== root.toLowerCase()
        || !Array.isArray(openDocuments) || openDocuments.length > 256) throw Error("workspace");
    const snapshots = [], names = new Set();
    let total = 0;
    for (const doc of openDocuments) {
      if (doc.isClosed || doc.uri?.scheme !== "file") continue;
      const relative = path.win32.relative(root, doc.uri.fsPath);
      if (relative === ".." || relative.startsWith("..\\") || path.win32.isAbsolute(relative)) continue;
      if (!relative || doc.uri.authority || path.win32.normalize(doc.uri.fsPath) !== doc.uri.fsPath
          || relative.split("\\").some(part => !part || /[\x00-\x1f<>:"|?*]/.test(part) || /[. ]$/.test(part))
          || names.has(relative.toLowerCase()) || snapshots.length >= 32
          || typeof doc.isDirty !== "boolean" || !Number.isSafeInteger(doc.version) || doc.version < 1
          || ![1, 2].includes(doc.eol)) throw Error("buffer");
      const source = doc.getText();
      if (typeof source !== "string" || source.includes("\0") || Buffer.byteLength(source,"utf8") > LIMIT
          || (total += Buffer.byteLength(source,"utf8")) > 128 * 1024
          || (doc.eol === 1 ? source.includes("\r") : /(^|[^\r])\n|\r(?!\n)/.test(source))) throw Error("limit");
      if (!documents.has(doc)) documents.set(doc, crypto.randomUUID());
      names.add(relative.toLowerCase());
      snapshots.push({relative_path:relative.replaceAll("\\","/"), document_token:documents.get(doc),
        version:doc.version, state_sha256:hash(source), source, eol:doc.eol, dirty:doc.isDirty});
    }
    snapshots.sort((a,b)=>a.relative_path < b.relative_path ? -1 : a.relative_path > b.relative_path ? 1 : 0);
    const after = observe(api.window.activeTextEditor);
    if (JSON.stringify(before) !== JSON.stringify(after)
        || !snapshots.some(item => item.document_token === before.document_token && item.source === before.source
          && item.version === before.version && item.dirty === before.dirty && item.eol === before.eol)) throw Error("changed");
    const result = {project_buffers_version:1, instance_id:instance, active_document_token:before.document_token,
      root, buffers:snapshots};
    // Bound the encoded reply as well as source bytes; escaped control characters can expand it.
    if (Buffer.byteLength(JSON.stringify(result),"utf8") > 384 * 1024) throw Error("limit");
    return result;
  }

  async function request(input) {
    let sent = false, confirmedRefusal = false, ownsBusy = false;
    try {
      if (fields(input, ["action"]) && input.action === "inspect") {
        return {ok:true, result:observe(api.window.activeTextEditor)};
      }
      if (fields(input,["action","instance_id","document_token","version","state_sha256"])
          && input.action === "inspect_project_buffers" && !busy) {
        return {ok:true,result:projectBuffers(input)};
      }
      if (fields(input,["action","original"]) && input.action === "recover") {
        const original=input.original;
        const expected=["action","job_id","instance_id","document_token","version","state_sha256"];
        if (original?.action === "apply") expected.push("source");
        if (!fields(original,expected) || !["apply","save"].includes(original.action) || original.instance_id!==instance) throw Error("request");
        const previous=attempts.get(original.job_id);
        if (!previous || previous.identity!==identity(original)) throw Error("attempt");
        if (previous.status === "running") return {ok:true,result:{editor_recovery_version:1,status:"running"}};
        if (previous.status !== "completed") throw Error("uncertain");
        return JSON.parse(previous.reply);
      }
      if (fields(input, ["action", "job_id", "instance_id", "document_token", "version", "state_sha256"])
          && input.action === "save" && uuid(input.job_id) && !busy) {
        const editor = api.window.activeTextEditor, before = observe(editor);
        if (input.instance_id !== before.instance_id || input.document_token !== before.document_token
            || input.version !== before.version || input.state_sha256 !== before.state_sha256
            || !before.dirty || typeof editor.document.save !== "function") throw Error("stale");
        reserve(input);
        busy = true; ownsBusy = true; sent = true;
        // Saving is separate from editing; never replace the buffer or choose a new destination.
        const saved = await editor.document.save();
        let after = observe(editor);
        for (let attempt = 0; saved === true && attempt < 100 && after.dirty
            && after.document_token === before.document_token && after.name === before.name
            && after.eol === before.eol && after.version === before.version && after.source === before.source; attempt++) {
          await new Promise(resolve => setTimeout(resolve, 20));
          after = observe(editor);
        }
        if (saved !== true || after.document_token !== before.document_token || after.name !== before.name
            || after.eol !== before.eol || after.version !== before.version || after.source !== before.source
            || after.dirty) throw Error("save_readback");
        return completed(input,{editor_version:1, job_id:input.job_id, status:"completed",
          before_sha256:before.state_sha256, after, native_edit_confirmed:false, native_save_confirmed:true});
      }
      if (!fields(input, ["action", "job_id", "instance_id", "document_token", "version", "state_sha256", "source"])
          || input.action !== "apply" || !uuid(input.job_id) || busy) throw Error("request");
      const editor = api.window.activeTextEditor, before = observe(editor);
      if (input.instance_id !== before.instance_id || input.document_token !== before.document_token
          || input.version !== before.version || input.state_sha256 !== before.state_sha256
          || typeof input.source !== "string" || input.source.includes("\0")
          || Buffer.byteLength(input.source, "utf8") > LIMIT || input.source === before.source
          || (before.eol === 1 ? input.source.includes("\r") : /(^|[^\r])\n|\r(?!\n)/.test(input.source))) throw Error("stale");
      reserve(input);
      busy = true; ownsBusy = true;
      const range = new api.Range(editor.document.positionAt(0), editor.document.positionAt(before.source.length));
      sent = true;
      const applied = await editor.edit(edit => edit.replace(range, input.source), {undoStopBefore:true, undoStopAfter:true});
      if (!applied) { confirmedRefusal = true; throw Error("native_refused"); }
      let after = observe(editor);
      // The IDE may acknowledge the edit before its document-change notification reaches this host.
      for (let attempt = 0; attempt < 100 && after.document_token === before.document_token
          && after.name === before.name && after.eol === before.eol
          && after.version === before.version && after.source === before.source; attempt++) {
        await new Promise(resolve => setTimeout(resolve, 20));
        after = observe(editor);
      }
      if (after.document_token !== before.document_token || after.name !== before.name || after.eol !== before.eol
          || after.version <= before.version || after.source !== input.source) throw Error("readback");
      return completed(input,{editor_version:1, job_id:input.job_id, status:"completed",
        before_sha256:before.state_sha256, after, native_edit_confirmed:true});
    } catch {
      if (ownsBusy) attempts.get(input.job_id).status=sent && !confirmedRefusal ? "uncertain" : "refused";
      return {ok:false, outcome:sent && !confirmedRefusal ? "uncertain" : "refused", error:"native_editor_not_confirmed"};
    } finally {
      if (ownsBusy) busy = false;
    }
  }
  return {instance_id:instance, request};
}

module.exports = {createEditorTools};
